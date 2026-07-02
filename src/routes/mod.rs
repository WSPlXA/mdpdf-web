use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    model::{
        ConvertJob, ConvertResponse, FileResponse, JobStatus, PreviewResponse, RenderRequest,
        UploadedFile,
    },
    service::{markdown::render_markdown_file, pdf::write_pdf},
    state::{is_path_inside, AppState},
};

const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

pub async fn files(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<FileResponse>> {
    let mut saved: Option<UploadedFile> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid multipart payload: {err}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original = field.file_name().unwrap_or("upload.md").to_string();
        validate_markdown_filename(&original)?;

        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::BadRequest(format!("failed to read upload: {err}")))?;
        if bytes.is_empty() {
            return Err(AppError::BadRequest("uploaded file is empty".into()));
        }
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(AppError::BadRequest("uploaded file exceeds 10 MiB".into()));
        }

        let id = format!("f_{}", Uuid::new_v4().simple());
        let path = state.files_dir.join(format!("{id}.md"));
        tokio::fs::write(&path, &bytes).await?;

        let file = UploadedFile {
            id: id.clone(),
            filename: sanitize_display_name(&original),
            path,
            size: bytes.len() as u64,
            created_at: Utc::now(),
        };
        saved = Some(file);
        break;
    }

    let file = saved.ok_or_else(|| AppError::BadRequest("missing multipart field: file".into()))?;
    let response = FileResponse {
        file_id: file.id.clone(),
        filename: file.filename.clone(),
        size: file.size,
    };
    state.files.write().await.insert(file.id.clone(), file);
    Ok(Json(response))
}

pub async fn preview(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RenderRequest>,
) -> Result<Json<PreviewResponse>> {
    let (markdown, filename) = match &req.markdown_content {
        Some(content) => {
            let name = req.filename.clone().unwrap_or_else(|| "document.md".to_string());
            (content.clone(), name)
        }
        None => {
            let file_id = req.file_id.as_deref().ok_or_else(|| AppError::BadRequest("missing file_id or markdown_content".into()))?;
            let file = get_file(&state, file_id).await?;
            let markdown = tokio::fs::read_to_string(&file.path).await?;
            (markdown, file.filename)
        }
    };
    let render = render_markdown_file(&state, &markdown, &filename, &req, None).await?;
    Ok(Json(PreviewResponse {
        html: render.html,
        warnings: render.warnings,
        logs: render.logs,
    }))
}

pub async fn convert(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RenderRequest>,
) -> Result<Json<ConvertResponse>> {
    let (markdown, filename, file_id) = match &req.markdown_content {
        Some(content) => {
            let name = req.filename.clone().unwrap_or_else(|| "document.md".to_string());
            (content.clone(), name, "".to_string())
        }
        None => {
            let fid = req.file_id.as_deref().ok_or_else(|| AppError::BadRequest("missing file_id or markdown_content".into()))?;
            let file = get_file(&state, fid).await?;
            let markdown = tokio::fs::read_to_string(&file.path).await?;
            (markdown, file.filename, file.id)
        }
    };
    
    let job_id = format!("job_{}", Uuid::new_v4().simple());
    let job = ConvertJob {
        id: job_id.clone(),
        file_id,
        theme: req.theme.clone(),
        status: JobStatus::Pending,
        pdf_url: None,
        warnings: Vec::new(),
        logs: vec!["queued".into()],
        error_message: None,
        created_at: Utc::now(),
        pdf_path: None,
        html_path: None,
    };
    state.jobs.write().await.insert(job_id.clone(), job);

    let response_job_id = job_id.clone();
    let background_state = state.clone();
    tokio::spawn(async move {
        run_convert_job(background_state, job_id, markdown, filename, req).await;
    });

    Ok(Json(ConvertResponse {
        job_id: response_job_id,
        status: JobStatus::Pending,
    }))
}

pub async fn job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<ConvertJob>> {
    let job = state
        .jobs
        .read()
        .await
        .get(&job_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("job not found: {job_id}")))?;
    Ok(Json(job))
}

#[derive(serde::Deserialize)]
pub struct DownloadQuery {
    #[serde(default)]
    inline: bool,
}

pub async fn download(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DownloadQuery>,
) -> Result<Response> {
    let job = state
        .jobs
        .read()
        .await
        .get(&job_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("job not found: {job_id}")))?;
    if job.status != JobStatus::Succeeded {
        return Err(AppError::BadRequest("job has not succeeded".into()));
    }
    let path = job
        .pdf_path
        .ok_or_else(|| AppError::NotFound("job has no PDF path".into()))?;
    if !is_path_inside(&state.jobs_dir, &path) {
        return Err(AppError::BadRequest("invalid PDF path".into()));
    }
    let bytes = tokio::fs::read(path).await?;
    let filename = format!("{job_id}.pdf");
    
    let disposition = if query.inline {
        format!("inline; filename=\"{filename}\"")
    } else {
        format!("attachment; filename=\"{filename}\"")
    };

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn get_file(state: &AppState, file_id: &str) -> Result<UploadedFile> {
    state
        .files
        .read()
        .await
        .get(file_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("file not found: {file_id}")))
}

async fn run_convert_job(
    state: Arc<AppState>,
    job_id: String,
    markdown: String,
    filename: String,
    req: RenderRequest,
) {
    set_job_processing(&state, &job_id).await;

    let job_dir = state.job_dir(&job_id);
    let result = async {
        tokio::fs::create_dir_all(&job_dir).await?;
        let render = render_markdown_file(&state, &markdown, &filename, &req, Some(&job_dir)).await?;
        let html_path = job_dir.join("document.html");
        let pdf_path = job_dir.join("document.pdf");
        tokio::fs::write(&html_path, render.html.as_bytes()).await?;
        let mut logs = render.logs;
        write_pdf(&html_path, &pdf_path, &render.print_options, &mut logs).await?;
        Ok::<_, AppError>((html_path, pdf_path, render.warnings, logs))
    }
    .await;

    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(&job_id) {
        match result {
            Ok((html_path, pdf_path, warnings, mut logs)) => {
                logs.push("pdf generated".into());
                job.status = JobStatus::Succeeded;
                job.html_path = Some(html_path);
                job.pdf_path = Some(pdf_path);
                job.pdf_url = Some(format!("/api/jobs/{job_id}/download"));
                job.warnings = warnings;
                job.logs = logs;
            }
            Err(err) => {
                job.status = JobStatus::Failed;
                job.error_message = Some(err.to_string());
                job.logs.push(format!("failed: {err}"));
            }
        }
    }
}

async fn set_job_processing(state: &AppState, job_id: &str) {
    if let Some(job) = state.jobs.write().await.get_mut(job_id) {
        job.status = JobStatus::Processing;
        job.logs.push("processing".into());
    }
}

fn validate_markdown_filename(name: &str) -> Result<()> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "only .md and .markdown files are accepted".into(),
        ))
    }
}

fn sanitize_display_name(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or("upload.md")
        .chars()
        .filter(|ch| *ch != '\0')
        .collect()
}

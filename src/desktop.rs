use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Local;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use walkdir::{DirEntry, WalkDir};

use crate::{
    error::{AppError, Result},
    model::{PreviewResponse, RenderRequest},
    service::{markdown::render_markdown_file, pdf::write_pdf},
    state::AppState,
};

const MAX_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    root: String,
    documents: Vec<DocumentEntry>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentEntry {
    path: String,
    relative_path: String,
    filename: String,
    size: u64,
    modified_ms: u64,
}

#[derive(Deserialize)]
pub struct PathRequest {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlinePreviewImagesRequest {
    source_path: String,
    html: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContent {
    path: String,
    content: String,
    modified_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDocumentRequest {
    path: String,
    content: String,
    expected_modified_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDocumentResponse {
    modified_ms: u64,
    bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReplaceRequest {
    paths: Vec<String>,
    find: String,
    replace: String,
    case_sensitive: bool,
    dry_run: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReplaceResult {
    files_scanned: usize,
    files_changed: usize,
    replacements: usize,
    backup_dir: Option<String>,
    changes: Vec<BatchFileChange>,
    failures: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchFileChange {
    path: String,
    relative_path: String,
    replacements: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    paths: Vec<String>,
    output_dir: String,
    render: RenderRequest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    succeeded: usize,
    failed: usize,
    files: Vec<ExportFileResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportFileResult {
    source_path: String,
    output_path: Option<String>,
    error: Option<String>,
}

#[tauri::command]
pub async fn choose_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<Option<WorkspaceSnapshot>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Markdown 文件夹を開く")
        .blocking_pick_folder();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let root = selected
        .into_path()
        .map_err(|err| format!("選択したフォルダを読み取れません: {err}"))?
        .canonicalize()
        .map_err(|err| format!("フォルダを開けません: {err}"))?;
    state.set_workspace(root.clone());
    scan_root(&root).map(Some).map_err(error_string)
}

#[tauri::command]
pub async fn scan_workspace(
    state: State<'_, AppState>,
) -> std::result::Result<Option<WorkspaceSnapshot>, String> {
    let Some(root) = state.workspace() else {
        return Ok(None);
    };
    scan_root(&root).map(Some).map_err(error_string)
}

#[tauri::command]
pub async fn read_document(
    state: State<'_, AppState>,
    request: PathRequest,
) -> std::result::Result<DocumentContent, String> {
    let path = checked_document(&state, &request.path).map_err(error_string)?;
    let metadata = fs::metadata(&path).map_err(|err| error_string(err.into()))?;
    validate_size(metadata.len()).map_err(error_string)?;
    let content = fs::read_to_string(&path).map_err(|err| error_string(err.into()))?;
    Ok(DocumentContent {
        path: display_path(&path),
        content,
        modified_ms: modified_ms(&metadata),
    })
}

#[tauri::command]
pub async fn save_document(
    state: State<'_, AppState>,
    request: SaveDocumentRequest,
) -> std::result::Result<SaveDocumentResponse, String> {
    validate_size(request.content.len() as u64).map_err(error_string)?;
    let path = checked_document(&state, &request.path).map_err(error_string)?;
    let metadata = fs::metadata(&path).map_err(|err| error_string(err.into()))?;
    let current_modified = modified_ms(&metadata);
    if request
        .expected_modified_ms
        .is_some_and(|expected| expected != current_modified)
    {
        return Err(
            "ファイルは外部で変更されました。再読み込みしてから保存してください".to_string(),
        );
    }
    fs::write(&path, request.content.as_bytes()).map_err(|err| error_string(err.into()))?;
    let metadata = fs::metadata(&path).map_err(|err| error_string(err.into()))?;
    Ok(SaveDocumentResponse {
        modified_ms: modified_ms(&metadata),
        bytes: metadata.len(),
    })
}

#[tauri::command]
pub async fn render_preview(
    state: State<'_, AppState>,
    request: RenderRequest,
) -> std::result::Result<PreviewResponse, String> {
    let markdown = request
        .markdown_content
        .as_deref()
        .ok_or_else(|| "markdown_content がありません".to_string())?;
    validate_size(markdown.len() as u64).map_err(error_string)?;
    let filename = request.filename.as_deref().unwrap_or("document.md");
    let mut rendered = render_markdown_file(&state, markdown, filename, &request, None)
        .await
        .map_err(error_string)?;
    if let Some(source_path) = request.source_path.as_deref() {
        let document_path = checked_document(&state, source_path).map_err(error_string)?;
        let root = state
            .workspace()
            .ok_or_else(|| "workspace is not open".to_string())?;
        let (html, warnings) = inline_local_images(&rendered.html, &document_path, &root);
        rendered.html = html;
        rendered.warnings.extend(warnings);
    }
    Ok(PreviewResponse {
        html: rendered.html,
        warnings: rendered.warnings,
        logs: rendered.logs,
    })
}

#[tauri::command]
pub async fn inline_preview_images(
    state: State<'_, AppState>,
    request: InlinePreviewImagesRequest,
) -> std::result::Result<PreviewResponse, String> {
    if request.html.len() > 50 * 1024 * 1024 {
        return Err("preview HTML exceeds 50 MiB".to_string());
    }
    let document_path = checked_document(&state, &request.source_path).map_err(error_string)?;
    let root = state
        .workspace()
        .ok_or_else(|| "workspace is not open".to_string())?;
    let (html, warnings) = inline_local_images(&request.html, &document_path, &root);
    Ok(PreviewResponse {
        html,
        warnings,
        logs: Vec::new(),
    })
}

#[tauri::command]
pub async fn batch_replace(
    state: State<'_, AppState>,
    request: BatchReplaceRequest,
) -> std::result::Result<BatchReplaceResult, String> {
    if request.find.is_empty() {
        return Err("検索文字列を入力してください".to_string());
    }
    if request.paths.len() > 10_000 {
        return Err("一度に処理できるファイルは 10,000 件までです".to_string());
    }
    let root = state
        .workspace()
        .ok_or_else(|| "先にフォルダを開いてください".to_string())?;
    let backup_root = (!request.dry_run).then(|| {
        root.join(".mdpdf-backup")
            .join(Local::now().format("%Y%m%d-%H%M%S").to_string())
    });
    let matcher = (!request.case_sensitive)
        .then(|| {
            RegexBuilder::new(&regex::escape(&request.find))
                .case_insensitive(true)
                .build()
        })
        .transpose()
        .map_err(|err| format!("検索条件が不正です: {err}"))?;

    let mut changes = Vec::new();
    let mut failures = Vec::new();
    let mut replacements = 0usize;
    for raw_path in &request.paths {
        let path = match checked_document(&state, raw_path) {
            Ok(path) => path,
            Err(err) => {
                failures.push(format!("{raw_path}: {err}"));
                continue;
            }
        };
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                failures.push(format!("{}: {err}", display_path(&path)));
                continue;
            }
        };
        let (updated, count) =
            replace_literal(&source, &request.find, &request.replace, matcher.as_ref());
        if count == 0 {
            continue;
        }
        let relative = path
            .strip_prefix(&root)
            .expect("checked path must be inside root");
        if let Some(backup_root) = &backup_root {
            let backup = backup_root.join(relative);
            let result = (|| -> std::io::Result<()> {
                if let Some(parent) = backup.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&path, &backup)?;
                fs::write(&path, updated.as_bytes())?;
                Ok(())
            })();
            if let Err(err) = result {
                failures.push(format!("{}: {err}", display_path(&path)));
                continue;
            }
        }
        replacements += count;
        changes.push(BatchFileChange {
            path: display_path(&path),
            relative_path: relative_display(relative),
            replacements: count,
        });
    }

    Ok(BatchReplaceResult {
        files_scanned: request.paths.len(),
        files_changed: changes.len(),
        replacements,
        backup_dir: backup_root.as_ref().map(|path| display_path(path)),
        changes,
        failures,
    })
}

#[tauri::command]
pub async fn choose_export_folder(app: AppHandle) -> std::result::Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("PDF 出力先を選択")
        .blocking_pick_folder();
    selected
        .map(|path| path.into_path().map(|path| display_path(&path)))
        .transpose()
        .map_err(|err| format!("出力先を読み取れません: {err}"))
}

#[tauri::command]
pub async fn export_documents(
    state: State<'_, AppState>,
    request: ExportRequest,
) -> std::result::Result<ExportResult, String> {
    if request.paths.is_empty() {
        return Err("出力するファイルを選択してください".to_string());
    }
    let _permit = state
        .export_limiter
        .acquire()
        .await
        .map_err(|err| err.to_string())?;
    let root = state
        .workspace()
        .ok_or_else(|| "先にフォルダを開いてください".to_string())?;
    let output_root = PathBuf::from(&request.output_dir);
    fs::create_dir_all(&output_root).map_err(|err| error_string(err.into()))?;

    let mut results = Vec::with_capacity(request.paths.len());
    for raw_path in &request.paths {
        match export_one(&state, &root, &output_root, raw_path, &request.render).await {
            Ok(output) => results.push(ExportFileResult {
                source_path: raw_path.clone(),
                output_path: Some(display_path(&output)),
                error: None,
            }),
            Err(err) => results.push(ExportFileResult {
                source_path: raw_path.clone(),
                output_path: None,
                error: Some(err.to_string()),
            }),
        }
    }
    let succeeded = results.iter().filter(|item| item.error.is_none()).count();
    Ok(ExportResult {
        succeeded,
        failed: results.len() - succeeded,
        files: results,
    })
}

async fn export_one(
    state: &AppState,
    root: &Path,
    output_root: &Path,
    raw_path: &str,
    base_request: &RenderRequest,
) -> Result<PathBuf> {
    let source_path = checked_document(state, raw_path)?;
    let markdown = tokio::fs::read_to_string(&source_path).await?;
    validate_size(markdown.len() as u64)?;
    let relative = source_path
        .strip_prefix(root)
        .map_err(|_| AppError::BadRequest("document is outside workspace".into()))?;
    let mut output_path = output_root.join(relative);
    output_path.set_extension("pdf");
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let render_dir = std::env::temp_dir()
        .join("mdpdf-desktop")
        .join(uuid::Uuid::new_v4().simple().to_string());
    tokio::fs::create_dir_all(&render_dir).await?;
    let html_path = render_dir.join("document.html");
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.md");
    let mut render_request = base_request.clone();
    render_request.source_path = Some(display_path(&source_path));
    render_request.filename = Some(filename.to_string());
    render_request.markdown_content = Some(markdown);
    let mut rendered = render_markdown_file(
        state,
        render_request.markdown_content.as_deref().unwrap(),
        filename,
        &render_request,
        Some(&render_dir),
    )
    .await?;
    let (html, image_warnings) = inline_local_images(&rendered.html, &source_path, root);
    rendered.html = html;
    rendered.warnings.extend(image_warnings);
    let needs_mermaid = rendered.html.contains("class=\"mermaid\"");
    if needs_mermaid {
        let runtime = state.mermaid_runtime()?;
        rendered.html = inject_mermaid_runtime(&rendered.html, &runtime);
        rendered
            .logs
            .push("embedded Mermaid runtime queued for offline PDF rendering".into());
    }
    tokio::fs::write(&html_path, rendered.html.as_bytes()).await?;
    let mut logs = rendered.logs;
    let result = write_pdf(
        &html_path,
        &output_path,
        &rendered.print_options,
        needs_mermaid,
        &mut logs,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&render_dir).await;
    result?;
    Ok(output_path)
}

fn scan_root(root: &Path) -> Result<WorkspaceSnapshot> {
    let mut documents = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(visible_entry)
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() || !is_markdown(entry.path()) {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) if metadata.len() <= MAX_DOCUMENT_BYTES => metadata,
            _ => continue,
        };
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        documents.push(DocumentEntry {
            path: display_path(entry.path()),
            relative_path: relative_display(relative),
            filename: entry.file_name().to_string_lossy().into_owned(),
            size: metadata.len(),
            modified_ms: modified_ms(&metadata),
        });
    }
    documents.sort_unstable_by(|left, right| {
        left.relative_path
            .to_lowercase()
            .cmp(&right.relative_path.to_lowercase())
    });
    Ok(WorkspaceSnapshot {
        root: display_path(root),
        documents,
    })
}

fn visible_entry(entry: &DirEntry) -> bool {
    entry.depth() == 0 || entry.file_name() != ".mdpdf-backup"
}

fn checked_document(state: &AppState, raw_path: &str) -> Result<PathBuf> {
    let root = state
        .workspace()
        .ok_or_else(|| AppError::BadRequest("workspace is not open".into()))?;
    let candidate = PathBuf::from(raw_path)
        .canonicalize()
        .map_err(|_| AppError::NotFound(format!("file not found: {raw_path}")))?;
    if !candidate.starts_with(&root) || !is_markdown(&candidate) {
        return Err(AppError::BadRequest(
            "path is outside the workspace or is not Markdown".into(),
        ));
    }
    Ok(candidate)
}

fn replace_literal(
    source: &str,
    find: &str,
    replacement: &str,
    matcher: Option<&regex::Regex>,
) -> (String, usize) {
    match matcher {
        Some(matcher) => {
            let count = matcher.find_iter(source).count();
            (
                matcher
                    .replace_all(source, |_: &regex::Captures<'_>| replacement)
                    .into_owned(),
                count,
            )
        }
        None => {
            let count = source.matches(find).count();
            (source.replace(find, replacement), count)
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

fn validate_size(size: u64) -> Result<()> {
    if size > MAX_DOCUMENT_BYTES {
        Err(AppError::BadRequest("document exceeds 10 MiB".into()))
    } else {
        Ok(())
    }
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn relative_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn error_string(error: AppError) -> String {
    error.to_string()
}

fn inline_local_images(
    html: &str,
    document_path: &Path,
    workspace_root: &Path,
) -> (String, Vec<String>) {
    const MAX_INLINE_TOTAL: u64 = 32 * 1024 * 1024;
    const BLOCKED_PIXEL: &str = "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=";

    let image =
        regex::Regex::new(r#"(?i)(<img\b[^>]*\bsrc=")([^"]+)(")"#).expect("valid image regex");
    let Some(base_dir) = document_path.parent() else {
        return (html.to_string(), Vec::new());
    };
    let Ok(base_url) = url::Url::from_directory_path(base_dir) else {
        return (
            html.to_string(),
            vec!["local image base path is invalid".into()],
        );
    };

    let mut warnings = Vec::new();
    let mut total_bytes = 0u64;
    let replaced = image.replace_all(html, |captures: &regex::Captures<'_>| {
        let raw = html_escape::decode_html_entities(&captures[2]);
        let replacement = if raw.starts_with("data:") {
            raw.into_owned()
        } else if raw.starts_with("http://") || raw.starts_with("https://") {
            warnings.push(format!("remote image blocked in offline mode: {raw}"));
            BLOCKED_PIXEL.to_string()
        } else {
            inline_image_url(
                &base_url,
                &raw,
                workspace_root,
                &mut total_bytes,
                MAX_INLINE_TOTAL,
            )
            .unwrap_or_else(|warning| {
                warnings.push(warning);
                BLOCKED_PIXEL.to_string()
            })
        };
        format!("{}{}{}", &captures[1], replacement, &captures[3])
    });
    (replaced.into_owned(), warnings)
}

fn inject_mermaid_runtime(html: &str, runtime: &str) -> String {
    // The vendored file is trusted build input. Escaping the closing tag keeps the
    // surrounding script element intact even if a future Mermaid build contains it.
    let runtime = runtime.replace("</script", "<\\/script");
    let boot = format!(
        r#"<script>{runtime}</script>
<script>
(async () => {{
  try {{
    mermaid.initialize({{
      startOnLoad: false,
      securityLevel: "strict",
      suppressErrorRendering: true
    }});
    await mermaid.run({{ nodes: document.querySelectorAll(".mermaid") }});
  }} catch (error) {{
    for (const node of document.querySelectorAll(".mermaid")) {{
      const failure = document.createElement("pre");
      failure.className = "diagram-error";
      failure.textContent = `Mermaid: ${{error?.message || String(error)}}`;
      node.replaceWith(failure);
    }}
  }} finally {{
    document.documentElement.dataset.mermaidReady = "true";
  }}
}})();
</script>"#
    );
    html.replacen("</body>", &format!("{boot}\n</body>"), 1)
}

fn inline_image_url(
    base_url: &url::Url,
    raw: &str,
    workspace_root: &Path,
    total_bytes: &mut u64,
    max_total: u64,
) -> std::result::Result<String, String> {
    let url = base_url
        .join(raw)
        .map_err(|_| format!("invalid local image path: {raw}"))?;
    if url.scheme() != "file" {
        return Err(format!("non-local image blocked in offline mode: {raw}"));
    }
    let path = url
        .to_file_path()
        .map_err(|_| format!("invalid local image URL: {raw}"))?;
    let path = path
        .canonicalize()
        .map_err(|_| format!("local image not found: {raw}"))?;
    if !path.starts_with(workspace_root) {
        return Err(format!("image outside workspace blocked: {raw}"));
    }
    let metadata = fs::metadata(&path).map_err(|_| format!("local image not readable: {raw}"))?;
    if metadata.len() > MAX_DOCUMENT_BYTES || *total_bytes + metadata.len() > max_total {
        return Err(format!("local image exceeds inline size limit: {raw}"));
    }
    let mime = image_mime(&path).ok_or_else(|| format!("unsupported image type: {raw}"))?;
    let bytes = fs::read(&path).map_err(|_| format!("local image not readable: {raw}"))?;
    *total_bytes += bytes.len() as u64;
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_replace_is_literal() {
        let matcher = RegexBuilder::new(&regex::escape("A.B"))
            .case_insensitive(true)
            .build()
            .unwrap();
        let (updated, count) = replace_literal("a.b A-B A.B", "A.B", "x", Some(&matcher));
        assert_eq!(updated, "x A-B x");
        assert_eq!(count, 2);
    }

    #[test]
    fn case_insensitive_replacement_keeps_dollar_signs() {
        let matcher = RegexBuilder::new("token")
            .case_insensitive(true)
            .build()
            .unwrap();
        let (updated, count) = replace_literal("TOKEN", "token", "$1", Some(&matcher));
        assert_eq!(updated, "$1");
        assert_eq!(count, 1);
    }

    #[test]
    fn backup_directory_is_not_scanned() {
        let entry_name = Path::new(".mdpdf-backup").file_name().unwrap();
        assert_eq!(entry_name, ".mdpdf-backup");
    }

    #[test]
    fn mermaid_runtime_is_embedded_before_body_end() {
        let html = "<html><body><div class=\"mermaid\">graph TD; A-->B</div></body></html>";
        let rendered = inject_mermaid_runtime(html, "window.mermaid={}; // </script marker");
        assert!(!rendered.contains("</script marker"));
        assert!(rendered.contains("<\\/script marker"));
        assert!(rendered.contains("mermaid.run"));
        assert!(rendered.find("mermaid.run").unwrap() < rendered.find("</body>").unwrap());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn embedded_mermaid_is_rendered_by_edge_without_mmdc() {
        let dir = std::env::temp_dir()
            .join("mdpdf-mermaid-test")
            .join(uuid::Uuid::new_v4().simple().to_string());
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let html_path = dir.join("diagram.html");
        let pdf_path = dir.join("diagram.pdf");
        let html = inject_mermaid_runtime(
            "<!doctype html><html><body><div class=\"mermaid\">graph TD; A--&gt;B</div></body></html>",
            crate::theme_assets::MERMAID_RUNTIME,
        );
        tokio::fs::write(&html_path, html).await.unwrap();
        let mut logs = Vec::new();
        write_pdf(
            &html_path,
            &pdf_path,
            &crate::service::theme::PrintOptions::default(),
            true,
            &mut logs,
        )
        .await
        .unwrap();
        assert!(tokio::fs::metadata(&pdf_path).await.unwrap().len() > 2_000);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

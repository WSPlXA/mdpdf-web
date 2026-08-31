use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::process::Command;

use crate::{
    error::{AppError, Result},
    service::{process::run_with_timeout, theme::PrintOptions},
};

pub async fn write_pdf(
    html_path: &Path,
    pdf_path: &Path,
    print_options: &PrintOptions,
    wait_for_scripts: bool,
    logs: &mut Vec<String>,
) -> Result<()> {
    let edge = find_edge()?;
    let source_url = url::Url::from_file_path(html_path)
        .map_err(|_| AppError::Conversion("failed to build local HTML URL".into()))?;
    let user_data_dir = std::env::temp_dir()
        .join("mdpdf-edge")
        .join(uuid::Uuid::new_v4().simple().to_string());
    tokio::fs::create_dir_all(&user_data_dir).await?;

    let mut command = Command::new(&edge);
    command
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-default-apps")
        .arg("--no-first-run")
        .arg("--no-proxy-server")
        .arg("--host-resolver-rules=MAP * 0.0.0.0")
        .arg("--print-to-pdf-no-header")
        .arg("--no-pdf-header-footer")
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg(format!("--print-to-pdf={}", pdf_path.display()));

    if wait_for_scripts {
        // Advance renderer time without sleeping for five wall-clock seconds. This
        // gives the bundled Mermaid promise time to replace source nodes with SVG.
        command.arg("--virtual-time-budget=5000");
        logs.push("offline script budget: 5000 ms virtual time".into());
    }
    command.arg(source_url.as_str());

    if print_options.display_header_footer {
        logs.push("page headers/footers: CSS @page margin boxes".into());
    }
    logs.push(format!("offline PDF renderer: {}", edge.display()));
    let result = run_with_timeout(command, 90).await?;
    if !result.status.success() {
        let _ = tokio::fs::remove_dir_all(&user_data_dir).await;
        return Err(AppError::Conversion(format!(
            "Edge PDF renderer failed: {}",
            String::from_utf8_lossy(&result.stderr)
                .lines()
                .take(12)
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    if !wait_for_pdf(pdf_path, Duration::from_secs(10)).await {
        let diagnostics = String::from_utf8_lossy(&result.stderr)
            .lines()
            .take(8)
            .collect::<Vec<_>>()
            .join("\n");
        let _ = tokio::fs::remove_dir_all(&user_data_dir).await;
        return Err(AppError::Conversion(format!(
            "Edge exited but PDF was not created: {diagnostics}"
        )));
    }
    let _ = tokio::fs::remove_dir_all(&user_data_dir).await;
    Ok(())
}

async fn wait_for_pdf(path: &Path, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::fs::metadata(path)
            .await
            .is_ok_and(|metadata| metadata.len() > 0)
        {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn find_edge() -> Result<PathBuf> {
    if let Ok(value) = std::env::var("MDPDF_EDGE") {
        if !value.trim().is_empty() {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    let mut candidates = Vec::with_capacity(4);
    for variable in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(variable) {
            candidates.push(
                PathBuf::from(base)
                    .join("Microsoft")
                    .join("Edge")
                    .join("Application")
                    .join("msedge.exe"),
            );
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::Conversion("Microsoft Edge was not found; set MDPDF_EDGE".into()))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn edge_writes_a_local_pdf_without_a_server() {
        let dir = std::env::temp_dir()
            .join("mdpdf-pdf-test")
            .join(uuid::Uuid::new_v4().simple().to_string());
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let html = dir.join("input.html");
        let pdf = dir.join("output.pdf");
        tokio::fs::write(
            &html,
            "<!doctype html><meta charset=\"utf-8\"><style>@page{size:A4}</style><h1>offline smoke</h1>",
        )
        .await
        .unwrap();

        let mut logs = Vec::new();
        write_pdf(&html, &pdf, &PrintOptions::default(), false, &mut logs)
            .await
            .unwrap();
        assert!(tokio::fs::metadata(&pdf).await.unwrap().len() > 1_000);
        assert!(logs
            .iter()
            .any(|line| line.contains("offline PDF renderer")));
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

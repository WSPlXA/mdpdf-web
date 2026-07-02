use std::path::Path;

use tokio::process::Command;

use crate::{
    error::{AppError, Result},
    service::{process::run_with_timeout, theme::PrintOptions},
};

pub async fn write_pdf(
    html_path: &Path,
    pdf_path: &Path,
    print_options: &PrintOptions,
    logs: &mut Vec<String>,
) -> Result<()> {
    let node = find_node_command()?;
    let print_script =
        std::env::var("MDPDF_PRINT_SCRIPT").unwrap_or_else(|_| "scripts/print_pdf.mjs".into());
    let options_path = pdf_path.with_file_name("print-options.json");
    let options_json = serde_json::to_vec(print_options)
        .map_err(|err| AppError::Conversion(format!("failed to serialize print options: {err}")))?;
    tokio::fs::write(&options_path, options_json).await?;

    let mut command = Command::new(&node);
    command
        .arg(&print_script)
        .arg(html_path)
        .arg(pdf_path)
        .arg(&options_path);

    logs.push(format!("print script: {node} {print_script}"));
    let result = run_with_timeout(command, 90).await?;
    if !result.status.success() {
        return Err(AppError::Conversion(format!(
            "pdf renderer failed: {}",
            String::from_utf8_lossy(&result.stderr)
                .lines()
                .take(12)
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    if tokio::fs::metadata(pdf_path).await.is_err() {
        return Err(AppError::Conversion("chromium exited but PDF was not created".into()));
    }
    Ok(())
}

fn find_node_command() -> Result<String> {
    if let Ok(value) = std::env::var("MDPDF_NODE") {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }
    let candidates: &[&str] = if cfg!(windows) { &["node.exe", "node"] } else { &["node"] };
    for candidate in candidates {
        if command_exists(candidate) {
            return Ok(candidate.to_string());
        }
    }
    Err(AppError::Conversion("Node executable not found; set MDPDF_NODE".into()))
}

fn command_exists(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

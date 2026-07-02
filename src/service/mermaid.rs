use std::path::Path;

use tokio::process::Command;

use crate::{
    error::{AppError, Result},
    service::process::run_with_timeout,
};

pub async fn render_mermaid_svg(index: usize, source: &str, dir: &Path) -> Result<String> {
    tokio::fs::create_dir_all(dir).await?;
    let input = dir.join(format!("diagram-{index:03}.mmd"));
    let output = dir.join(format!("diagram-{index:03}.svg"));
    tokio::fs::write(&input, source.as_bytes()).await?;

    let mut command = Command::new("mmdc");
    command
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--quiet")
        .arg("--backgroundColor")
        .arg("transparent")
        .arg("--theme")
        .arg("default");
    if let Ok(config) = std::env::var("MDPDF_PUPPETEER_CONFIG") {
        if !config.trim().is_empty() {
            command.arg("--puppeteerConfigFile").arg(config);
        }
    }

    let result = run_with_timeout(command, 20).await?;
    if !result.status.success() {
        return Err(AppError::Conversion(trim_process_error(&result.stderr)));
    }

    let svg = tokio::fs::read_to_string(&output).await?;
    Ok(strip_xml_header(svg))
}

fn trim_process_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines().take(8).collect::<Vec<_>>().join("\n")
}

fn strip_xml_header(svg: String) -> String {
    svg.lines()
        .filter(|line| !line.trim_start().starts_with("<?xml"))
        .collect::<Vec<_>>()
        .join("\n")
}

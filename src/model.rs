use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub struct UploadedFile {
    pub id: String,
    pub filename: String,
    #[serde(skip_serializing)]
    pub path: PathBuf,
    pub size: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Processing,
    Succeeded,
    Failed,
}

#[derive(Clone, Serialize)]
pub struct ConvertJob {
    pub id: String,
    pub file_id: String,
    pub filename: String,
    pub theme: String,
    pub status: JobStatus,
    pub pdf_url: Option<String>,
    pub warnings: Vec<String>,
    pub logs: Vec<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub pdf_path: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub html_path: Option<PathBuf>,
}

#[derive(Deserialize)]
pub struct RenderRequest {
    pub file_id: Option<String>,
    pub markdown_content: Option<String>,
    pub compare_markdown_content: Option<String>,
    pub filename: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub render_mermaid: bool,
    #[serde(default)]
    pub strict_mermaid: bool,
    #[serde(default)]
    pub format: Option<PdfFormatOverride>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PdfFormatOverride {
    pub cover_enabled: Option<bool>,
    pub toc_enabled: Option<bool>,
    pub chapter_page_break: Option<bool>,
    pub doc_code: Option<String>,
    pub version: Option<String>,
    pub owner: Option<String>,
    pub page_size: Option<String>,
    pub margin_top: Option<String>,
    pub margin_right: Option<String>,
    pub margin_bottom: Option<String>,
    pub margin_left: Option<String>,
    pub page_numbers: Option<bool>,
    pub footer_format: Option<String>,
    pub footer_align: Option<String>,
    pub header_enabled: Option<bool>,
    pub header_format: Option<String>,
    pub header_align: Option<String>,
}

#[derive(Serialize)]
pub struct FileResponse {
    pub file_id: String,
    pub filename: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub html: String,
    pub warnings: Vec<String>,
    pub logs: Vec<String>,
}

#[derive(Serialize)]
pub struct ConvertResponse {
    pub job_id: String,
    pub status: JobStatus,
}

fn default_theme() -> String {
    "jp-standard".to_string()
}

fn default_true() -> bool {
    true
}

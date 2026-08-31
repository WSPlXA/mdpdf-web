use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
pub struct RenderRequest {
    pub source_path: Option<String>,
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
    pub custom_css: Option<String>,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub html: String,
    pub warnings: Vec<String>,
    pub logs: Vec<String>,
}

fn default_theme() -> String {
    "jp-standard".to_string()
}

fn default_true() -> bool {
    true
}

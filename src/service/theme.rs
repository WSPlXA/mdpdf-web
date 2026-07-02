use std::path::Path;

use html_escape::encode_text;
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Result},
    model::{PdfFormatOverride, RenderRequest},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintOptions {
    pub print_background: bool,
    pub prefer_css_page_size: bool,
    pub display_header_footer: bool,
    pub header_template: String,
    pub footer_template: String,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            print_background: true,
            prefer_css_page_size: true,
            display_header_footer: true,
            header_template: "<div></div>".to_string(),
            footer_template: "<div style=\"width:100%; font-size:8px; color:#666; padding:0 18mm; text-align:right;\"><span class=\"pageNumber\"></span> / <span class=\"totalPages\"></span></div>".to_string(),
            margin_top: 0.0,
            margin_right: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThemeRenderOptions {
    pub print_css: String,
    pub print_options: PrintOptions,
    pub document_options: DocumentOptions,
}

#[derive(Clone, Debug)]
pub struct DocumentOptions {
    pub cover_enabled: bool,
    pub toc_enabled: bool,
    pub chapter_page_break: bool,
    pub doc_code: String,
    pub version: String,
    pub owner: String,
}

#[derive(Debug, Deserialize)]
struct ThemeConfig {
    #[serde(default)]
    page: PageConfig,
    #[serde(default)]
    layout: LayoutConfig,
    pdf: Option<PdfConfig>,
}

#[derive(Debug, Deserialize)]
struct PageConfig {
    #[serde(default = "default_page_size")]
    size: String,
    #[serde(default)]
    margin: PageMargin,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            size: default_page_size(),
            margin: PageMargin::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PageMargin {
    #[serde(default = "default_margin_top")]
    top: String,
    #[serde(default = "default_margin_right")]
    right: String,
    #[serde(default = "default_margin_bottom")]
    bottom: String,
    #[serde(default = "default_margin_left")]
    left: String,
}

impl Default for PageMargin {
    fn default() -> Self {
        Self {
            top: default_margin_top(),
            right: default_margin_right(),
            bottom: default_margin_bottom(),
            left: default_margin_left(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct LayoutConfig {
    page_number: Option<bool>,
    toc: Option<bool>,
    chapter_page_break: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PdfConfig {
    #[serde(default = "default_true")]
    print_background: bool,
    #[serde(default = "default_true")]
    prefer_css_page_size: bool,
    #[serde(default)]
    chrome_margin: ChromeMargin,
    #[serde(default)]
    header: HeaderFooterConfig,
    #[serde(default)]
    footer: HeaderFooterConfig,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self::with_page_numbers(true)
    }
}

impl PdfConfig {
    fn with_page_numbers(page_numbers: bool) -> Self {
        Self {
            print_background: true,
            prefer_css_page_size: true,
            chrome_margin: ChromeMargin::default(),
            header: HeaderFooterConfig::default(),
            footer: HeaderFooterConfig::default_footer(page_numbers),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChromeMargin {
    #[serde(default)]
    top: f64,
    #[serde(default)]
    right: f64,
    #[serde(default)]
    bottom: f64,
    #[serde(default)]
    left: f64,
}

impl Default for ChromeMargin {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct HeaderFooterConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    text: String,
    #[serde(default)]
    page_numbers: bool,
    #[serde(default = "default_page_format")]
    format: String,
    #[serde(default = "default_align")]
    align: String,
    #[serde(default = "default_footer_font_size")]
    font_size: String,
    #[serde(default = "default_footer_color")]
    color: String,
    #[serde(default = "default_margin_left")]
    padding_left: String,
    #[serde(default = "default_margin_right")]
    padding_right: String,
}

impl Default for HeaderFooterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            text: String::new(),
            page_numbers: false,
            format: default_page_format(),
            align: default_align(),
            font_size: default_footer_font_size(),
            color: default_footer_color(),
            padding_left: default_margin_left(),
            padding_right: default_margin_right(),
        }
    }
}

impl HeaderFooterConfig {
    fn default_footer(page_numbers: bool) -> Self {
        Self {
            enabled: page_numbers,
            page_numbers,
            ..Self::default()
        }
    }
}

pub async fn load_theme_render_options(
    theme_dir: &Path,
    req: &RenderRequest,
) -> Result<ThemeRenderOptions> {
    let mut config = load_theme_config(theme_dir).await?;
    apply_request_format(&mut config, req.format.as_ref())?;
    validate_config(&config)?;
    let print_css = render_print_css(theme_dir, &config).await?;
    let print_options = build_print_options(&config, req);
    let document_options = build_document_options(&config, req);
    Ok(ThemeRenderOptions {
        print_css,
        print_options,
        document_options,
    })
}

fn apply_request_format(config: &mut ThemeConfig, format: Option<&PdfFormatOverride>) -> Result<()> {
    let Some(format) = format else {
        return Ok(());
    };

    if let Some(value) = clean_optional(&format.page_size) {
        config.page.size = value;
    }
    if let Some(value) = clean_optional(&format.margin_top) {
        config.page.margin.top = value;
    }
    if let Some(value) = clean_optional(&format.margin_right) {
        config.page.margin.right = value;
    }
    if let Some(value) = clean_optional(&format.margin_bottom) {
        config.page.margin.bottom = value;
    }
    if let Some(value) = clean_optional(&format.margin_left) {
        config.page.margin.left = value;
    }

    if format.page_numbers.is_some()
        || clean_optional(&format.footer_format).is_some()
        || clean_optional(&format.footer_align).is_some()
    {
        let default_page_numbers = config.layout.page_number.unwrap_or(true);
        let pdf = config
            .pdf
            .get_or_insert_with(|| PdfConfig::with_page_numbers(default_page_numbers));
        if let Some(value) = clean_optional(&format.footer_format) {
            validate_footer_format(&value)?;
            pdf.footer.enabled = true;
            pdf.footer.page_numbers = true;
            pdf.footer.format = value;
        }
        if let Some(value) = clean_optional(&format.footer_align) {
            pdf.footer.align = value;
        }
        if let Some(enabled) = format.page_numbers {
            pdf.footer.enabled = enabled;
            pdf.footer.page_numbers = enabled;
        }
    }

    if format.header_enabled.is_some()
        || clean_optional(&format.header_format).is_some()
        || clean_optional(&format.header_align).is_some()
    {
        let default_page_numbers = config.layout.page_number.unwrap_or(true);
        let pdf = config
            .pdf
            .get_or_insert_with(|| PdfConfig::with_page_numbers(default_page_numbers));
        if let Some(value) = clean_optional(&format.header_format) {
            pdf.header.enabled = true;
            if value.contains("{page}") || value.contains("{total}") {
                pdf.header.page_numbers = true;
                pdf.header.format = value;
            } else {
                pdf.header.page_numbers = false;
                pdf.header.text = value;
            }
        }
        if let Some(value) = clean_optional(&format.header_align) {
            pdf.header.align = value;
        }
        if let Some(enabled) = format.header_enabled {
            pdf.header.enabled = enabled;
        }
    }

    Ok(())
}

fn clean_optional(value: &Option<String>) -> Option<String> {
    value.as_ref().map(|item| item.trim()).filter(|item| !item.is_empty()).map(str::to_string)
}

async fn load_theme_config(theme_dir: &Path) -> Result<ThemeConfig> {
    let path = theme_dir.join("theme.yaml");
    let content = tokio::fs::read_to_string(&path).await?;
    serde_yaml::from_str(&content)
        .map_err(|err| AppError::Conversion(format!("invalid theme.yaml: {err}")))
}

async fn render_print_css(theme_dir: &Path, config: &ThemeConfig) -> Result<String> {
    let print = tokio::fs::read_to_string(theme_dir.join("print.css")).await?;
    Ok(print
        .replace("{{page_size}}", &config.page.size)
        .replace("{{page_margin_top}}", &config.page.margin.top)
        .replace("{{page_margin_right}}", &config.page.margin.right)
        .replace("{{page_margin_bottom}}", &config.page.margin.bottom)
        .replace("{{page_margin_left}}", &config.page.margin.left))
}

fn build_print_options(config: &ThemeConfig, req: &RenderRequest) -> PrintOptions {
    let default_pdf;
    let pdf = match &config.pdf {
        Some(pdf) => pdf,
        None => {
            default_pdf = PdfConfig::with_page_numbers(config.layout.page_number.unwrap_or(true));
            &default_pdf
        }
    };

    let header_template = if pdf.header.enabled {
        build_template(&pdf.header)
    } else {
        "<div></div>".to_string()
    };
    let footer_template = if pdf.footer.enabled {
        build_template(&pdf.footer)
    } else {
        "<div></div>".to_string()
    };

    PrintOptions {
        print_background: pdf.print_background,
        prefer_css_page_size: pdf.prefer_css_page_size,
        display_header_footer: pdf.header.enabled || pdf.footer.enabled,
        header_template,
        footer_template: footer_template.replace("{theme}", &encode_text(&req.theme)),
        margin_top: pdf.chrome_margin.top,
        margin_right: pdf.chrome_margin.right,
        margin_bottom: pdf.chrome_margin.bottom,
        margin_left: pdf.chrome_margin.left,
    }
}

fn build_document_options(config: &ThemeConfig, req: &RenderRequest) -> DocumentOptions {
    let format = req.format.as_ref();
    DocumentOptions {
        cover_enabled: format
            .and_then(|item| item.cover_enabled)
            .unwrap_or(false),
        toc_enabled: format
            .and_then(|item| item.toc_enabled)
            .unwrap_or_else(|| config.layout.toc.unwrap_or(false)),
        chapter_page_break: format
            .and_then(|item| item.chapter_page_break)
            .unwrap_or_else(|| config.layout.chapter_page_break.unwrap_or(false)),
        doc_code: format
            .and_then(|item| clean_optional(&item.doc_code))
            .unwrap_or_default(),
        version: format
            .and_then(|item| clean_optional(&item.version))
            .unwrap_or_default(),
        owner: format
            .and_then(|item| clean_optional(&item.owner))
            .unwrap_or_default(),
    }
}

fn build_template(config: &HeaderFooterConfig) -> String {
    let body = if config.page_numbers {
        page_number_markup(&config.format)
    } else {
        encode_text(&config.text).into_owned()
    };
    format!(
        "<div style=\"width:100%; font-size:{}; color:{}; padding:0 {}; text-align:{};\">{}</div>",
        config.font_size,
        config.color,
        horizontal_padding_css(config),
        config.align,
        body
    )
}

fn page_number_markup(format: &str) -> String {
    let escaped = encode_text(format).into_owned();
    escaped
        .replace("{page}", "<span class=\"pageNumber\"></span>")
        .replace("{total}", "<span class=\"totalPages\"></span>")
}

fn horizontal_padding_css(config: &HeaderFooterConfig) -> String {
    format!("{} 0 {}", config.padding_right, config.padding_left)
}

fn validate_config(config: &ThemeConfig) -> Result<()> {
    let default_pdf;
    let pdf = match &config.pdf {
        Some(pdf) => pdf,
        None => {
            default_pdf = PdfConfig::with_page_numbers(config.layout.page_number.unwrap_or(true));
            &default_pdf
        }
    };
    validate_css_ident("page.size", &config.page.size)?;
    for (name, value) in [
        ("page.margin.top", &config.page.margin.top),
        ("page.margin.right", &config.page.margin.right),
        ("page.margin.bottom", &config.page.margin.bottom),
        ("page.margin.left", &config.page.margin.left),
        ("pdf.header.font_size", &pdf.header.font_size),
        ("pdf.header.padding_left", &pdf.header.padding_left),
        ("pdf.header.padding_right", &pdf.header.padding_right),
        ("pdf.footer.font_size", &pdf.footer.font_size),
        ("pdf.footer.padding_left", &pdf.footer.padding_left),
        ("pdf.footer.padding_right", &pdf.footer.padding_right),
    ] {
        validate_css_length(name, value)?;
    }
    for (name, value) in [
        ("pdf.header.align", &pdf.header.align),
        ("pdf.footer.align", &pdf.footer.align),
    ] {
        validate_align(name, value)?;
    }
    for (name, value) in [
        ("pdf.header.color", &pdf.header.color),
        ("pdf.footer.color", &pdf.footer.color),
    ] {
        validate_css_color(name, value)?;
    }
    Ok(())
}

fn validate_css_ident(name: &str, value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b' ');
    if valid {
        Ok(())
    } else {
        Err(AppError::Conversion(format!("invalid {name}: {value}")))
    }
}

fn validate_css_length(name: &str, value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'%' | b'a'..=b'z' | b'A'..=b'Z')
        });
    if valid {
        Ok(())
    } else {
        Err(AppError::Conversion(format!("invalid {name}: {value}")))
    }
}

fn validate_align(name: &str, value: &str) -> Result<()> {
    if matches!(value, "left" | "center" | "right") {
        Ok(())
    } else {
        Err(AppError::Conversion(format!("invalid {name}: {value}")))
    }
}

fn validate_css_color(name: &str, value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'#' | b'(' | b')' | b',' | b'.' | b'%' | b' '));
    if valid {
        Ok(())
    } else {
        Err(AppError::Conversion(format!("invalid {name}: {value}")))
    }
}

fn validate_footer_format(value: &str) -> Result<()> {
    if value.len() > 80 {
        return Err(AppError::Conversion(
            "invalid footer_format: maximum length is 80 bytes".into(),
        ));
    }
    if !value.contains("{page}") {
        return Err(AppError::Conversion(
            "invalid footer_format: missing {page}".into(),
        ));
    }
    Ok(())
}

fn default_page_size() -> String {
    "A4".to_string()
}

fn default_margin_top() -> String {
    "20mm".to_string()
}

fn default_margin_right() -> String {
    "18mm".to_string()
}

fn default_margin_bottom() -> String {
    "18mm".to_string()
}

fn default_margin_left() -> String {
    "18mm".to_string()
}

fn default_page_format() -> String {
    "{page} / {total}".to_string()
}

fn default_align() -> String {
    "right".to_string()
}

fn default_footer_font_size() -> String {
    "11px".to_string()
}

fn default_footer_color() -> String {
    "#666".to_string()
}

fn default_true() -> bool {
    true
}

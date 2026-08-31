use std::{path::Path, sync::LazyLock};

use comrak::{markdown_to_html, Options};
use html_escape::encode_text;
use memchr::memchr;
use regex::Regex;
use similar::{ChangeTag, TextDiff};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    model::RenderRequest,
    service::theme::{load_theme_render_options, DocumentOptions, PrintOptions},
    state::AppState,
    theme_assets::{embedded_theme, EmbeddedTheme, PRISM_CSS},
};

pub struct RenderedDocument {
    pub html: String,
    pub warnings: Vec<String>,
    pub logs: Vec<String>,
    pub print_options: PrintOptions,
}

struct MermaidBlock {
    source_start: usize,
    source_end: usize,
}

struct DiffMarkers {
    add_start: String,
    add_end: String,
    del_start: String,
    del_end: String,
}

static TOC_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<h([23])(?:\s[^>]*)?>(.*?)</h[23]>"#).expect("valid regex"));
static TOC_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bid="([^"]+)""#).expect("valid regex"));
static TOC_ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<a\b[^>]*></a>"#).expect("valid regex"));
static RENDER_OPTIONS: LazyLock<Options<'static>> = LazyLock::new(|| {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.header_ids = Some("h-".to_string());
    options.render.unsafe_ = false;
    options
});

pub async fn render_markdown_file(
    _state: &AppState,
    markdown: &str,
    filename: &str,
    req: &RenderRequest,
    render_dir: Option<&Path>,
) -> Result<RenderedDocument> {
    validate_theme_name(&req.theme)?;
    let theme_assets = embedded_theme(&req.theme)
        .ok_or_else(|| AppError::BadRequest(format!("unknown theme: {}", req.theme)))?;
    let theme = load_theme_render_options(theme_assets, req).await?;

    let diffed;
    let (md_input, diff_markers) = if let Some(ref old_md) = req.compare_markdown_content {
        diffed = diff_markdown(old_md, markdown);
        (diffed.markdown.as_str(), Some(&diffed.markers))
    } else {
        (markdown, None)
    };

    let body = render_body(md_input, req, render_dir, diff_markers).await?;
    let body_html = decorate_body(
        &body.html,
        markdown,
        filename,
        &req.theme,
        &theme.document_options,
    )?;
    let html = apply_template(theme_assets, filename, &body_html, &theme.print_css)?;
    Ok(RenderedDocument {
        html,
        warnings: body.warnings,
        logs: body.logs,
        print_options: theme.print_options,
    })
}

struct DiffMarkdown {
    markdown: String,
    markers: DiffMarkers,
}

fn diff_markdown(old: &str, new: &str) -> DiffMarkdown {
    let diff = TextDiff::from_lines(old, new);
    let markers = DiffMarkers::new();
    let mut output = String::with_capacity(old.len() + new.len() + 512);
    let mut active: Option<ChangeTag> = None;

    for change in diff.iter_all_changes() {
        let tag = change.tag();
        if active != Some(tag) {
            close_diff_run(&mut output, &markers, active);
            open_diff_run(&mut output, &markers, tag);
            active = Some(tag);
        }
        output.push_str(change.value());
    }
    close_diff_run(&mut output, &markers, active);

    DiffMarkdown {
        markdown: output,
        markers,
    }
}

impl DiffMarkers {
    fn new() -> Self {
        let id = Uuid::new_v4().simple();
        Self {
            add_start: format!("@@MDPDF_DIFF_{id}_ADD_START@@"),
            add_end: format!("@@MDPDF_DIFF_{id}_ADD_END@@"),
            del_start: format!("@@MDPDF_DIFF_{id}_DEL_START@@"),
            del_end: format!("@@MDPDF_DIFF_{id}_DEL_END@@"),
        }
    }

    fn contains_any(&self, value: &str) -> bool {
        value.contains(&self.add_start)
            || value.contains(&self.add_end)
            || value.contains(&self.del_start)
            || value.contains(&self.del_end)
    }
}

fn open_diff_run(out: &mut String, markers: &DiffMarkers, tag: ChangeTag) {
    match tag {
        ChangeTag::Equal => {}
        ChangeTag::Delete => push_marker(out, &markers.del_start),
        ChangeTag::Insert => push_marker(out, &markers.add_start),
    }
}

fn close_diff_run(out: &mut String, markers: &DiffMarkers, tag: Option<ChangeTag>) {
    match tag {
        Some(ChangeTag::Delete) => push_marker(out, &markers.del_end),
        Some(ChangeTag::Insert) => push_marker(out, &markers.add_end),
        _ => {}
    }
}

fn push_marker(out: &mut String, marker: &str) {
    out.push_str("\n\n");
    out.push_str(marker);
    out.push_str("\n\n");
}

fn decorate_body(
    html: &str,
    markdown: &str,
    filename: &str,
    theme: &str,
    options: &DocumentOptions,
) -> Result<String> {
    validate_document_metadata(options)?;

    let mut out = String::with_capacity(html.len() + 4096);
    let title = extract_markdown_title(markdown).unwrap_or_else(|| filename.to_string());
    if options.cover_enabled {
        out.push_str(&render_cover(&title, theme, options));
    }
    if options.toc_enabled {
        out.push_str(&render_toc(html));
    }
    out.push_str("<section class=\"mdpdf-editable\">");
    if options.chapter_page_break {
        out.push_str("<section class=\"chapter-breaks\">");
        out.push_str(html);
        out.push_str("</section>");
    } else {
        out.push_str(html);
    }
    out.push_str("</section>");
    Ok(out)
}

fn render_cover(title: &str, _theme: &str, options: &DocumentOptions) -> String {
    let mut rows = Vec::with_capacity(3);
    if !options.doc_code.is_empty() {
        rows.push(("文档编号", options.doc_code.as_str()));
    }
    if !options.version.is_empty() {
        rows.push(("版本", options.version.as_str()));
    }
    if !options.owner.is_empty() {
        rows.push(("作成者 / 部門", options.owner.as_str()));
    }

    let mut html = String::with_capacity(1024);
    html.push_str("<section class=\"doc-cover\">");
    html.push_str("<div class=\"doc-cover-main\">");
    html.push_str("<h1>");
    html.push_str(&encode_text(title));
    html.push_str("</h1>");
    html.push_str("</div><dl class=\"doc-cover-meta\">");
    for (name, value) in rows {
        html.push_str("<div><dt>");
        html.push_str(&encode_text(name));
        html.push_str("</dt><dd>");
        html.push_str(&encode_text(value));
        html.push_str("</dd></div>");
    }
    html.push_str("</dl></section>");
    html
}

fn render_toc(html: &str) -> String {
    let headings = collect_toc_items(html);
    if headings.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(1024 + headings.len() * 160);
    out.push_str("<nav class=\"doc-toc\"><h2>目录</h2><ol>");
    for item in headings {
        out.push_str("<li class=\"toc-level-");
        out.push(char::from(b'0' + item.level));
        out.push_str("\"><a href=\"#");
        out.push_str(&item.id);
        out.push_str("\">");
        out.push_str(&item.title_html);
        out.push_str("</a></li>");
    }
    out.push_str("</ol></nav>");
    out
}

struct TocItem {
    level: u8,
    id: String,
    title_html: String,
}

fn collect_toc_items(html: &str) -> Vec<TocItem> {
    TOC_HEADING
        .captures_iter(html)
        .take(128)
        .filter_map(|caps| {
            let level = caps.get(1)?.as_str().as_bytes()[0] - b'0';
            let body = caps.get(2)?.as_str();
            let id = TOC_ID.captures(body)?.get(1)?.as_str().to_string();
            let title_html = TOC_ANCHOR.replace_all(body, "").into_owned();
            Some(TocItem {
                level,
                id,
                title_html,
            })
        })
        .collect()
}

fn extract_markdown_title(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let title = trimmed.strip_prefix("# ")?;
        let clean = title.trim();
        if clean.is_empty() {
            None
        } else {
            Some(clean.to_string())
        }
    })
}

fn validate_document_metadata(options: &DocumentOptions) -> Result<()> {
    for (name, value) in [
        ("doc_code", &options.doc_code),
        ("version", &options.version),
        ("owner", &options.owner),
    ] {
        if value.len() > 80 {
            return Err(AppError::BadRequest(format!("{name} exceeds 80 bytes")));
        }
    }
    Ok(())
}

async fn render_body(
    markdown: &str,
    req: &RenderRequest,
    _render_dir: Option<&Path>,
    diff_markers: Option<&DiffMarkers>,
) -> Result<RenderedDocument> {
    let (without_diagrams, diagrams) = extract_mermaid_blocks(markdown);
    let rendered = markdown_to_html(&without_diagrams, &RENDER_OPTIONS);
    let mut html = replace_mermaid_placeholders(
        &rendered,
        markdown,
        &diagrams,
        req.render_mermaid,
        req.strict_mermaid,
    )?;
    let warnings = Vec::new();
    let logs = Vec::new();

    if html.contains("@@MERMAID_BLOCK_") {
        return Err(AppError::Conversion(
            "internal mermaid placeholder leaked into rendered HTML".into(),
        ));
    }
    if let Some(markers) = diff_markers {
        html = apply_diff_markers(html, markers)?;
    }

    Ok(RenderedDocument {
        html,
        warnings,
        logs,
        print_options: PrintOptions::default(),
    })
}

fn apply_diff_markers(html: String, markers: &DiffMarkers) -> Result<String> {
    let tokens = [
        (
            format!("<p>{}</p>", markers.add_start),
            "<div class=\"diff-add\">",
        ),
        (format!("<p>{}</p>", markers.add_end), "</div>"),
        (
            format!("<p>{}</p>", markers.del_start),
            "<div class=\"diff-del\">",
        ),
        (format!("<p>{}</p>", markers.del_end), "</div>"),
    ];
    let mut output = String::with_capacity(html.len());
    let mut cursor = 0;
    while cursor < html.len() {
        let next = tokens
            .iter()
            .filter_map(|(paragraph, replacement)| {
                html[cursor..]
                    .find(paragraph)
                    .map(|offset| (cursor + offset, paragraph.as_str(), *replacement))
            })
            .min_by_key(|(offset, _, _)| *offset);
        let Some((offset, marker, replacement)) = next else {
            output.push_str(&html[cursor..]);
            break;
        };
        output.push_str(&html[cursor..offset]);
        output.push_str(replacement);
        cursor = offset + marker.len();
    }
    if markers.contains_any(&output) {
        return Err(AppError::Conversion(
            "internal diff marker leaked into rendered HTML".into(),
        ));
    }
    Ok(output)
}

fn apply_template(theme: &EmbeddedTheme, title: &str, body: &str, print: &str) -> Result<String> {
    let mut combined_style = String::with_capacity(theme.style.len() + PRISM_CSS.len() + 32);
    combined_style.push_str(theme.style);
    if !PRISM_CSS.is_empty() {
        combined_style.push_str("\n/* Prism CSS */\n");
        combined_style.push_str(PRISM_CSS);
    }

    let html = theme
        .template
        .replace("{{title}}", &encode_text(title))
        .replace("{{style}}", &combined_style)
        .replace("{{print_style}}", &print)
        .replace("{{body}}", body);

    Ok(html)
}

fn extract_mermaid_blocks(markdown: &str) -> (String, Vec<MermaidBlock>) {
    let bytes = markdown.as_bytes();
    let mut blocks = Vec::new();
    let mut output = String::with_capacity(markdown.len());
    let mut copy_from = 0;
    let mut line_start = 0;

    while line_start < bytes.len() {
        let (line_end, next_line) = line_bounds(bytes, line_start);
        let line = markdown[line_start..line_end].trim_end_matches('\r');
        let Some(rest) = line.strip_prefix("```mermaid") else {
            line_start = next_line;
            continue;
        };
        if !rest.trim().is_empty() || next_line >= bytes.len() {
            line_start = next_line;
            continue;
        }

        let source_start = next_line;
        let mut closing_start = source_start;
        let mut closing_next = source_start;
        let mut found_closing = false;
        while closing_start < bytes.len() {
            let (closing_end, after_closing) = line_bounds(bytes, closing_start);
            let closing = markdown[closing_start..closing_end].trim_end_matches('\r');
            if closing
                .strip_prefix("```")
                .is_some_and(|suffix| suffix.trim().is_empty())
            {
                closing_next = after_closing;
                found_closing = true;
                break;
            }
            closing_start = after_closing;
        }
        if !found_closing {
            break;
        }

        output.push_str(&markdown[copy_from..line_start]);
        let placeholder = format!("@@MERMAID_BLOCK_{}@@", blocks.len());
        let source = markdown[source_start..closing_start].trim_end_matches(['\r', '\n']);
        blocks.push(MermaidBlock {
            source_start,
            source_end: source_start + source.len(),
        });
        output.push_str("\n\n");
        output.push_str(&placeholder);
        output.push_str("\n\n");
        copy_from = closing_next;
        line_start = closing_next;
    }
    output.push_str(&markdown[copy_from..]);
    (output, blocks)
}

fn line_bounds(bytes: &[u8], start: usize) -> (usize, usize) {
    match memchr(b'\n', &bytes[start..]) {
        Some(relative) => (start + relative, start + relative + 1),
        None => (bytes.len(), bytes.len()),
    }
}

fn replace_mermaid_placeholders(
    html: &str,
    source_markdown: &str,
    blocks: &[MermaidBlock],
    render_mermaid: bool,
    strict_mermaid: bool,
) -> Result<String> {
    const PREFIX: &str = "@@MERMAID_BLOCK_";
    let mut cursor = 0;
    let mut output = String::with_capacity(html.len() + blocks.len() * 64);
    while let Some(relative_start) = html[cursor..].find(PREFIX) {
        let marker_start = cursor + relative_start;
        let digits_start = marker_start + PREFIX.len();
        let Some(relative_end) = html[digits_start..].find("@@") else {
            return Err(AppError::Conversion("invalid Mermaid placeholder".into()));
        };
        let marker_end = digits_start + relative_end + 2;
        let index = html[digits_start..digits_start + relative_end]
            .parse::<usize>()
            .map_err(|_| AppError::Conversion("invalid Mermaid placeholder index".into()))?;
        let block = blocks.get(index).ok_or_else(|| {
            AppError::Conversion("Mermaid placeholder index is out of range".into())
        })?;
        let source = &source_markdown[block.source_start..block.source_end];
        let content_start = if marker_start >= 3 && &html[marker_start - 3..marker_start] == "<p>" {
            marker_start - 3
        } else {
            marker_start
        };
        let content_end = if html[marker_end..].starts_with("</p>") {
            marker_end + 4
        } else {
            marker_end
        };
        output.push_str(&html[cursor..content_start]);
        if render_mermaid {
            output.push_str(
                "<figure class=\"mermaid-diagram\"><div class=\"mermaid\" data-strict=\"",
            );
            output.push_str(if strict_mermaid { "true" } else { "false" });
            output.push_str("\">");
            output.push_str(&encode_text(source));
            output.push_str("</div></figure>");
        } else {
            output.push_str("<pre class=\"mermaid-source\"><code>");
            output.push_str(&encode_text(source));
            output.push_str("</code></pre>");
        }
        cursor = content_end;
    }
    output.push_str(&html[cursor..]);
    if output.contains(PREFIX) {
        return Err(AppError::Conversion(
            "internal Mermaid placeholder leaked into rendered HTML".into(),
        ));
    }
    Ok(output)
}

fn validate_theme_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(AppError::BadRequest("invalid theme name".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn diff_render_keeps_user_html_escaped() {
        let old = "safe\n";
        let new = "safe\n<script>alert(1)</script>\n";
        let diffed = diff_markdown(old, new);
        let req = RenderRequest {
            source_path: None,
            markdown_content: Some(new.to_string()),
            compare_markdown_content: Some(old.to_string()),
            filename: Some("document.md".to_string()),
            theme: "jp-standard".to_string(),
            render_mermaid: false,
            strict_mermaid: false,
            format: None,
        };

        let rendered = render_body(&diffed.markdown, &req, None, Some(&diffed.markers))
            .await
            .expect("diff render should succeed");

        assert!(rendered.html.contains("diff-add"));
        assert!(!rendered.html.contains("<script>alert(1)</script>"));
        assert!(!diffed.markers.contains_any(&rendered.html));
    }

    #[test]
    fn toc_reads_comrak_anchor_ids() {
        let html = r##"<h2><a inert href="#part" aria-hidden="true" class="anchor" id="h-part"></a>Part</h2>"##;
        let items = collect_toc_items(html);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "h-part");
        assert_eq!(items[0].title_html, "Part");
    }

    #[test]
    fn optimized_mermaid_pass_handles_multiple_blocks() {
        let markdown = "before\n```mermaid\ngraph TD; A-->B\n```\nmiddle\n```mermaid\ngraph LR; C-->D\n```\nafter";
        let (without, blocks) = extract_mermaid_blocks(markdown);
        let rendered = markdown_to_html(&without, &Options::default());
        let html = replace_mermaid_placeholders(&rendered, markdown, &blocks, true, true).unwrap();
        assert_eq!(html.matches("class=\"mermaid\"").count(), 2);
        assert!(html.contains("data-strict=\"true\""));
        assert!(!html.contains("@@MERMAID_BLOCK_"));
    }
}

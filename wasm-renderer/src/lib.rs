use comrak::{markdown_to_html, Options};
use html_escape::encode_text;
use memchr::memchr;
use regex::Regex;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::{cell::RefCell, sync::LazyLock};
use wasm_bindgen::prelude::*;

const MODERN_TEMPLATE: &str = include_str!("../../themes/modern-tech/template.html");
const MODERN_STYLE: &str = include_str!("../../themes/modern-tech/style.css");
const MODERN_PRINT: &str = include_str!("../../themes/modern-tech/print.css");
const JP_TEMPLATE: &str = include_str!("../../themes/jp-standard/template.html");
const JP_STYLE: &str = include_str!("../../themes/jp-standard/style.css");
const JP_PRINT: &str = include_str!("../../themes/jp-standard/print.css");
const PRISM_STYLE: &str = include_str!("../../themes/common/prism.min.css");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderInput {
    markdown: String,
    #[serde(default = "default_filename")]
    filename: String,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    render_mermaid: bool,
    #[serde(default)]
    compare_markdown: Option<String>,
    #[serde(default)]
    cover_enabled: bool,
    #[serde(default)]
    toc_enabled: bool,
    #[serde(default)]
    chapter_page_break: bool,
    #[serde(default = "default_page_size")]
    page_size: String,
}

#[derive(Serialize)]
struct RenderOutput {
    html: String,
    warnings: Vec<String>,
    logs: Vec<String>,
}

struct ThemeAssets {
    template: &'static str,
    style: &'static LazyLock<String>,
    print: &'static str,
}

struct MermaidBlock {
    source_start: usize,
    source_end: usize,
}

struct DiffMarkers {
    add_start: &'static str,
    add_end: &'static str,
    del_start: &'static str,
    del_end: &'static str,
}

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
static TOC_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<h([23])(?:\s[^>]*)?>(.*?)</h[23]>"#).expect("valid regex"));
static TOC_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bid="([^"]+)""#).expect("valid regex"));
static TOC_ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<a\b[^>]*></a>"#).expect("valid regex"));
static MODERN_COMBINED_STYLE: LazyLock<String> = LazyLock::new(|| combined_style(MODERN_STYLE));
static JP_COMBINED_STYLE: LazyLock<String> = LazyLock::new(|| combined_style(JP_STYLE));

struct ParseCache {
    markdown: String,
    html_with_placeholders: String,
    diagrams: Vec<MermaidBlock>,
}

thread_local! {
    static PARSE_CACHE: RefCell<Option<ParseCache>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub struct WasmRenderOutput {
    html: String,
    warnings: Vec<String>,
    logs: Vec<String>,
}

#[wasm_bindgen]
impl WasmRenderOutput {
    pub fn take_html(&mut self) -> String {
        std::mem::take(&mut self.html)
    }

    pub fn take_warnings_json(&mut self) -> String {
        serde_json::to_string(&std::mem::take(&mut self.warnings)).unwrap_or_else(|_| "[]".into())
    }

    pub fn take_logs_json(&mut self) -> String {
        serde_json::to_string(&std::mem::take(&mut self.logs)).unwrap_or_else(|_| "[]".into())
    }
}

#[wasm_bindgen]
pub fn render_markdown(input_json: &str) -> Result<String, JsValue> {
    let input: RenderInput = serde_json::from_str(input_json)
        .map_err(|error| JsValue::from_str(&format!("invalid render request: {error}")))?;
    let output = render(input).map_err(|error| JsValue::from_str(&error))?;
    serde_json::to_string(&output)
        .map_err(|error| JsValue::from_str(&format!("failed to serialize render result: {error}")))
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render_markdown_fast(
    markdown: String,
    filename: String,
    theme: String,
    render_mermaid: bool,
    compare_markdown: Option<String>,
    cover_enabled: bool,
    toc_enabled: bool,
    chapter_page_break: bool,
    page_size: String,
) -> Result<WasmRenderOutput, JsValue> {
    let output = render(RenderInput {
        markdown,
        filename,
        theme,
        render_mermaid,
        compare_markdown,
        cover_enabled,
        toc_enabled,
        chapter_page_break,
        page_size,
    })
    .map_err(|error| JsValue::from_str(&error))?;
    Ok(WasmRenderOutput {
        html: output.html,
        warnings: output.warnings,
        logs: output.logs,
    })
}

fn render(input: RenderInput) -> Result<RenderOutput, String> {
    if input.markdown.len() > 10 * 1024 * 1024 {
        return Err("markdown exceeds the 10 MiB preview limit".into());
    }
    let RenderInput {
        markdown,
        filename,
        theme,
        render_mermaid,
        compare_markdown,
        cover_enabled,
        toc_enabled,
        chapter_page_break,
        page_size,
    } = input;
    let theme = theme_assets(&theme)?;
    let title = extract_markdown_title(&markdown)
        .unwrap_or(&filename)
        .to_string();
    let markers = DiffMarkers::default();
    let has_diff = compare_markdown.is_some();
    let markdown = if let Some(old) = compare_markdown {
        diff_markdown(&old, &markdown, &markers)
    } else {
        markdown
    };

    let (mut body, _) = render_markdown_body(markdown, render_mermaid)?;
    if has_diff {
        body = apply_diff_markers(body, &markers)?;
    }

    let warnings = Vec::new();
    let logs = Vec::new();

    let cover = cover_enabled.then(|| render_cover(&title));
    let toc = toc_enabled.then(|| render_toc(&body));

    let print = theme
        .print
        .replace("{{page_size}}", &validate_page_size(&page_size)?)
        .replace("{{page_margin_top}}", "20mm")
        .replace("{{page_margin_right}}", "18mm")
        .replace("{{page_margin_bottom}}", "18mm")
        .replace("{{page_margin_left}}", "18mm");
    let style = theme.style.as_str();
    let shell = theme
        .template
        .replace("{{title}}", &encode_text(&title))
        .replace("{{style}}", style)
        .replace("{{print_style}}", &print);
    let marker = shell
        .find("{{body}}")
        .ok_or_else(|| "theme template is missing {{body}}".to_string())?;
    let mut html = String::with_capacity(
        shell.len()
            + body.len()
            + cover.as_ref().map_or(0, String::len)
            + toc.as_ref().map_or(0, String::len)
            + 64,
    );
    html.push_str(&shell[..marker]);
    if let Some(cover) = cover {
        html.push_str(&cover);
    }
    if let Some(toc) = toc {
        html.push_str(&toc);
    }
    if chapter_page_break {
        html.push_str("<section class=\"chapter-breaks\">");
        html.push_str(&body);
        html.push_str("</section>");
    } else {
        html.push_str(&body);
    }
    html.push_str(&shell[marker + "{{body}}".len()..]);

    Ok(RenderOutput {
        html,
        warnings,
        logs,
    })
}

fn theme_assets(name: &str) -> Result<ThemeAssets, String> {
    match name {
        "modern-tech" => Ok(ThemeAssets {
            template: MODERN_TEMPLATE,
            style: &MODERN_COMBINED_STYLE,
            print: MODERN_PRINT,
        }),
        "jp-standard" => Ok(ThemeAssets {
            template: JP_TEMPLATE,
            style: &JP_COMBINED_STYLE,
            print: JP_PRINT,
        }),
        _ => Err(format!("unknown theme: {name}")),
    }
}

fn validate_page_size(value: &str) -> Result<String, String> {
    match value {
        "A3" | "A4" | "Letter" => Ok(value.to_string()),
        _ => Err(format!("invalid page size: {value}")),
    }
}

fn combined_style(style: &str) -> String {
    let mut output = String::with_capacity(style.len() + PRISM_STYLE.len() + 1);
    output.push_str(style);
    output.push('\n');
    output.push_str(PRISM_STYLE);
    output
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

fn render_markdown_body(markdown: String, render_mermaid: bool) -> Result<(String, usize), String> {
    PARSE_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let hit = slot
            .as_ref()
            .is_some_and(|cached| cached.markdown == markdown);
        if !hit {
            let (without_diagrams, diagrams) = extract_mermaid_blocks(&markdown);
            let html_with_placeholders = markdown_to_html(&without_diagrams, &RENDER_OPTIONS);
            *slot = Some(ParseCache {
                markdown,
                html_with_placeholders,
                diagrams,
            });
        }
        let cached = slot.as_ref().expect("parse cache was initialized");
        let body = replace_mermaid_placeholders(
            &cached.html_with_placeholders,
            &cached.markdown,
            &cached.diagrams,
            render_mermaid,
        )?;
        Ok((body, cached.diagrams.len()))
    })
}

fn replace_mermaid_placeholders(
    html: &str,
    source_markdown: &str,
    blocks: &[MermaidBlock],
    render_mermaid: bool,
) -> Result<String, String> {
    const PREFIX: &str = "@@MERMAID_BLOCK_";
    let mut cursor = 0;
    let mut output = String::with_capacity(html.len() + blocks.len() * 64);

    while let Some(relative_start) = html[cursor..].find(PREFIX) {
        let marker_start = cursor + relative_start;
        let digits_start = marker_start + PREFIX.len();
        let Some(relative_end) = html[digits_start..].find("@@") else {
            return Err("invalid Mermaid placeholder".into());
        };
        let marker_end = digits_start + relative_end + 2;
        let index = html[digits_start..digits_start + relative_end]
            .parse::<usize>()
            .map_err(|_| "invalid Mermaid placeholder index".to_string())?;
        let block = blocks
            .get(index)
            .ok_or_else(|| "Mermaid placeholder index is out of range".to_string())?;
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
            output.push_str("<figure class=\"mermaid-diagram\"><div class=\"mermaid\">");
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
        return Err("internal Mermaid placeholder leaked into rendered HTML".into());
    }
    Ok(output)
}

fn diff_markdown(old: &str, new: &str, markers: &DiffMarkers) -> String {
    let mut output = String::with_capacity(old.len() + new.len() + 256);
    let mut active = None;
    for change in TextDiff::from_lines(old, new).iter_all_changes() {
        if active != Some(change.tag()) {
            close_diff_run(&mut output, markers, active);
            open_diff_run(&mut output, markers, change.tag());
            active = Some(change.tag());
        }
        output.push_str(change.value());
    }
    close_diff_run(&mut output, markers, active);
    output
}

fn open_diff_run(out: &mut String, markers: &DiffMarkers, tag: ChangeTag) {
    match tag {
        ChangeTag::Equal => {}
        ChangeTag::Delete => push_marker(out, markers.del_start),
        ChangeTag::Insert => push_marker(out, markers.add_start),
    }
}

fn close_diff_run(out: &mut String, markers: &DiffMarkers, tag: Option<ChangeTag>) {
    match tag {
        Some(ChangeTag::Delete) => push_marker(out, markers.del_end),
        Some(ChangeTag::Insert) => push_marker(out, markers.add_end),
        _ => {}
    }
}

fn push_marker(out: &mut String, marker: &str) {
    out.push_str("\n\n");
    out.push_str(marker);
    out.push_str("\n\n");
}

fn apply_diff_markers(html: String, markers: &DiffMarkers) -> Result<String, String> {
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
    if [
        markers.add_start,
        markers.add_end,
        markers.del_start,
        markers.del_end,
    ]
    .iter()
    .any(|marker| output.contains(marker))
    {
        return Err("internal diff marker leaked into rendered HTML".into());
    }
    Ok(output)
}

impl Default for DiffMarkers {
    fn default() -> Self {
        Self {
            add_start: "@@MDPDF_WASM_DIFF_ADD_START@@",
            add_end: "@@MDPDF_WASM_DIFF_ADD_END@@",
            del_start: "@@MDPDF_WASM_DIFF_DEL_START@@",
            del_end: "@@MDPDF_WASM_DIFF_DEL_END@@",
        }
    }
}

fn extract_markdown_title(markdown: &str) -> Option<&str> {
    markdown.lines().find_map(|line| {
        let clean = line.trim_start().strip_prefix("# ")?.trim();
        (!clean.is_empty()).then_some(clean)
    })
}

fn render_cover(title: &str) -> String {
    format!(
        "<section class=\"doc-cover\"><div class=\"doc-cover-main\"><h1>{}</h1></div><dl class=\"doc-cover-meta\"></dl></section>",
        encode_text(title)
    )
}

fn render_toc(html: &str) -> String {
    let mut items = String::new();
    for caps in TOC_HEADING.captures_iter(html).take(128) {
        let Some(id) = TOC_ID.captures(&caps[2]).and_then(|value| value.get(1)) else {
            continue;
        };
        let title = TOC_ANCHOR.replace_all(&caps[2], "");
        items.push_str(&format!(
            "<li class=\"toc-level-{}\"><a href=\"#{}\">{}</a></li>",
            &caps[1],
            id.as_str(),
            title
        ));
    }
    if items.is_empty() {
        String::new()
    } else {
        format!("<nav class=\"doc-toc\"><h2>目录</h2><ol>{items}</ol></nav>")
    }
}

fn default_filename() -> String {
    "document.md".into()
}
fn default_theme() -> String {
    "jp-standard".into()
}
fn default_page_size() -> String {
    "A4".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_safe_document_with_toc_and_mermaid_placeholder() {
        let output = render(RenderInput {
            markdown:
                "# Title\n\n## Part\n\n<script>x</script>\n\n```mermaid\ngraph TD; A-->B\n```"
                    .into(),
            filename: "test.md".into(),
            theme: "jp-standard".into(),
            render_mermaid: true,
            compare_markdown: None,
            cover_enabled: false,
            toc_enabled: true,
            chapter_page_break: false,
            page_size: "A4".into(),
        })
        .unwrap();
        assert!(output.html.contains("class=\"doc-toc\""), "{}", output.html);
        assert!(output.html.contains("class=\"mermaid\""));
        assert!(!output.html.contains("<script>x</script>"));
    }

    #[test]
    fn extracts_multiple_mermaid_blocks_with_crlf() {
        let markdown = "before\r\n```mermaid\r\ngraph TD; A-->B\r\n```\r\nmiddle\n```mermaid\nsequenceDiagram\nA->>B: hi\n```\nafter";
        let (without, blocks) = extract_mermaid_blocks(markdown);
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            &markdown[blocks[0].source_start..blocks[0].source_end],
            "graph TD; A-->B"
        );
        assert_eq!(
            &markdown[blocks[1].source_start..blocks[1].source_end],
            "sequenceDiagram\nA->>B: hi"
        );
        assert!(without.contains("@@MERMAID_BLOCK_0@@"));
        assert!(without.contains("@@MERMAID_BLOCK_1@@"));
        let rendered = markdown_to_html(&without, &RENDER_OPTIONS);
        let html = replace_mermaid_placeholders(&rendered, markdown, &blocks, true).unwrap();
        assert_eq!(html.matches("class=\"mermaid\"").count(), 2);
        assert!(!html.contains("@@MERMAID_BLOCK_"));
    }
}

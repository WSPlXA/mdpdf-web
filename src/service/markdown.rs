use std::path::Path;

use comrak::{markdown_to_html, Options};
use html_escape::encode_text;
use regex::Regex;
use similar::{ChangeTag, TextDiff};

use crate::{
    error::{AppError, Result},
    model::RenderRequest,
    service::{
        mermaid::render_mermaid_svg,
        theme::{load_theme_render_options, DocumentOptions, PrintOptions},
    },
    state::AppState,
};

pub struct RenderedDocument {
    pub html: String,
    pub warnings: Vec<String>,
    pub logs: Vec<String>,
    pub print_options: PrintOptions,
}

struct MermaidBlock {
    placeholder: String,
    source: String,
}

pub async fn render_markdown_file(
    state: &AppState,
    markdown: &str,
    filename: &str,
    req: &RenderRequest,
    render_dir: Option<&Path>,
) -> Result<RenderedDocument> {
    validate_theme_name(&req.theme)?;
    let theme_dir = state.theme_dir(&req.theme);
    let theme = load_theme_render_options(&theme_dir, req).await?;
    
    let diffed_markdown;
    let md_input = if let Some(ref old_md) = req.compare_markdown_content {
        diffed_markdown = diff_markdown(old_md, markdown);
        &diffed_markdown
    } else {
        markdown
    };

    let body = render_body(md_input, req, render_dir).await?;
    let body_html = decorate_body(
        &body.html,
        markdown,
        filename,
        &req.theme,
        &theme.document_options,
    )?;
    let html = apply_template(&theme_dir, filename, &body_html, &theme.print_css).await?;
    Ok(RenderedDocument {
        html,
        warnings: body.warnings,
        logs: body.logs,
        print_options: theme.print_options,
    })
}

fn diff_markdown(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();
    
    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Equal => {
                output.push_str(value);
            }
            ChangeTag::Delete => {
                output.push_str("<div class=\"diff-del\">\n\n");
                output.push_str(value);
                output.push_str("\n\n</div>\n");
            }
            ChangeTag::Insert => {
                output.push_str("<div class=\"diff-add\">\n\n");
                output.push_str(value);
                output.push_str("\n\n</div>\n");
            }
        }
    }
    output
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
    if options.chapter_page_break {
        out.push_str("<section class=\"chapter-breaks\">");
        out.push_str(html);
        out.push_str("</section>");
    } else {
        out.push_str(html);
    }
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
    let heading = Regex::new(r#"(?s)<h([23]) id="([^"]+)">(.*?)</h[23]>"#).expect("valid regex");
    heading
        .captures_iter(html)
        .take(128)
        .filter_map(|caps| {
            let level = caps.get(1)?.as_str().as_bytes()[0] - b'0';
            let id = caps.get(2)?.as_str().to_string();
            let title_html = caps.get(3)?.as_str().to_string();
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
    render_dir: Option<&Path>,
) -> Result<RenderedDocument> {
    let (without_diagrams, diagrams) = extract_mermaid_blocks(markdown);
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.header_ids = Some("h-".to_string());
    if req.compare_markdown_content.is_some() {
        options.render.unsafe_ = true; // Allow diff-add and diff-del HTML tags
    } else {
        options.render.unsafe_ = false;
    }

    let mut html = markdown_to_html(&without_diagrams, &options);
    let mut warnings = Vec::new();
    let mut logs = Vec::new();

    for (idx, block) in diagrams.iter().enumerate() {
        let replacement = if req.render_mermaid {
            let dir = match render_dir {
                Some(path) => path.to_path_buf(),
                None => std::env::temp_dir().join(format!("mdpdf-preview-{}", uuid::Uuid::new_v4())),
            };
            match render_mermaid_svg(idx + 1, &block.source, &dir).await {
                Ok(svg) => {
                    logs.push(format!("mermaid block {} rendered", idx + 1));
                    format!("<figure class=\"mermaid-diagram\">{svg}</figure>")
                }
                Err(err) if req.strict_mermaid => {
                    return Err(AppError::Conversion(format!(
                        "mermaid block {} failed: {err}",
                        idx + 1
                    )));
                }
                Err(err) => {
                    warnings.push(format!("mermaid block {} failed: {err}", idx + 1));
                    format!(
                        "<div class=\"diagram-error\"><strong>Mermaid rendering failed at block {}.</strong><pre>{}</pre></div>",
                        idx + 1,
                        encode_text(&block.source)
                    )
                }
            }
        } else {
            format!(
                "<pre class=\"mermaid-source\"><code>{}</code></pre>",
                encode_text(&block.source)
            )
        };
        let paragraph = format!("<p>{}</p>", block.placeholder);
        if html.contains(&paragraph) {
            html = html.replace(&paragraph, &replacement);
        } else {
            html = html.replace(&block.placeholder, &replacement);
        }
    }

    if html.contains("@@MERMAID_BLOCK_") {
        return Err(AppError::Conversion(
            "internal mermaid placeholder leaked into rendered HTML".into(),
        ));
    }

    detect_wide_tables(&html, &mut warnings);
    Ok(RenderedDocument {
        html,
        warnings,
        logs,
        print_options: PrintOptions::default(),
    })
}

async fn apply_template(theme_dir: &Path, title: &str, body: &str, print: &str) -> Result<String> {
    let template = tokio::fs::read_to_string(theme_dir.join("template.html")).await?;
    let style = tokio::fs::read_to_string(theme_dir.join("style.css")).await?;
    
    let themes_common_dir = theme_dir.parent().ok_or_else(|| {
        AppError::Conversion("invalid theme path structure".into())
    })?;
    
    let prism_css = tokio::fs::read_to_string(themes_common_dir.join("common/prism.min.css"))
        .await
        .unwrap_or_default();
        
    let prism_js = tokio::fs::read_to_string(themes_common_dir.join("common/prism.min.js"))
        .await
        .unwrap_or_default();

    let mut combined_style = style;
    if !prism_css.is_empty() {
        combined_style.push_str("\n/* Prism CSS */\n");
        combined_style.push_str(&prism_css);
    }

    let mut html = template
        .replace("{{title}}", &encode_text(title))
        .replace("{{style}}", &combined_style)
        .replace("{{print_style}}", &print)
        .replace("{{body}}", body);

    if !prism_js.is_empty() {
        let highlight_script = r#"
<script>
  window.addEventListener('DOMContentLoaded', () => {
    Prism.highlightAll();
  });
</script>
"#;
        let script_block = format!("<script>\n{}\n</script>\n{}", prism_js, highlight_script);
        if html.contains("</body>") {
            html = html.replace("</body>", &format!("{}\n</body>", script_block));
        } else {
            html.push_str(&script_block);
        }
    }

    Ok(html)
}

fn extract_mermaid_blocks(markdown: &str) -> (String, Vec<MermaidBlock>) {
    let fence = Regex::new(r"(?ms)^```mermaid\s*\n(.*?)\n```\s*$").expect("valid regex");
    let mut blocks = Vec::new();
    let replaced = fence
        .replace_all(markdown, |caps: &regex::Captures<'_>| {
            let id = blocks.len();
            let placeholder = format!("@@MERMAID_BLOCK_{id}@@");
            blocks.push(MermaidBlock {
                placeholder: placeholder.clone(),
                source: caps[1].to_string(),
            });
            format!("\n\n{placeholder}\n\n")
        })
        .to_string();
    (replaced, blocks)
}

fn detect_wide_tables(html: &str, warnings: &mut Vec<String>) {
    let th_count = html.matches("<th").count();
    if th_count >= 8 {
        warnings.push("document contains wide tables; PDF uses fixed layout and word wrapping".into());
    }
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

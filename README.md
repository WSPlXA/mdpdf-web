# mdpdf-web

Rust Axum based Markdown to PDF web tool for internal design documents.

## Data Layout

The MVP keeps only control-plane metadata in memory:

- `UploadedFile`: generated id, display filename, disk path, size, timestamp.
- `ConvertJob`: generated id, file id, status, output paths, warnings and short logs.

Markdown content is not split into long-lived paragraph objects. Each conversion reads the file into one contiguous `String`, replaces Mermaid fences with placeholders, renders HTML once through Comrak, optionally prepends cover and TOC HTML, then writes one HTML file and one PDF file under an isolated job directory.

Theme configuration is parsed once per render into a compact `PrintOptions` struct. The Node print script receives that bounded JSON and only performs Chromium DevTools Protocol calls.

## Hot Path

The hot path is:

```text
read Markdown -> extract Mermaid fences -> comrak HTML -> cover/TOC pass -> template -> chromium PDF
```

No per-request database write is used in the MVP. External processes are invoked with argument arrays and timeouts.

## Run

With Docker:

```powershell
docker compose up --build
```

Then open:

```text
http://localhost:8080
```

Without Docker you need these on PATH:

- `cargo`
- `mmdc`
- `chromium` / `chrome` / `msedge`, or set `MDPDF_CHROMIUM`
- optional: `MDPDF_PUPPETEER_CONFIG` if `mmdc` must point at a fixed Chromium
- optional: `MDPDF_PRINT_SCRIPT` to override the DevTools PDF renderer script

```powershell
cargo run
```

## Theme PDF Config

`themes/<name>/theme.yaml` owns page and footer output:

```yaml
page:
  size: A4
  margin:
    top: 20mm
    right: 18mm
    bottom: 18mm
    left: 18mm
pdf:
  header:
    enabled: false
  footer:
    enabled: true
    page_numbers: true
    format: "{page} / {total}"
    align: right
    font_size: 8px
    color: "#666"
```

`themes/<name>/print.css` uses the page placeholders emitted from YAML. `scripts/print_pdf.mjs` should stay generic; do not hard-code company footer variants there.

Per-request overrides can be sent on preview or convert without changing the theme file:

```json
{
  "file_id": "f_xxx",
  "theme": "jp-standard",
  "render_mermaid": true,
  "strict_mermaid": true,
  "format": {
    "cover_enabled": true,
    "toc_enabled": true,
    "chapter_page_break": true,
    "doc_code": "DOC-001",
    "version": "v1.0",
    "owner": "platform",
    "page_size": "A4",
    "margin_top": "20mm",
    "margin_right": "18mm",
    "margin_bottom": "18mm",
    "margin_left": "18mm",
    "page_numbers": true,
    "footer_format": "Page {page} of {total}",
    "footer_align": "center"
  }
}
```

## Regression Smoke

With the service running:

```powershell
.\scripts\regression_smoke.ps1
```

The script uploads `samples\regression.md`, verifies Mermaid placeholders do not leak, verifies cover and TOC HTML exist, converts to PDF, downloads `workdir\regression-output.pdf`, and checks the page footer when Python has `pypdf`.

## API

- `POST /api/files`
- `POST /api/preview`
- `POST /api/convert`
- `GET /api/jobs/{job_id}`
- `GET /api/jobs/{job_id}/download`

## Limits

- Upload: `.md` or `.markdown`
- Max upload body: 10 MiB
- Mermaid timeout: 20s per block
- Chromium timeout: 90s per PDF
- PDF page footer: Chromium DevTools `Page.printToPDF`, rendered as `page / total`

param(
    [string]$BaseUrl = "http://localhost:8080",
    [string]$Sample = "samples\regression.md",
    [string]$OutFile = "workdir\regression-output.pdf",
    [string]$Python = "python"
)

$ErrorActionPreference = "Stop"

$samplePath = Resolve-Path $Sample
$upload = curl.exe -s -F "file=@$samplePath;type=text/markdown" "$BaseUrl/api/files" | ConvertFrom-Json
$payload = @{
    file_id = $upload.file_id
    theme = "jp-standard"
    render_mermaid = $true
    strict_mermaid = $true
    format = @{
        cover_enabled = $true
        toc_enabled = $true
        chapter_page_break = $true
        doc_code = "DOC-REG-001"
        version = "v1.0"
        owner = "platform"
        page_size = "A4"
        margin_top = "20mm"
        margin_right = "18mm"
        margin_bottom = "18mm"
        margin_left = "18mm"
        page_numbers = $true
        footer_format = "Page {page} of {total}"
        footer_align = "center"
    }
} | ConvertTo-Json -Compress -Depth 4

$preview = Invoke-RestMethod -Method Post -Uri "$BaseUrl/api/preview" -ContentType "application/json" -Body $payload
if ($preview.html -match "@@MERMAID_BLOCK_") {
    throw "preview contains leaked Mermaid placeholder"
}
if ($preview.html -notmatch "mermaid-diagram") {
    throw "preview does not contain rendered Mermaid figure"
}
if ($preview.html -notmatch "doc-cover") {
    throw "preview does not contain cover"
}
if ($preview.html -notmatch "doc-toc") {
    throw "preview does not contain TOC"
}

$convert = Invoke-RestMethod -Method Post -Uri "$BaseUrl/api/convert" -ContentType "application/json" -Body $payload
$job = $null
for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Seconds 1
    $job = Invoke-RestMethod -Uri "$BaseUrl/api/jobs/$($convert.job_id)"
    if ($job.status -eq "succeeded" -or $job.status -eq "failed") {
        break
    }
}
if ($job.status -ne "succeeded") {
    throw "job failed or timed out: $($job | ConvertTo-Json -Compress)"
}

$outPath = Join-Path (Get-Location) $OutFile
New-Item -ItemType Directory -Force -Path (Split-Path $outPath) | Out-Null
Invoke-WebRequest -Uri "$BaseUrl$($job.pdf_url)" -OutFile $outPath
$pdf = Get-Item $outPath
if ($pdf.Length -lt 2048) {
    throw "PDF is too small: $($pdf.Length) bytes"
}

$textCheck = "skipped"
try {
    $env:PYTHONIOENCODING = "utf-8"
    $script = @"
from pypdf import PdfReader
pdf = r'''$outPath'''
reader = PdfReader(pdf)
text = '\n'.join(page.extract_text() or '' for page in reader.pages)
if 'Page 1' not in text:
    raise SystemExit('missing page footer')
print('pages=' + str(len(reader.pages)))
"@
    $textOutput = (& $Python -c $script 2>&1) -join "; "
    if ($LASTEXITCODE -ne 0) {
        if ($textOutput -match "ModuleNotFoundError|No module named") {
            $textCheck = "skipped: pypdf is not installed"
        } else {
            throw $textOutput
        }
    } else {
        $textCheck = $textOutput
    }
} catch {
    throw "PDF text check failed: $($_.Exception.Message)"
}

[pscustomobject]@{
    file_id = $upload.file_id
    job_id = $convert.job_id
    status = $job.status
    pdf_path = $pdf.FullName
    pdf_bytes = $pdf.Length
    preview_has_placeholder = ($preview.html -match "@@MERMAID_BLOCK_")
    preview_has_mermaid_figure = ($preview.html -match "mermaid-diagram")
    preview_has_cover = ($preview.html -match "doc-cover")
    preview_has_toc = ($preview.html -match "doc-toc")
    pdf_text_check = $textCheck
    logs = $job.logs
    warnings = $job.warnings
} | ConvertTo-Json -Depth 6

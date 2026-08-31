param(
    [string]$ExePath = "target\release\mdpdf-desktop.exe",
    [string]$OutputDir = "dist\Markdown-PDF-Desktop"
)

$ErrorActionPreference = "Stop"
$resolvedExe = (Resolve-Path -LiteralPath $ExePath).Path
$output = [System.IO.Path]::GetFullPath((Join-Path $PWD $OutputDir))

New-Item -ItemType Directory -Path $output -Force | Out-Null
Copy-Item -LiteralPath $resolvedExe -Destination (Join-Path $output "Markdown-PDF-Desktop.exe") -Force

[pscustomobject]@{
    executable = Join-Path $output "Markdown-PDF-Desktop.exe"
    themes = "embedded"
} | ConvertTo-Json

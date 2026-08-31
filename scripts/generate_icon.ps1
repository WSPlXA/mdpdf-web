$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NativeIcon {
    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    public static extern bool DestroyIcon(IntPtr handle);
}
"@

$iconDirectory = Join-Path $PSScriptRoot "..\icons"
$iconPath = Join-Path $iconDirectory "icon.ico"
New-Item -ItemType Directory -Force -Path $iconDirectory | Out-Null

$bitmap = [System.Drawing.Bitmap]::new(64, 64)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$background = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(47, 111, 115))
$paper = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::White)
$accent = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(47, 111, 115))
$font = [System.Drawing.Font]::new("Segoe UI", 21, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
$handle = [IntPtr]::Zero

try {
    $graphics.FillRectangle($background, 0, 0, 64, 64)
    $graphics.FillRectangle($paper, 13, 8, 38, 48)
    $graphics.FillRectangle($accent, 19, 18, 26, 4)
    $graphics.FillRectangle($accent, 19, 28, 18, 4)
    $graphics.DrawString("M", $font, $accent, 20, 34)
    $handle = $bitmap.GetHicon()
    $icon = [System.Drawing.Icon]::FromHandle($handle)
    $stream = [System.IO.File]::Create($iconPath)
    try {
        $icon.Save($stream)
    } finally {
        $stream.Dispose()
        $icon.Dispose()
    }
} finally {
    if ($handle -ne [IntPtr]::Zero) {
        [NativeIcon]::DestroyIcon($handle) | Out-Null
    }
    $font.Dispose()
    $accent.Dispose()
    $paper.Dispose()
    $background.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
}

Write-Host "generated $iconPath"

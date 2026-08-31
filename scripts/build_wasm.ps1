$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$wasmTargetDir = Join-Path $root "wasm-renderer\target"
$target = Join-Path $wasmTargetDir "wasm32-unknown-unknown\release\mdpdf_wasm.wasm"
$output = Join-Path $root "public\wasm"
$generatedWasm = Join-Path $output "mdpdf_wasm_bg.wasm"
$generatedJs = Join-Path $output "mdpdf_wasm.js"

cargo +stable-x86_64-pc-windows-msvc build --manifest-path (Join-Path $root "wasm-renderer\Cargo.toml") --target wasm32-unknown-unknown --target-dir $wasmTargetDir --release
if ($LASTEXITCODE -ne 0) { throw "WASM cargo build failed" }

$bindingsMissing = !(Test-Path -LiteralPath $generatedWasm) -or !(Test-Path -LiteralPath $generatedJs)
$bindingsStale = !$bindingsMissing -and (
    (Get-Item -LiteralPath $target).LastWriteTimeUtc -gt
    (Get-Item -LiteralPath $generatedWasm).LastWriteTimeUtc
)

if ($bindingsMissing -or $bindingsStale) {
    wasm-bindgen $target --out-dir $output --target web --no-typescript
    if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed" }
} else {
    Write-Host "WASM bindings are up to date"
}

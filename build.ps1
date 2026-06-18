# Tower Hacker - build script (Windows PowerShell)
# Compiles the Rust game to WebAssembly and copies the .wasm next to index.html.
#
# Run from this folder:   .\build.ps1
# (If you get an execution-policy error, run once:
#     powershell -ExecutionPolicy Bypass -File .\build.ps1   )

$ErrorActionPreference = "Stop"

# Always run from the folder this script lives in (so cargo finds Cargo.toml).
Set-Location -Path $PSScriptRoot
Write-Host "==> Working dir: $PSScriptRoot" -ForegroundColor DarkGray

$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) { $cargo = "cargo" }   # fall back to PATH

# Use the GNU toolchain (bundles its own linker) so we don't need Visual Studio's
# link.exe to compile the host-side build scripts / proc-macros.
# Install it once with:  rustup toolchain install stable-x86_64-pc-windows-gnu
$toolchain = "stable-x86_64-pc-windows-gnu"

# Target-specific linker flag so miniquad's JS-provided symbols (init_webgl, gl*, now,
# run_animation_loop, ...) become wasm imports instead of link errors. Set as an env var
# (not just .cargo/config.toml) so it always applies, regardless of current directory.
$env:CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS = "-C link-arg=--allow-undefined"

Write-Host "==> Building WASM (release) with $toolchain ..." -ForegroundColor Green
& $cargo "+$toolchain" build --release --target wasm32-unknown-unknown

$wasm = "target\wasm32-unknown-unknown\release\kamikaze.wasm"
if (-not (Test-Path $wasm)) {
    throw "Build finished but $wasm was not found. Check the cargo output above."
}

Copy-Item $wasm ".\kamikaze.wasm" -Force
$size = [math]::Round((Get-Item ".\kamikaze.wasm").Length / 1MB, 2)
Write-Host "==> Copied kamikaze.wasm ($size MB) next to index.html." -ForegroundColor Green
Write-Host "==> Now run:  python serve.py   and open http://localhost:8080" -ForegroundColor Cyan

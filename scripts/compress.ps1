# Compress the release binary with UPX (saves ~50-70% size)
# Install UPX: winget install upx  OR  choco install upx
$exe = Join-Path $PSScriptRoot "..\target\release\lightshotv2.exe"
if (-not (Test-Path $exe)) {
    Write-Error "Build first: cargo build --release"
    exit 1
}
$before = (Get-Item $exe).Length / 1MB
if (Get-Command upx -ErrorAction SilentlyContinue) {
    upx --best --lzma $exe
    $after = (Get-Item $exe).Length / 1MB
    Write-Host "Compressed: $([math]::Round($before, 2)) MB -> $([math]::Round($after, 2)) MB"
} else {
    Write-Host "UPX not found. Install with: winget install upx"
}

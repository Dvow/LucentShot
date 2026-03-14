$exe = "target\release\lightshotv2.exe"
if (!(Test-Path $exe)) {
    Write-Host "Run 'cargo build --release' first"
    exit 1
}
if (!(Get-Command upx -ErrorAction SilentlyContinue)) {
    Write-Host "UPX not found. Installing via winget..."
    winget install -e --id UPX.UPX --accept-package-agreements --accept-source-agreements
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
    if (!(Get-Command upx -ErrorAction SilentlyContinue)) {
        Write-Host "Install complete. Run this script again in a new terminal."
        exit 1
    }
}
$before = (Get-Item $exe).Length / 1MB
upx --best $exe
$after = (Get-Item $exe).Length / 1MB
Write-Host "Compressed: $([math]::Round($before, 2)) MB -> $([math]::Round($after, 2)) MB"

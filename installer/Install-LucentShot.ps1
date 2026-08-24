param(
    [switch]$Build,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$App = 'LucentShot'
$Exe = 'lucentshot.exe'
$Root = Join-Path $env:LOCALAPPDATA "Programs\$App"
$Menu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$App"
$Key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$App"
$Repo = Split-Path $PSScriptRoot

function Find-File([string]$Name) {
    foreach ($dir in @(
        $PSScriptRoot,
        (Join-Path $PSScriptRoot 'tessdata'),
        (Join-Path $Repo 'target\release'),
        (Join-Path $Repo 'target\release\tessdata'),
        (Join-Path $Repo 'assets')
    )) {
        $path = Join-Path $dir $Name
        if (Test-Path -LiteralPath $path) { return $path }
    }
    throw "missing $Name. Run cargo build --release or use the portable zip."
}

if ($Uninstall) {
    Remove-Item -LiteralPath $Root, $Menu, $Key -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "[OK] $App removed"
    return
}

if ($Build) {
    Push-Location $Repo
    try {
        cargo build --release
        if ($LASTEXITCODE) { throw "cargo build failed (exit $LASTEXITCODE)" }
    }
    finally { Pop-Location }
}

New-Item -ItemType Directory -Force -Path $Root, "$Root\tessdata", $Menu | Out-Null
Copy-Item (Find-File $Exe) (Join-Path $Root $Exe) -Force
Copy-Item $PSCommandPath $Root -Force
foreach ($name in 'tesseract.dll', 'leptonica-1.85.0.dll') {
    Copy-Item (Find-File $name) (Join-Path $Root $name) -Force
}
Copy-Item (Find-File 'eng.traineddata') "$Root\tessdata\eng.traineddata" -Force

$exePath = Join-Path $Root $Exe
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut("$Menu\$App.lnk")
$shortcut.TargetPath = $exePath
$shortcut.WorkingDirectory = $Root
$shortcut.Save()

$script = Join-Path $Root (Split-Path $PSCommandPath -Leaf)
$uninstall = "`"$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe`" -NoProfile -ExecutionPolicy Bypass -File `"$script`" -Uninstall"
New-Item -Path $Key -Force | Out-Null
foreach ($pair in @{
        DisplayName     = $App
        DisplayVersion  = (Get-Item $exePath).VersionInfo.ProductVersion
        Publisher       = 'Dvow'
        InstallLocation = $Root
        DisplayIcon     = $exePath
        UninstallString = $uninstall
    }.GetEnumerator()) {
    New-ItemProperty -Path $Key -Name $pair.Key -Value $pair.Value -Force | Out-Null
}
New-ItemProperty -Path $Key -Name NoModify -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $Key -Name NoRepair -PropertyType DWord -Value 1 -Force | Out-Null
Write-Host "[OK] $App installed at $Root"

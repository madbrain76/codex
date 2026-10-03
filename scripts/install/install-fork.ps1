[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
    throw "This fork installer currently supports Windows x86_64 only."
}

$repository = "madbrain76/codex"
$target = "x86_64-pc-windows-msvc"
$asset = "codex-package-$target.zip"
$codexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $HOME ".codex" }
$releaseRoot = Join-Path $codexHome "packages\standalone\releases"
$visibleBin = if ($env:CODEX_INSTALL_DIR) { $env:CODEX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\codex" }
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("codex-fork-" + [guid]::NewGuid())

try {
    New-Item -ItemType Directory -Force -Path $temporary, $releaseRoot | Out-Null
    $archive = Join-Path $temporary $asset
    Invoke-WebRequest -Uri "https://github.com/$repository/releases/latest/download/$asset" -OutFile $archive
    Expand-Archive -LiteralPath $archive -DestinationPath $temporary

    $manifest = Get-Content -Raw (Join-Path $temporary "codex-package.json") | ConvertFrom-Json
    $releaseDir = Join-Path $releaseRoot ("$($manifest.version)-$target")
    if (Test-Path -LiteralPath $releaseDir) { Remove-Item -Recurse -Force -LiteralPath $releaseDir }
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    Get-ChildItem -Force -LiteralPath $temporary | Where-Object { $_.Name -ne $asset } | Move-Item -Destination $releaseDir
    if (-not (Test-Path -LiteralPath (Join-Path $releaseDir "bin\codex.exe"))) { throw "Downloaded archive is not a Codex package." }

    $parent = Split-Path -Parent $visibleBin
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    if (Test-Path -LiteralPath $visibleBin) { Remove-Item -Recurse -Force -LiteralPath $visibleBin }
    New-Item -ItemType Junction -Path $visibleBin -Target (Join-Path $releaseDir "bin") | Out-Null
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$visibleBin*") { [Environment]::SetEnvironmentVariable("Path", "$visibleBin;$userPath", "User") }
    $env:Path = "$visibleBin;$env:Path"
    Write-Host "Installed Codex $($manifest.version) to $visibleBin\codex.exe"
    Write-Host "Open a new PowerShell window, then run: codex"
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $temporary
}

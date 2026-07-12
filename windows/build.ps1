# Build complete Windows distribution + NiaoSetup.exe installer.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$WinDir = $PSScriptRoot
$InstallerDir = Join-Path $WinDir "installer"

Write-Host "== Niao Windows full build =="
Write-Host ""

Set-Location $Root
Write-Host "[1/4] Building niao.exe and nm.exe (release)..."
cargo build --release --no-default-features --bin niao --bin nm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "[2/4] Building NiaoUninstall.exe..."
Set-Location $InstallerDir
cargo build --release --bin NiaoUninstall
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "[3/4] Staging payload (binaries + libraries + uninstaller)..."
Set-Location $Root
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $WinDir "prepare-bundle.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "[4/4] Building NiaoSetup.exe installer..."
Set-Location $InstallerDir
cargo build --release --bin NiaoSetup
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$setupSrc = Join-Path $WinDir "installer\target\release\NiaoSetup.exe"
$setupDest = Join-Path $WinDir "NiaoSetup.exe"
Copy-Item $setupSrc $setupDest -Force

Set-Location $WinDir
Write-Host ""
Write-Host "== Build complete =="
Write-Host ""
Write-Host "  Installer:   $setupDest"
Write-Host "  Portable:    $(Join-Path $WinDir 'niao.cmd')"
Write-Host "  niao_home:   $(Join-Path $WinDir 'niao_home')"
Write-Host ""
Write-Host "Install:"
Write-Host "  Double-click NiaoSetup.exe"
Write-Host "  Start Menu -> Niao -> Niao Terminal"
Write-Host "  Uninstall: niao uninstall  or  Windows Settings -> Apps"
Write-Host ""
Write-Host "Or run portable (no install):"
Write-Host "  niao.cmd run examples\hello.niao"

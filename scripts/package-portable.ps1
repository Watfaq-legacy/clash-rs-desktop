param(
    [string]$Configuration = "release",
    [string]$OutputRoot = "dist",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetDir = Join-Path $repoRoot "target\$Configuration"
$packageName = "clash-rs-desktop-windows-x64"
$outputDir = Join-Path (Join-Path $repoRoot $OutputRoot) $packageName

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        cargo build --release -p clash-rs-desktop-ui --features reactor
        cargo build --release -p clash-rs-desktop-service
    }
    finally {
        Pop-Location
    }
}

if (Test-Path $outputDir) {
    Remove-Item $outputDir -Recurse -Force
}

New-Item -ItemType Directory -Path $outputDir | Out-Null
New-Item -ItemType Directory -Path (Join-Path $outputDir "bin") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $outputDir "docs") | Out-Null

$uiExe = Join-Path $targetDir "clash-rs-desktop-ui.exe"
$serviceExe = Join-Path $targetDir "clash-rs-desktop-service.exe"

if (-not (Test-Path $uiExe)) {
    throw "UI executable not found: $uiExe"
}
if (-not (Test-Path $serviceExe)) {
    throw "Service executable not found: $serviceExe"
}

Copy-Item $uiExe (Join-Path $outputDir "bin\clash-rs-desktop-ui.exe")
Copy-Item $serviceExe (Join-Path $outputDir "bin\clash-rs-desktop-service.exe")
Copy-Item (Join-Path $repoRoot "README.md") (Join-Path $outputDir "README.md")
Copy-Item (Join-Path $repoRoot "docs\DASHBOARD_PARITY.md") (Join-Path $outputDir "docs\DASHBOARD_PARITY.md")
Copy-Item (Join-Path $repoRoot "docs\IMPLEMENTATION_PLAN.md") (Join-Path $outputDir "docs\IMPLEMENTATION_PLAN.md")

$runScript = @'
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $root "bin\clash-rs-desktop-ui.exe")
'@
Set-Content -Path (Join-Path $outputDir "Run-ClashRsDesktop.ps1") -Value $runScript -Encoding UTF8

$serviceScript = @'
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --pipe
'@
Set-Content -Path (Join-Path $outputDir "Run-ServicePipeHost.ps1") -Value $serviceScript -Encoding UTF8

$installServiceScript = @'
$ErrorActionPreference = "Stop"
function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script from an elevated PowerShell session."
    }
}
Assert-Administrator
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$programData = Join-Path $env:ProgramData "clash-rs-desktop"
New-Item -ItemType Directory -Force -Path $programData | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $programData "profiles") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $programData "runtime") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $programData "core") | Out-Null
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --install
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --start-service
'@
Set-Content -Path (Join-Path $outputDir "Install-Service.ps1") -Value $installServiceScript -Encoding UTF8

$startServiceScript = @'
$ErrorActionPreference = "Stop"
function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script from an elevated PowerShell session."
    }
}
Assert-Administrator
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --start-service
'@
Set-Content -Path (Join-Path $outputDir "Start-Service.ps1") -Value $startServiceScript -Encoding UTF8

$stopServiceScript = @'
$ErrorActionPreference = "Stop"
function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script from an elevated PowerShell session."
    }
}
Assert-Administrator
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --stop-service
'@
Set-Content -Path (Join-Path $outputDir "Stop-Service.ps1") -Value $stopServiceScript -Encoding UTF8

$uninstallServiceScript = @'
$ErrorActionPreference = "Stop"
function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script from an elevated PowerShell session."
    }
}
Assert-Administrator
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $root "bin\clash-rs-desktop-service.exe") --uninstall
'@
Set-Content -Path (Join-Path $outputDir "Uninstall-Service.ps1") -Value $uninstallServiceScript -Encoding UTF8

$zipPath = Join-Path (Join-Path $repoRoot $OutputRoot) "$packageName.zip"
if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path (Join-Path $outputDir "*") -DestinationPath $zipPath

Write-Host "Portable package: $outputDir"
Write-Host "Zip package: $zipPath"

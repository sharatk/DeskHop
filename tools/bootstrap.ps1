# Installs the DeskHop development toolchain on Windows 10 22H2 / 11.
# Run from an elevated terminal (Build Tools needs it):
#   tools\bootstrap.cmd
# Safe to re-run; installed packages are skipped.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Run from an elevated terminal: right-click Terminal > Run as administrator.'
    exit 1
}

function Install-WingetPackage([string]$Id, [string[]]$Extra = @()) {
    winget list --id $Id --exact --accept-source-agreements | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "ok   $Id"
        return
    }
    Write-Host "add  $Id"
    winget install --id $Id --exact --silent --accept-package-agreements --accept-source-agreements @Extra
    if ($LASTEXITCODE -ne 0) { throw "winget install $Id failed ($LASTEXITCODE)" }
}

# MSVC linker and Windows SDK for x64 and ARM64.
Install-WingetPackage 'Microsoft.VisualStudio.2022.BuildTools' @(
    '--override',
    '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --includeRecommended'
)
Install-WingetPackage 'Rustlang.Rustup'
Install-WingetPackage 'OpenJS.NodeJS.LTS'
Install-WingetPackage 'Microsoft.DotNet.SDK.8'   # WiX v5 ships as a .NET tool.

# Pick up PATH changes from the installs above.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')

# rust-toolchain.toml selects the channel, components, and targets.
Push-Location $root
try {
    rustup toolchain install

    if (-not (dotnet tool list --global | Select-String -Quiet '^wix\s')) {
        dotnet tool install --global wix
    }

    npm install --global @fission-ai/openspec

    Push-Location apps\desktop
    try { npm install } finally { Pop-Location }

    cargo archcheck
} finally {
    Pop-Location
}

Write-Host ''
Write-Host 'Done. Open a new terminal, then:'
Write-Host '  openspec init          # once, to set up openspec/ and /opsx commands'
Write-Host '  cargo build --workspace'

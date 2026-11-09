<#
.SYNOPSIS
    7player installer for Windows.

.DESCRIPTION
    Downloads the latest (or a pinned) Windows release, verifies its SHA-256,
    unpacks it into a per-user directory, and puts 7player on your PATH.

.PARAMETER Version
    Release tag to install, e.g. "0.1.0". Defaults to the newest release.

.PARAMETER InstallDir
    Target directory. Defaults to %LOCALAPPDATA%\Programs\7player.

.PARAMETER NoPath
    Unpack only; do not touch the user PATH.

.EXAMPLE
    irm https://raw.githubusercontent.com/7unar/7player/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version 0.1.0 -InstallDir C:\Tools\7player
#>
[CmdletBinding()]
param(
    [string]$Version = '',
    [string]$InstallDir = '',
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Repo    = '7unar/7player'
$Target  = 'x86_64-pc-windows-msvc'
$ExeName = '7player.exe'

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\7player'
}

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "  OK  $msg" -ForegroundColor Green }
function Write-Warn2($msg){ Write-Host "  !!  $msg" -ForegroundColor Yellow }

# --- resolve version -------------------------------------------------------
if (-not $Version) {
    Write-Step 'Resolving latest release'
    $api = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $rel = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = '7player-installer' } -TimeoutSec 30
        $Version = $rel.tag_name.TrimStart('v')
    } catch {
        throw "Could not query the latest release ($($_.Exception.Message)). Pass -Version explicitly."
    }
}
Write-Ok "version $Version"

$archive = "7player-$Target.zip"
$base    = "https://github.com/$Repo/releases/download/v$Version"
$zipUrl  = "$base/$archive"
$shaUrl  = "$base/$archive.sha256"

# --- download --------------------------------------------------------------
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("7player-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
    $zipPath = Join-Path $tmp $archive
    Write-Step "Downloading $archive"
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -UseBasicParsing -TimeoutSec 300
    $sizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 1)
    Write-Ok "$sizeMb MB"

    # --- verify ------------------------------------------------------------
    Write-Step 'Verifying SHA-256'
    try {
        $shaText = (Invoke-WebRequest -Uri $shaUrl -UseBasicParsing -TimeoutSec 60).Content
        $expected = ($shaText -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) {
            throw "checksum mismatch:`n  expected $expected`n  actual   $actual"
        }
        Write-Ok 'checksum matches'
    } catch [System.Net.WebException] {
        Write-Warn2 'no .sha256 published for this release; skipping verification'
    } catch [Microsoft.PowerShell.Commands.HttpResponseException] {
        Write-Warn2 'no .sha256 published for this release; skipping verification'
    }

    # --- unpack ------------------------------------------------------------
    Write-Step "Installing to $InstallDir"
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

    # The archive has a flat root; accept it either way.
    $srcRoot = Join-Path $tmp "7player-$Version-$Target"
    if (-not (Test-Path (Join-Path $srcRoot $ExeName))) { $srcRoot = $tmp }
    if (-not (Test-Path (Join-Path $srcRoot $ExeName))) {
        throw "$ExeName not found in the archive"
    }
    Copy-Item -Path (Join-Path $srcRoot '*') -Destination $InstallDir -Recurse -Force
    Write-Ok "$ExeName installed"

    # --- PATH --------------------------------------------------------------
    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not $userPath) { $userPath = '' }
        $entries = $userPath -split ';' | Where-Object { $_ }
        if ($entries -notcontains $InstallDir) {
            Write-Step 'Adding to user PATH'
            $newPath = (@($entries) + $InstallDir) -join ';'
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            Write-Ok 'PATH updated (open a new terminal to pick it up)'
        } else {
            Write-Ok 'already on PATH'
        }
    }
} finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# --- done ------------------------------------------------------------------
Write-Host ''
Write-Host "7player $Version installed." -ForegroundColor Green
Write-Host "  binary : $(Join-Path $InstallDir $ExeName)"
Write-Host '  run it : 7player           (new terminal), or:'
Write-Host "           & '$(Join-Path $InstallDir $ExeName)'"
Write-Host ''
Write-Host 'Tips:'
Write-Host '  - Use Windows Terminal at 120x40 or larger for the intended layout.'
Write-Host '  - Playback needs WASAPI (built into Windows) — no extra runtime deps.'
Write-Host '  - Uninstall: delete the install folder and drop it from PATH.'

# Brings Class Markers up to date with the installed Convergence (run by Update.bat):
#   1. tools/generate.py   - re-read Convergence's files (markers, notes, descriptions)
#   2. build the plugin and copy ClassMarkers.dll to the project root
#   3. tools/profiles.ps1  - put our entries back in Convergence's launch profiles (an update
#                            replaces them)
# The icons need nothing: the plugin remakes them from Convergence's art when that changes.
# Needs Python (with the packages in tools/requirements.txt) and Rust (cargo).

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot                  # ...\ClassMarkers

function Step($what, [scriptblock]$run) {
    Write-Host ""
    Write-Host "== $what"
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE) - see the messages above." }
}

if (Get-Process -Name eldenring -ErrorAction SilentlyContinue) {
    throw "Elden Ring is running. Close it first: the plugin can't be replaced while the game uses it."
}
foreach ($tool in 'python', 'cargo') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool was not found. Install it (see the README's setup section) and run this again."
    }
}

Push-Location $root
try {
    Step "Reading Convergence's files" { python tools\generate.py }
    Step "Building the plugin" { cargo build --release --manifest-path plugin\Cargo.toml }
    Copy-Item plugin\target\release\ClassMarkers.dll ClassMarkers.dll -Force
    Write-Host "  copied ClassMarkers.dll"
    Step "Launch profiles" { powershell -NoProfile -ExecutionPolicy Bypass -File tools\profiles.ps1 install }
}
finally {
    Pop-Location
}

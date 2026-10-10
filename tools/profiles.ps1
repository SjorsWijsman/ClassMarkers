# Adds or removes the Class Markers plugin in The Convergence's me3 launch profiles.
#   profiles.ps1 install   - add the [[natives]] entry (backs each profile up first)
#   profiles.ps1 uninstall - remove it again
# The entry is fenced by marker comments so uninstall removes exactly what install added.

param([Parameter(Mandatory)][ValidateSet('install', 'uninstall')][string]$Action)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot                  # ...\ClassMarkers
# ConvergenceER: from convergence_path.txt (this machine's setting), else the folder we sit in.
$setting = Join-Path $root 'convergence_path.txt'
$convergence = if (Test-Path $setting) { (Get-Content $setting -Raw).Trim() } else { Split-Path -Parent $root }
$me3Dir = Join-Path $convergence 'me3'
# The profiles load ConvergenceER\ClassMarkers\ClassMarkers.dll; when this project lives elsewhere,
# that folder is a junction to it.
$link = Join-Path $convergence 'ClassMarkers'
$dll = Join-Path $root 'ClassMarkers.dll'
$begin = '# >>> Class Markers (added by ClassMarkers\Install.bat)'
$end = '# <<< Class Markers'
$block = "$begin`r`n[[natives]]`r`npath = './../ClassMarkers/ClassMarkers.dll'`r`n$end"
$added = "`r`n`r`n" + $block + "`r`n"    # exactly what install appends, so uninstall can remove it exactly

if (-not (Test-Path $me3Dir)) {
    throw "Could not find The Convergence's me3 folder at $me3Dir. Put the ConvergenceER folder path in $setting."
}
if ($Action -eq 'install' -and -not (Test-Path $dll)) {
    throw "ClassMarkers.dll is missing from $root"
}
if ($Action -eq 'install' -and -not (Test-Path $link)) {
    New-Item -ItemType Junction -Path $link -Target $root | Out-Null
    Write-Host "  linked $link -> $root"
}

$profiles = Get-ChildItem $me3Dir -Filter '*.me3'
if (-not $profiles) { throw "No .me3 profiles found in $me3Dir" }

foreach ($p in $profiles) {
    $text = [IO.File]::ReadAllText($p.FullName)
    $present = $text -match [regex]::Escape($begin)
    if ($Action -eq 'install') {
        if ($present) { Write-Host "  $($p.Name): already installed"; continue }
        Copy-Item $p.FullName "$($p.FullName).before-classmarkers" -Force
        $text = $text + $added
        [IO.File]::WriteAllText($p.FullName, $text)
        Write-Host "  $($p.Name): added (backup: $($p.Name).before-classmarkers)"
    } else {
        if (-not $present) { Write-Host "  $($p.Name): not installed"; continue }
        $text = $text.Replace($added, '')
        if ($text -match [regex]::Escape($begin)) {
            throw "$($p.Name) contains a hand-edited Class Markers entry; remove it manually or restore $($p.Name).before-classmarkers"
        }
        [IO.File]::WriteAllText($p.FullName, $text)
        Write-Host "  $($p.Name): removed"
    }
}

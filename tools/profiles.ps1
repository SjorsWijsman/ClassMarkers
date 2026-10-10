# Adds or removes Class Markers in The Convergence's me3 launch profiles.
#   profiles.ps1 install   - add our entries (backs each profile up first), or update an older entry
#   profiles.ps1 uninstall - remove them again
# Two entries, fenced by marker comments so uninstall removes exactly what install added:
#   [[natives]] the plugin DLL
#   [[package]] ClassMarkers\mod, loaded after Convergence's package: our copy of the world map
#               movie with our icons (rebuilt by the plugin at every start, see plugin/src/gfx.rs).

param([Parameter(Mandatory)][ValidateSet('install', 'uninstall')][string]$Action,
      [string]$Convergence)   # ConvergenceER folder; default: convergence_path.txt or our parent
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot                  # ...\ClassMarkers
# ConvergenceER: from convergence_path.txt (this machine's setting), else the folder we sit in.
$setting = Join-Path $root 'convergence_path.txt'
$convergence = if ($Convergence) { $Convergence } elseif (Test-Path $setting) { (Get-Content $setting -Raw).Trim() } else { Split-Path -Parent $root }
$me3Dir = Join-Path $convergence 'me3'
# The profiles load ConvergenceER\ClassMarkers\ClassMarkers.dll; when this project lives elsewhere,
# that folder is a junction to it.
$link = Join-Path $convergence 'ClassMarkers'
$dll = Join-Path $root 'ClassMarkers.dll'
$begin = '# >>> Class Markers (added by ClassMarkers\Install.bat)'
$end = '# <<< Class Markers'
# Our block as install appended it: two line breaks, the fenced lines, one line break.
$fenced = '(?s)(\r?\n){0,2}' + [regex]::Escape($begin) + '.*?' + [regex]::Escape($end) + '\r?\n?'
# me3 only serves package files that exist at launch: put Convergence's own movie there as a
# placeholder; the plugin replaces it with the version that has our icons.
$movie = 'menu\02_120_worldmap.gfx'

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
if ($Action -eq 'install' -and -not (Test-Path (Join-Path $root "mod\$movie"))) {
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'mod\menu') | Out-Null
    Copy-Item (Join-Path $convergence "mod\$movie") (Join-Path $root "mod\$movie")
    Write-Host "  placed mod\$movie"
}
if ($Action -eq 'uninstall' -and (Test-Path (Join-Path $root 'mod'))) {
    Remove-Item -Recurse -Force (Join-Path $root 'mod')
}

$profiles = Get-ChildItem $me3Dir -Filter '*.me3'
if (-not $profiles) { throw "No .me3 profiles found in $me3Dir" }

foreach ($p in $profiles) {
    $text = [IO.File]::ReadAllText($p.FullName)
    $present = $text -match [regex]::Escape($begin)
    if ($Action -eq 'install') {
        # Our package must load after Convergence's: read its id from the profile.
        $m = [regex]::Match($text, '(?s)\[\[package\]\][^\[]*?id\s*=\s*"([^"]+)"')
        if (-not $m.Success) { throw "$($p.Name): no Convergence [[package]] with an id found" }
        $block = "$begin`r`n[[natives]]`r`npath = './../ClassMarkers/ClassMarkers.dll'`r`n`r`n" +
                 "[[package]]`r`nid = `"class-markers`"`r`npath = './../ClassMarkers/mod'`r`n" +
                 "load_after = [{ id = `"$($m.Groups[1].Value)`", optional = false }]`r`n$end"
        $added = "`r`n`r`n" + $block + "`r`n"
        if ($present) {
            $current = [regex]::Match($text, $fenced).Value
            if ($current -eq $added) { Write-Host "  $($p.Name): already installed"; continue }
            $text = [regex]::Replace($text, $fenced, '') + $added
            [IO.File]::WriteAllText($p.FullName, $text)
            Write-Host "  $($p.Name): updated"
            continue
        }
        Copy-Item $p.FullName "$($p.FullName).before-classmarkers" -Force
        [IO.File]::WriteAllText($p.FullName, $text + $added)
        Write-Host "  $($p.Name): added (backup: $($p.Name).before-classmarkers)"
    } else {
        if (-not $present) { Write-Host "  $($p.Name): not installed"; continue }
        $text = [regex]::Replace($text, $fenced, '')
        [IO.File]::WriteAllText($p.FullName, $text)
        Write-Host "  $($p.Name): removed"
    }
}

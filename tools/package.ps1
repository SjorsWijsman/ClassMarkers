# Packs a release: dist\ClassMarkers-<version>.zip with what players need, laid out to be unzipped
# into ConvergenceER (it holds a ClassMarkers folder). Run after building the plugin; GitHub runs it
# for each version tag (.github/workflows/release.yml).
#   ClassMarkers\ClassMarkers.dll, icons\colors.txt, Install.bat, Uninstall.bat, tools\profiles.ps1,
#   README.md (docs/player-readme.md), LICENSE, THIRD-PARTY-NOTICES.md (from cargo metadata)
param([string]$Dll)   # the built plugin; default plugin\target\release\ClassMarkers.dll
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
if (-not $Dll) { $Dll = Join-Path $root 'plugin\target\release\ClassMarkers.dll' }
if (-not (Test-Path $Dll)) { throw "The plugin isn't built: $Dll is missing (cargo build --release in plugin\)." }
$version = (Select-String -Path (Join-Path $root 'plugin\Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

$dist = Join-Path $root 'dist'
$stage = Join-Path $dist 'stage'
$pack = Join-Path $stage 'ClassMarkers'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path (Join-Path $pack 'tools') | Out-Null

Copy-Item $Dll (Join-Path $pack 'ClassMarkers.dll')
Copy-Item (Join-Path $root 'icons') (Join-Path $pack 'icons') -Recurse   # colors.txt (icons are made in game)
foreach ($f in 'Install.bat', 'Uninstall.bat') { Copy-Item (Join-Path $root $f) (Join-Path $pack $f) }
Copy-Item (Join-Path $root 'tools\profiles.ps1') (Join-Path $pack 'tools\profiles.ps1')
Copy-Item (Join-Path $root 'docs\player-readme.md') (Join-Path $pack 'README.md')
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $pack 'LICENSE')

# The DLL contains its Rust dependencies: list them with their licenses (from cargo metadata).
$meta = cargo metadata --format-version 1 --manifest-path (Join-Path $root 'plugin\Cargo.toml') | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
$notices = @(
    '# Third-party notices', '',
    'ClassMarkers.dll includes these Rust crates, used under their licenses (see each repository).',
    'Its technique for growing game tables and text in memory follows Map For Goblins - DLL by',
    'VirusAlex (MIT, https://github.com/VirusAlex/ERR-MapForGoblins-DLL).', '',
    '| Crate | Version | License | Repository |', '|---|---|---|---|'
)
foreach ($p in ($meta.packages | Where-Object { $_.name -ne 'class_markers' } | Sort-Object name, version)) {
    $notices += "| $($p.name) | $($p.version) | $($p.license) | $($p.repository) |"
}
$notices | Set-Content -Encoding utf8 (Join-Path $pack 'THIRD-PARTY-NOTICES.md')

$zip = Join-Path $dist "ClassMarkers-$version.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $pack -DestinationPath $zip
Remove-Item -Recurse -Force $stage
Write-Host "Packed $zip"

# Installs the Strata panel for Premiere Pro (current user).
# Usage: right-click > Run with PowerShell (from the unzipped folder),
# or: powershell -File install-windows.ps1 -Server http://strata-server:8080
# The panel then updates itself from the Strata server.
param([string]$Server = "")
$ErrorActionPreference = "Stop"
$src = Join-Path $PSScriptRoot "io.stratagraphics.premiere"
if (-not (Test-Path $src)) { $src = Split-Path -Parent $PSScriptRoot }
$dest = Join-Path $env:APPDATA "Adobe\CEP\extensions\io.stratagraphics.premiere"
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
# The panel as installed before the rename.
$former = Join-Path $env:APPDATA "Adobe\CEP\extensions\io.pulsecg.premiere"
if (Test-Path $former) { Remove-Item -Recurse -Force $former }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
foreach ($item in "CSXS", "host", "ui", "index.html", "loader.js", ".debug", "strata.ini") {
  $from = Join-Path $src $item
  if (Test-Path $from) { Copy-Item -Recurse -Force $from $dest }
}
if ($Server) { Set-Content -Path (Join-Path $dest "strata.ini") -Value "[strata]`nserver=$Server" }
# The panel is not signed by Adobe Exchange: allow unsigned extensions.
foreach ($v in 9..16) {
  New-Item -Force -Path "HKCU:\Software\Adobe\CSXS.$v" | Out-Null
  Set-ItemProperty -Path "HKCU:\Software\Adobe\CSXS.$v" -Name PlayerDebugMode -Value "1"
}
Write-Host "Installed in $dest"
Write-Host "Restart Premiere Pro, then open Window > Extensions > Strata."

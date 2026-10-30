# Removes the Strata Multiview service and program files. Data in %ProgramData%\Strata Multiview is kept unless -Purge.
param([switch]$Purge)
$ErrorActionPreference = "Continue"
$prefix = Join-Path $env:ProgramFiles "Strata Multiview"
& (Join-Path $prefix "strata-mv-server.exe") service uninstall
Remove-Item -Recurse -Force $prefix -ErrorAction SilentlyContinue
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
[Environment]::SetEnvironmentVariable("Path", (($machinePath -split ";") | Where-Object { $_ -ne $prefix }) -join ";", "Machine")
if ($Purge) { Remove-Item -Recurse -Force (Join-Path $env:ProgramData "Strata Multiview") -ErrorAction SilentlyContinue }
Write-Host "Strata Multiview removed."

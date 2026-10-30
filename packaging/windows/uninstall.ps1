# Removes the Strata Graphics service and program files. Data in %ProgramData%\Strata Graphics is kept unless -Purge.
param([switch]$Purge)
$ErrorActionPreference = "Continue"
$prefix = Join-Path $env:ProgramFiles "Strata Graphics"
& (Join-Path $prefix "strata-server.exe") service uninstall
Remove-Item -Recurse -Force $prefix -ErrorAction SilentlyContinue
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
[Environment]::SetEnvironmentVariable("Path", (($machinePath -split ";") | Where-Object { $_ -ne $prefix }) -join ";", "Machine")
if ($Purge) { Remove-Item -Recurse -Force (Join-Path $env:ProgramData "Strata Graphics") -ErrorAction SilentlyContinue }
Write-Host "Strata Graphics removed."

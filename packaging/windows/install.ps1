# Strata Multiview — Windows installer (Windows service + firewall rule).
# Usage (elevated PowerShell):  .\install.ps1 [-Bind 0.0.0.0:8090] [-DataDir "C:\ProgramData\Strata Multiview"]
param(
  [string]$Bind = "0.0.0.0:8090",
  [string]$DataDir = ""
)
$ErrorActionPreference = "Stop"

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Error "Please run this script from an elevated PowerShell (Run as administrator)."
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$prefix = Join-Path $env:ProgramFiles "Strata Multiview"
$exe = Join-Path $prefix "strata-mv-server.exe"

# Upgrade: remove the previous service (data is kept).
if (Get-Service -Name "strata-multiview" -ErrorAction SilentlyContinue) {
  & $exe service uninstall | Out-Null
}
New-Item -ItemType Directory -Force -Path $prefix | Out-Null
Copy-Item (Join-Path $here "strata-mv-server.exe") $exe -Force
Get-ChildItem $here -Include *.md, OFL.txt -File -Recurse | Copy-Item -Destination $prefix -Force
# NDI runtime shipped with the package, loaded from next to the server.
Get-ChildItem $here -Include Processing.NDI.Lib.x64.dll, NDI-NOTICE.txt -File -Recurse | Copy-Item -Destination $prefix -Force
Unblock-File $exe

$cliArgs = @("service", "install", "--bind", $Bind)
if ($DataDir -ne "") { $cliArgs += @("--data-dir", $DataDir) }
& $exe @cliArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Make `strata-mv-server` available in new terminals.
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if (($machinePath -split ";") -notcontains $prefix) {
  [Environment]::SetEnvironmentVariable("Path", "$machinePath;$prefix", "Machine")
}
Write-Host "Logs: $env:ProgramData\Strata Multiview\logs\strata-mv-server.log"

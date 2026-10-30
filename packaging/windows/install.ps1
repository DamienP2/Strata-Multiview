# Strata Graphics — Windows installer (Windows service + firewall rule).
# Usage (elevated PowerShell):  .\install.ps1 [-Bind 0.0.0.0:8080] [-DataDir "C:\ProgramData\Strata Graphics"]
param(
  [string]$Bind = "0.0.0.0:8080",
  [string]$DataDir = ""
)
$ErrorActionPreference = "Stop"

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Error "Please run this script from an elevated PowerShell (Run as administrator)."
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$prefix = Join-Path $env:ProgramFiles "Strata Graphics"
$exe = Join-Path $prefix "strata-server.exe"

# Upgrade: remove the previous service (data is kept).
if (Get-Service -Name "strata-graphics" -ErrorAction SilentlyContinue) {
  & $exe service uninstall | Out-Null
}
# An install from before the rename (Pulse CG): its service and program go,
# its data is moved by the service install below.
$former = Join-Path $env:ProgramFiles "PulseCG"
if (Get-Service -Name "pulse-cg" -ErrorAction SilentlyContinue) {
  & (Join-Path $former "cg-server.exe") service uninstall | Out-Null
}
Remove-Item -Recurse -Force $former -ErrorAction SilentlyContinue
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
[Environment]::SetEnvironmentVariable("Path", (($machinePath -split ";") | Where-Object { $_ -ne $former }) -join ";", "Machine")
New-Item -ItemType Directory -Force -Path $prefix | Out-Null
Copy-Item (Join-Path $here "strata-server.exe") $exe -Force
Get-ChildItem $here -Include *.md, OFL.txt -File -Recurse | Copy-Item -Destination $prefix -Force
# NDI runtime shipped with the package, loaded from next to the server.
Get-ChildItem $here -Include Processing.NDI.Lib.x64.dll, NDI-NOTICE.txt -File -Recurse | Copy-Item -Destination $prefix -Force
Unblock-File $exe

$cliArgs = @("service", "install", "--bind", $Bind)
if ($DataDir -ne "") { $cliArgs += @("--data-dir", $DataDir) }
& $exe @cliArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Make `strata-server` available in new terminals.
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if (($machinePath -split ";") -notcontains $prefix) {
  [Environment]::SetEnvironmentVariable("Path", "$machinePath;$prefix", "Machine")
}
Write-Host "Logs: $env:ProgramData\Strata Graphics\logs\strata-server.log"

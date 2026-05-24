# install-adk.ps1 - Install Windows ADK 10.1.26100.2454 + WinPE add-on
# (English-only to avoid PS 5.1 GBK codepage parsing issues with UTF-8 .ps1)
#
# Usage:
#   1. Right-click PowerShell -> Run as Administrator
#   2. powershell -NoProfile -ExecutionPolicy Bypass -File C:\NeuroBoot\tools-dev\install-adk.ps1
#   3. Wait for two /passive progress windows to finish (5-15 min each)
#   4. Paste the verification output back to Claude

#Requires -RunAsAdministrator

$adk = 'C:\NeuroBoot\tools-dev\adksetup.exe'
$winpe = 'C:\NeuroBoot\tools-dev\adkwinpesetup.exe'
$adkRoot = 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit'

if (-not (Test-Path $adk)) {
    Write-Error "adksetup.exe not found: $adk"
    exit 1
}
if (-not (Test-Path $winpe)) {
    Write-Error "adkwinpesetup.exe not found: $winpe"
    exit 1
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "Must run from admin PowerShell. Right-click PowerShell -> Run as administrator."
    exit 1
}

Write-Host "[1/2] Installing Windows ADK (Deployment Tools only)..."
Write-Host "  /passive: progress window will appear; no interaction needed."
Write-Host "  Roughly 5-15 minutes depending on network speed."
$start1 = Get-Date
& $adk /passive /norestart /features OptionId.DeploymentTools
$elapsed1 = (Get-Date) - $start1
Write-Host ("  Done (elapsed {0} min, exit code {1})" -f [math]::Round($elapsed1.TotalMinutes,1), $LASTEXITCODE)

Write-Host ""
Write-Host "[2/2] Installing WinPE add-on..."
$start2 = Get-Date
& $winpe /passive /norestart /features OptionId.WindowsPreinstallationEnvironment
$elapsed2 = (Get-Date) - $start2
Write-Host ("  Done (elapsed {0} min, exit code {1})" -f [math]::Round($elapsed2.TotalMinutes,1), $LASTEXITCODE)

Write-Host ""
Write-Host "=== Install verification ==="
if (Test-Path $adkRoot) {
    Write-Host "  ADK base: $adkRoot"
    $depTools = Test-Path "$adkRoot\Deployment Tools"
    $winpeAdd = Test-Path "$adkRoot\Windows Preinstallation Environment"
    Write-Host ("  Deployment Tools: {0}" -f $(if ($depTools) {'YES'} else {'MISSING'}))
    Write-Host ("  WinPE add-on:     {0}" -f $(if ($winpeAdd) {'YES'} else {'MISSING'}))

    $copype = Get-ChildItem $adkRoot -Recurse -Filter 'copype.cmd' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($copype) {
        Write-Host "  copype.cmd:          $($copype.FullName)"
    } else {
        Write-Host "  copype.cmd:          MISSING (WinPE add-on not installed properly?)"
    }

    $makewinpe = Get-ChildItem $adkRoot -Recurse -Filter 'MakeWinPEMedia.cmd' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($makewinpe) {
        Write-Host "  MakeWinPEMedia.cmd:  $($makewinpe.FullName)"
    } else {
        Write-Host "  MakeWinPEMedia.cmd:  MISSING"
    }

    $dismFile = Get-ChildItem $adkRoot -Recurse -Filter 'dism.exe' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -like '*amd64*' } | Select-Object -First 1
    if ($dismFile) {
        Write-Host "  ADK dism.exe amd64:  $($dismFile.FullName)"
    } else {
        Write-Host "  ADK dism.exe amd64:  not found separately (system dism works too)"
    }
} else {
    Write-Error "ADK install failed - $adkRoot does not exist"
}

Write-Host ""
Write-Host "[DONE] Paste the verification output above to Claude to continue stage 6.2"

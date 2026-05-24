# collect-crt-redist.ps1
# Pure English (PS 5.1 GBK decode hazard for non-BOM Chinese .ps1).
# Collects VCRUNTIME140.dll + all 15 api-ms-win-crt-*.dll + ucrtbase.dll
# into pe-build\payload\crt-redist\ for 04-add-payload.ps1 [2.5/5] to consume.
#
# Usage (PowerShell):
#     C:\NeuroBoot\tools-dev\collect-crt-redist.ps1
#
# Sources:
# - VCRUNTIME140        <- VS 2026 redist (latest MSVC redist subdir)
# - api-ms-win-crt-*    <- Win10 SDK Redist (NOT System32 - modern Windows
#                          wraps these into ucrtbase's API set, so System32
#                          does not contain the individual stub DLLs)
# - ucrtbase.dll        <- C:\Windows\System32 (the actual implementation)
#
# See docs/TODO-v1.0.1-fixes.md section 1 for root cause analysis.

$ErrorActionPreference = 'Stop'

$target = 'C:\NeuroBoot\pe-build\payload\crt-redist'
New-Item -ItemType Directory -Path $target -Force | Out-Null

Write-Host "[collect-crt-redist] target = $target"
Write-Host ""

# === 1. VCRUNTIME140.dll ===
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found at $vswhere - VS 2026 not installed?" }
$vsroot = & $vswhere -latest -property installationPath
$redistPattern = "$vsroot\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT\vcruntime140.dll"
$redistFile = Get-Item $redistPattern -ErrorAction SilentlyContinue |
    Sort-Object { [version]($_.Directory.Parent.Parent.Name) } -Descending |
    Select-Object -First 1
if ($null -eq $redistFile) { throw "vcruntime140.dll not found under $vsroot\VC\Redist\MSVC\" }
Write-Host "[1/3] vcruntime140.dll <- $($redistFile.FullName)"
Copy-Item $redistFile.FullName $target -Force

# === 2. api-ms-win-crt-*.dll (UCRT umbrella) from Windows SDK Redist ===
$sdkRedistBase = "${env:ProgramFiles(x86)}\Windows Kits\10\Redist"
$ucrtSrc = Get-ChildItem $sdkRedistBase -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like '10.*' } |
    Sort-Object Name -Descending |
    Where-Object { Test-Path "$($_.FullName)\ucrt\DLLs\x64" } |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $ucrtSrc) { throw "Win10 SDK ucrt redist not found under $sdkRedistBase" }
$ucrtSrc = "$ucrtSrc\ucrt\DLLs\x64"
Write-Host "[2/3] api-ms-win-crt-*.dll <- $ucrtSrc"

$ucrt = @(
    'api-ms-win-crt-runtime-l1-1-0.dll',
    'api-ms-win-crt-stdio-l1-1-0.dll',
    'api-ms-win-crt-math-l1-1-0.dll',
    'api-ms-win-crt-locale-l1-1-0.dll',
    'api-ms-win-crt-heap-l1-1-0.dll',
    'api-ms-win-crt-string-l1-1-0.dll',
    'api-ms-win-crt-time-l1-1-0.dll',
    'api-ms-win-crt-convert-l1-1-0.dll',
    'api-ms-win-crt-utility-l1-1-0.dll',
    'api-ms-win-crt-filesystem-l1-1-0.dll',
    'api-ms-win-crt-environment-l1-1-0.dll',
    'api-ms-win-crt-process-l1-1-0.dll',
    'api-ms-win-crt-conio-l1-1-0.dll',
    'api-ms-win-crt-multibyte-l1-1-0.dll',
    'api-ms-win-crt-private-l1-1-0.dll'
)
$missing = @()
foreach ($d in $ucrt) {
    $src = Join-Path $ucrtSrc $d
    if (Test-Path $src) {
        Copy-Item $src $target -Force
    } else {
        # Fallback: System32\downlevel\
        $alt = "C:\Windows\System32\downlevel\$d"
        if (Test-Path $alt) {
            Copy-Item $alt $target -Force
        } else {
            $missing += $d
        }
    }
}
if ($missing.Count -gt 0) { Write-Warning "Missing UCRT umbrella DLLs: $($missing -join ', ')" }

# === 3. ucrtbase.dll ===
$ucrtbase = 'C:\Windows\System32\ucrtbase.dll'
if (-not (Test-Path $ucrtbase)) { throw "ucrtbase.dll not found at $ucrtbase" }
Write-Host "[3/3] ucrtbase.dll <- $ucrtbase"
Copy-Item $ucrtbase $target -Force

Write-Host ""
Write-Host "=== Final contents ==="
Get-ChildItem $target | Sort-Object Name | ForEach-Object {
    "{0,-45} {1,8:N0} KB" -f $_.Name, [math]::Round($_.Length/1KB)
}
$count = (Get-ChildItem $target).Count
$totalKB = [math]::Round(((Get-ChildItem $target | Measure-Object Length -Sum).Sum)/1KB, 0)
Write-Host ""
Write-Host "[DONE] $count files, $totalKB KB total. Ready for 04-add-payload.ps1 [2.5/5]."

# 03-mount-and-customize.ps1
# Stage 6.4: DISM mount boot.wim + Add WinPE optional components.
# Pure English to avoid PS 5.1 GBK parsing issues.
# Must be run with admin elevation (DISM mount needs admin).
# Writes transcript to workspace\stage64.log for caller to read.

$logFile = 'C:\NeuroBoot\pe-build\workspace\stage64.log'
Start-Transcript -Path $logFile -Force | Out-Null

try {
    $adkRoot = 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit'
    $ocs = "$adkRoot\Windows Preinstallation Environment\amd64\WinPE_OCs"
    $wim = 'C:\NeuroBoot\pe-build\workspace\media\sources\boot.wim'
    $mount = 'C:\NeuroBoot\pe-build\workspace\mount'
    $dism = "$adkRoot\Deployment Tools\amd64\DISM\dism.exe"

    if (-not (Test-Path $dism)) { throw "ADK dism not found: $dism" }
    if (-not (Test-Path $wim)) { throw "boot.wim not found: $wim - run copype first" }
    if (-not (Test-Path $mount)) { throw "mount dir not found: $mount" }

    # Clean any stale mount state (idempotent rerun)
    Write-Host "=== Cleanup any stale DISM mount ==="
    & $dism /Cleanup-Wim 2>&1 | Out-Null
    & $dism /Unmount-Image /MountDir:$mount /Discard 2>&1 | Out-Null

    Write-Host ""
    Write-Host "=== Mount boot.wim Index 1 to $mount ==="
    & $dism /Mount-Image /ImageFile:$wim /Index:1 /MountDir:$mount
    if ($LASTEXITCODE -ne 0) { throw "Mount failed (exit $LASTEXITCODE)" }

    # OC dependency order: WMI -> NetFx -> Scripting -> PowerShell -> StorageWMI
    # WinPE-StorageWMI needs WMI + Scripting; gives Get-Disk / Get-Partition cmdlets
    # v1.0.1: + WinPE-FontSupport-ZH-CN so PE cmd / system dialogs render Chinese
    # (NeuroBoot.exe has its own embedded Noto Sans SC, but llama-server log /
    # startnet.cmd output / system error dialogs need this CAB).
    $ocList = @(
        'WinPE-WMI',
        'WinPE-NetFx',
        'WinPE-Scripting',
        'WinPE-PowerShell',
        'WinPE-StorageWMI',
        'WinPE-FontSupport-ZH-CN'
    )

    foreach ($oc in $ocList) {
        $mainCab = "$ocs\$oc.cab"
        $langCab = "$ocs\en-us\${oc}_en-us.cab"

        Write-Host ""
        Write-Host "=== Adding $oc ==="
        if (-not (Test-Path $mainCab)) {
            Write-Warning "Main cab not found: $mainCab"
            continue
        }
        & $dism /Image:$mount /Add-Package /PackagePath:$mainCab
        if ($LASTEXITCODE -ne 0) { throw "$oc main failed (exit $LASTEXITCODE)" }

        if (Test-Path $langCab) {
            & $dism /Image:$mount /Add-Package /PackagePath:$langCab
            if ($LASTEXITCODE -ne 0) { throw "$oc lang pack failed (exit $LASTEXITCODE)" }
        } else {
            Write-Host "  (no en-us lang pack for $oc)"
        }
    }

    Write-Host ""
    Write-Host "=== Verifying installed packages ==="
    & $dism /Image:$mount /Get-Packages | Select-String 'WinPE-' | Select-Object -First 30

    Write-Host ""
    Write-Host "[DONE] Image still mounted at $mount (will commit in stage 6.6)"
} catch {
    Write-Error "Stage 6.4 failed: $_"
    exit 1
} finally {
    Stop-Transcript | Out-Null
}

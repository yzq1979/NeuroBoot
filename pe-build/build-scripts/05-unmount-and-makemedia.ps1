# 05-unmount-and-makemedia.ps1
# Stage 6.6: DISM /Unmount-Image /Commit + MakeWinPEMedia /ISO.
# Pure English. Admin required.

$logFile = 'C:\NeuroBoot\pe-build\workspace\stage66.log'
Start-Transcript -Path $logFile -Force | Out-Null

try {
    $adkRoot   = 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit'
    $mount     = 'C:\NeuroBoot\pe-build\workspace\mount'
    $workspace = 'C:\NeuroBoot\pe-build\workspace'
    $output    = 'C:\NeuroBoot\pe-build\output'
    $iso       = "$output\NeuroBoot.iso"
    $dism      = "$adkRoot\Deployment Tools\amd64\DISM\dism.exe"
    $makewinpe = "$adkRoot\Windows Preinstallation Environment\MakeWinPEMedia.cmd"
    $dandiset  = "$adkRoot\Deployment Tools\DandISetEnv.bat"

    foreach ($p in @($dism, $makewinpe, $dandiset)) {
        if (-not (Test-Path $p)) { throw "Missing ADK tool: $p" }
    }
    if (-not (Test-Path "$mount\NeuroBoot\neuroboot.exe")) {
        throw "NeuroBoot.exe not found in mount - did stage 6.5 succeed?"
    }

    # Prep output dir
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    if (Test-Path $iso) {
        Remove-Item $iso -Force
        Write-Host "Removed previous ISO: $iso"
    }

    # ---- Phase 1: Unmount /Commit ----
    Write-Host ""
    Write-Host "=== [1/2] DISM /Unmount-Image /Commit ==="
    Write-Host "    Packing mount contents back into boot.wim..."
    Write-Host "    This compresses 2.4+ GB payload; expect 5-15 min."
    $tStart = Get-Date
    & $dism /Unmount-Image /MountDir:$mount /Commit
    $tElapsed = (Get-Date) - $tStart
    if ($LASTEXITCODE -ne 0) { throw "Unmount /Commit failed (exit $LASTEXITCODE)" }
    Write-Host ("    Done in {0:N1} min" -f $tElapsed.TotalMinutes)

    $bw = Get-Item "$workspace\media\sources\boot.wim"
    Write-Host ("    New boot.wim size: {0:N2} GB" -f ($bw.Length/1GB))

    # ---- Phase 2: MakeWinPEMedia /ISO ----
    Write-Host ""
    Write-Host "=== [2/2] MakeWinPEMedia /ISO ==="
    Write-Host "    Building bootable ISO via oscdimg (BIOS+UEFI hybrid)..."
    $tStart = Get-Date

    # MakeWinPEMedia needs OSCDImgRoot from DandISetEnv. Wrap via cmd to source env first.
    $cmdLine = "call `"$dandiset`" >nul && `"$makewinpe`" /ISO `"$workspace`" `"$iso`""
    cmd /c $cmdLine
    $tElapsed = (Get-Date) - $tStart
    if ($LASTEXITCODE -ne 0) { throw "MakeWinPEMedia failed (exit $LASTEXITCODE)" }
    Write-Host ("    Done in {0:N1} min" -f $tElapsed.TotalMinutes)

    # ---- Verify ----
    if (-not (Test-Path $iso)) { throw "ISO file not created at $iso" }
    $isoSizeGB = (Get-Item $iso).Length / 1GB

    Write-Host ""
    Write-Host "==============================================="
    Write-Host ("[OK] NeuroBoot.iso generated: {0}" -f $iso)
    Write-Host ("     Size: {0:N2} GB" -f $isoSizeGB)
    Write-Host "==============================================="
    Write-Host ""
    Write-Host "Next: Stage 6.7 - install Ventoy on the Kingston 32GB USB,"
    Write-Host "      then drop this ISO into the Ventoy data partition."
} catch {
    Write-Error "Stage 6.6 failed: $_"
    exit 1
} finally {
    Stop-Transcript | Out-Null
}

# setup-new-usb.ps1
# Semi-automated: deploy NeuroBoot.iso to a USB stick.
# - Detects USB removable drives
# - Asks user to pick target (avoids accidental wipe of wrong drive)
# - If target is already a Ventoy disk: skip install, copy ISO only
# - If target is NOT Ventoy: opens Ventoy2Disk.exe GUI for user to manually
#   install Ventoy (safer than CLI which auto-formats with no UI confirmation),
#   waits for user to close GUI, then copies ISO to new Ventoy data partition.
#
# Pure English. Admin required (Ventoy install needs admin).

#Requires -RunAsAdministrator

$ErrorActionPreference = 'Stop'

# ---- Paths ----
$iso = 'C:\NeuroBoot\pe-build\output\NeuroBoot.iso'
$ventoyDir = Get-ChildItem 'C:\NeuroBoot\tools-dev\ventoy' -Directory -Filter 'ventoy-*' -ErrorAction SilentlyContinue |
             Select-Object -First 1
if (-not $ventoyDir) {
    Write-Error "Ventoy not found at C:\NeuroBoot\tools-dev\ventoy\ventoy-*. Run docs/BUILD.md section 4.1 first."
    exit 1
}
$ventoy2disk = Join-Path $ventoyDir.FullName 'Ventoy2Disk.exe'
if (-not (Test-Path $ventoy2disk)) {
    Write-Error "Ventoy2Disk.exe not found at $ventoy2disk"
    exit 1
}
if (-not (Test-Path $iso)) {
    Write-Error "NeuroBoot.iso not found at $iso. Run pe-build/build-scripts/99-build-all.ps1 first."
    exit 1
}

$isoSizeGB = (Get-Item $iso).Length / 1GB
Write-Host ""
Write-Host ("NeuroBoot.iso ready: {0} ({1:N2} GB)" -f $iso, $isoSizeGB)
Write-Host ("Ventoy2Disk.exe:     {0}" -f $ventoy2disk)
Write-Host ""

# ---- List USB removable drives ----
Write-Host "=== Detected USB removable drives ==="
$usbs = @(Get-Volume |
          Where-Object { $_.DriveType -eq 'Removable' -and $_.DriveLetter } |
          Sort-Object DriveLetter)
if ($usbs.Count -eq 0) {
    Write-Error "No removable USB drive detected. Insert a USB stick and retry."
    exit 1
}
$i = 0
foreach ($v in $usbs) {
    $i++
    "[{0}] Drive {1}:  Label='{2}'  FS={3}  Size={4:N2} GB  Free={5:N2} GB" -f `
        $i, $v.DriveLetter, $v.FileSystemLabel, $v.FileSystem, ($v.Size/1GB), ($v.SizeRemaining/1GB)
}

# ---- User picks target ----
Write-Host ""
$choice = Read-Host "Pick USB number (1-$($usbs.Count)) or 'q' to quit"
if ($choice -eq 'q' -or $choice -eq 'Q') { Write-Host "Cancelled."; exit 0 }
$idx = 0
if (-not [int]::TryParse($choice, [ref]$idx) -or $idx -lt 1 -or $idx -gt $usbs.Count) {
    Write-Error "Invalid choice: $choice"
    exit 1
}
$target = $usbs[$idx - 1]
$targetDrive = "$($target.DriveLetter):"
$isVentoy = $target.FileSystemLabel -eq 'Ventoy'

Write-Host ""
Write-Host "Selected: $targetDrive (Label='$($target.FileSystemLabel)', $([math]::Round($target.Size/1GB, 2)) GB)"

# ---- Branch: already Ventoy or new install ----
if ($isVentoy) {
    Write-Host ""
    Write-Host "[INFO] $targetDrive looks like an existing Ventoy disk (label 'Ventoy')."
    Write-Host "       Skipping Ventoy install. Will overwrite any existing NeuroBoot.iso."
    $confirm = Read-Host "Continue? (y/N)"
    if ($confirm -ne 'y' -and $confirm -ne 'Y') { Write-Host "Cancelled."; exit 0 }
} else {
    Write-Host ""
    Write-Host "[WARN] $targetDrive is NOT a Ventoy disk (label '$($target.FileSystemLabel)')."
    Write-Host "       To proceed, you need to install Ventoy onto $targetDrive."
    Write-Host "       Ventoy install will FORMAT THE ENTIRE DRIVE - ALL DATA LOST."
    Write-Host ""
    $confirm = Read-Host "Type 'WIPE $targetDrive' (literally) to launch Ventoy GUI for install"
    if ($confirm -ne "WIPE $targetDrive") { Write-Host "Cancelled."; exit 0 }

    Write-Host ""
    Write-Host "Launching Ventoy2Disk.exe GUI..."
    Write-Host "  In the GUI:"
    Write-Host "    1. Make sure Device dropdown shows your USB ($targetDrive)"
    Write-Host "    2. Click Install"
    Write-Host "    3. Confirm 'Will erase all data' (twice)"
    Write-Host "    4. Wait for 'Ventoy install successfully' dialog"
    Write-Host "    5. CLOSE the Ventoy GUI window"
    Write-Host ""

    # Launch Ventoy GUI and wait for it to close
    $proc = Start-Process $ventoy2disk -PassThru
    $proc.WaitForExit()
    Write-Host "Ventoy GUI closed. Re-scanning drives..."
    Start-Sleep -Seconds 2

    # Re-detect the Ventoy data partition (drive letter may have changed slightly)
    $newVentoy = @(Get-Volume |
                   Where-Object { $_.FileSystemLabel -eq 'Ventoy' -and $_.DriveType -eq 'Removable' } |
                   Sort-Object DriveLetter)
    if ($newVentoy.Count -eq 0) {
        Write-Error "No Ventoy data partition found after install. Did install succeed?"
        exit 1
    }
    if ($newVentoy.Count -gt 1) {
        Write-Warning "Multiple Ventoy disks detected. Using the first one."
    }
    $target = $newVentoy[0]
    $targetDrive = "$($target.DriveLetter):"
    Write-Host "[OK] Ventoy data partition at $targetDrive"
}

# ---- Copy ISO ----
Write-Host ""
Write-Host "=== Copying NeuroBoot.iso to $targetDrive ==="
$dest = Join-Path $targetDrive 'NeuroBoot.iso'
$tStart = Get-Date
Copy-Item $iso -Destination $dest -Force
$tElapsed = (Get-Date) - $tStart
$f = Get-Item $dest
Write-Host ("[OK] Copied {0:N2} GB in {1:N1} s ({2:N1} MB/s)" -f `
    ($f.Length/1GB), $tElapsed.TotalSeconds, (($f.Length/1MB) / $tElapsed.TotalSeconds))

Write-Host ""
Write-Host "===================================================="
Write-Host "[DONE] USB $targetDrive is ready to boot NeuroBoot PE."
Write-Host ""
Write-Host "Next steps on TARGET MACHINE:"
Write-Host "  1. Boot with USB plugged in. Reboot and press F12 (Lenovo)"
Write-Host "     or your machine's boot-menu hotkey (Esc/F8/F11)."
Write-Host "  2. If Secure Boot blocks, enter BIOS, disable Secure Boot."
Write-Host "  3. Pick USB from boot menu."
Write-Host "  4. Ventoy menu appears -> select NeuroBoot.iso -> 'Boot in normal mode'."
Write-Host "  5. Wait ~90 seconds (PE load + 60s llama-server warmup)."
Write-Host "  6. NeuroBoot window opens. Use it."
Write-Host "  7. Inside PE cmd, type 'wpeutil shutdown' to power off."
Write-Host "===================================================="

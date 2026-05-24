# 99-build-all.ps1
# One-shot NeuroBoot PE ISO build pipeline.
# Pure English. Admin required.
#
# Prerequisites (run BUILD.md sections 1-1.7 first):
#   - Windows ADK 10.1.26100.2454 + WinPE add-on installed at default path
#   - Rust 1.92+ msvc target (rustc/cargo in PATH)
#   - VS 2022/2026 Build Tools with C++ workload (Rust msvc linker)
#   - llama.cpp CPU build extracted at C:\NeuroBoot\tools-dev\llama-cpp\b9294\
#   - Qwen GGUF at C:\NeuroBoot\models\Qwen3-4B-Instruct-2507-Q4_K_M.gguf
#   - Mesa-dist-win extracted at C:\NeuroBoot\tools-dev\mesa-extract\
#   - Smart App Control disabled (Win11 25H2)
#
# What this script does:
#   Phase 0: cargo build --release with crt-static
#   Phase 1: 01-collect-neuroboot-payload.ps1
#   Phase 2: 02-run-copype.cmd (init workspace)
#   Phase 3: 03-mount-and-customize.ps1 (mount boot.wim + add 5 WinPE OCs)
#   Phase 4: 04-add-payload.ps1 (copy NeuroBoot + llama.cpp + GGUF + startnet.cmd)
#   Phase 5: 05-unmount-and-makemedia.ps1 (unmount /Commit + MakeWinPEMedia /ISO)
#
# Output: C:\NeuroBoot\pe-build\output\NeuroBoot.iso (~2.9 GB)
# Total time: 5-20 min (depending on cargo cache state)

#Requires -RunAsAdministrator

$ErrorActionPreference = 'Stop'
$root = 'C:\NeuroBoot'
$scripts = "$root\pe-build\build-scripts"
# v1.0.1 fix: log MUST live above workspace/ - Phase 2 does Remove-Item on
# the entire workspace dir, and Start-Transcript holds the log file open,
# causing delete to silently fail and copype to error "destination exists".
$logFile = "$root\pe-build\build-all.log"
$totalStart = Get-Date

Start-Transcript -Path $logFile -Force | Out-Null

try {
    # ---- Sanity check prerequisites ----
    Write-Host "=== Sanity check prerequisites ==="
    $checks = @{
        'Rust cargo'        = (Get-Command cargo -ErrorAction SilentlyContinue) -ne $null
        'ADK base'          = Test-Path 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools'
        'WinPE add-on'      = Test-Path 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Windows Preinstallation Environment'
        'llama.cpp b9294'   = Test-Path "$root\tools-dev\llama-cpp\b9294\llama-server.exe"
        'Qwen GGUF'         = Test-Path "$root\models\Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
        'Mesa opengl32'     = Test-Path "$root\tools-dev\mesa-extract\x64\opengl32.dll"
        'Mesa libgallium'   = Test-Path "$root\tools-dev\mesa-extract\x64\libgallium_wgl.dll"
        'NeuroBoot Cargo.toml' = Test-Path "$root\app\Cargo.toml"
        # v1.0.1 fix: llama-server release build needs CRT redist DLLs in same dir.
        # If missing, run tools-dev\collect-crt-redist.ps1 to populate this directory.
        'CRT redist (v1.0.1)'  = (Test-Path "$root\pe-build\payload\crt-redist\vcruntime140.dll") -and
                                  (Test-Path "$root\pe-build\payload\crt-redist\ucrtbase.dll") -and
                                  (Test-Path "$root\pe-build\payload\crt-redist\api-ms-win-crt-runtime-l1-1-0.dll")
    }
    $missing = @()
    foreach ($k in $checks.Keys) {
        $ok = $checks[$k]
        "  {0,-22} {1}" -f $k, $(if ($ok) { 'OK' } else { 'MISSING' })
        if (-not $ok) { $missing += $k }
    }
    if ($missing.Count -gt 0) {
        throw "Prerequisites missing: $($missing -join ', '). See docs/BUILD.md sections 1.1-1.6"
    }

    # ---- Phase 0: cargo build --release with crt-static ----
    Write-Host ""
    Write-Host "=== [Phase 0] cargo build --release (crt-static) ==="
    $t0 = Get-Date
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    # Kill stale neuroboot.exe that may hold target/release/neuroboot.exe file lock
    Get-Process neuroboot -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    cargo build --release --manifest-path "$root\app\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 0 done in {0:N1} min" -f $t.TotalMinutes)

    # ---- Phase 1: collect payload ----
    Write-Host ""
    Write-Host "=== [Phase 1] Collect NeuroBoot payload ==="
    $t0 = Get-Date
    & "$scripts\01-collect-neuroboot-payload.ps1"
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) { throw "Phase 1 failed" }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 1 done in {0:N1} s" -f $t.TotalSeconds)

    # ---- Phase 2: copype workspace ----
    Write-Host ""
    Write-Host "=== [Phase 2] copype amd64 workspace ==="
    $t0 = Get-Date
    # Clean any stale mount first (DISM holds mount/ across runs)
    & 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools\amd64\DISM\dism.exe' /Unmount-Image /MountDir:"$root\pe-build\workspace\mount" /Discard 2>&1 | Out-Null
    if (Test-Path "$root\pe-build\workspace") {
        Remove-Item "$root\pe-build\workspace" -Recurse -Force -ErrorAction Stop
    }
    cmd /c "$scripts\02-run-copype.cmd"
    if ($LASTEXITCODE -ne 0) { throw "Phase 2 (copype) failed (exit $LASTEXITCODE)" }
    if (-not (Test-Path "$root\pe-build\workspace\media\sources\boot.wim")) {
        throw "Phase 2: boot.wim not produced"
    }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 2 done in {0:N1} s" -f $t.TotalSeconds)

    # ---- Phase 3: mount + add OCs ----
    Write-Host ""
    Write-Host "=== [Phase 3] Mount boot.wim + add WinPE OCs ==="
    $t0 = Get-Date
    & "$scripts\03-mount-and-customize.ps1"
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) { throw "Phase 3 failed" }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 3 done in {0:N1} min" -f $t.TotalMinutes)

    # ---- Phase 4: add payload to mount ----
    Write-Host ""
    Write-Host "=== [Phase 4] Add NeuroBoot payload to mount ==="
    $t0 = Get-Date
    & "$scripts\04-add-payload.ps1"
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) { throw "Phase 4 failed" }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 4 done in {0:N1} min" -f $t.TotalMinutes)

    # ---- Phase 5: unmount /commit + MakeWinPEMedia /ISO ----
    Write-Host ""
    Write-Host "=== [Phase 5] Unmount /Commit + MakeWinPEMedia /ISO ==="
    $t0 = Get-Date
    & "$scripts\05-unmount-and-makemedia.ps1"
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) { throw "Phase 5 failed" }
    $t = (Get-Date) - $t0
    Write-Host ("  Phase 5 done in {0:N1} min" -f $t.TotalMinutes)

    # ---- Verify final ISO ----
    $iso = "$root\pe-build\output\NeuroBoot.iso"
    if (-not (Test-Path $iso)) { throw "Final ISO not created at $iso" }
    $isoSizeGB = (Get-Item $iso).Length / 1GB
    $totalElapsed = (Get-Date) - $totalStart

    Write-Host ""
    Write-Host "==============================================="
    Write-Host "[BUILD COMPLETE]"
    Write-Host ("  NeuroBoot.iso: {0}" -f $iso)
    Write-Host ("  Size:          {0:N2} GB" -f $isoSizeGB)
    Write-Host ("  Total time:    {0:N1} min" -f $totalElapsed.TotalMinutes)
    Write-Host "==============================================="
    Write-Host ""
    Write-Host "Next steps (see docs/BUILD.md section 4-5):"
    Write-Host "  1. Run Ventoy2Disk.exe (admin) -> Install to your USB"
    Write-Host "  2. Copy NeuroBoot.iso to USB root"
    Write-Host "  3. BIOS: disable Secure Boot, enable USB boot"
    Write-Host "  4. F12 boot menu -> select USB -> Ventoy menu -> NeuroBoot.iso"
} catch {
    Write-Error "Build failed: $_"
    exit 1
} finally {
    Stop-Transcript | Out-Null
}

# 04-add-payload.ps1
# Stage 6.5: Copy NeuroBoot + llama.cpp + Qwen GGUF into the mounted PE,
# write startnet.cmd to auto-launch NeuroBoot on PE boot.
# Pure English. Admin required (mount dir owned by admin).

$logFile = 'C:\NeuroBoot\pe-build\workspace\stage65.log'
Start-Transcript -Path $logFile -Force | Out-Null

try {
    $mount             = 'C:\NeuroBoot\pe-build\workspace\mount'
    $payloadNeuroBoot  = 'C:\NeuroBoot\pe-build\payload\neuroboot'
    $llamaSrc          = 'C:\NeuroBoot\tools-dev\llama-cpp\b9294'
    $crtRedist         = 'C:\NeuroBoot\pe-build\payload\crt-redist'
    # v2 Stage 1.3: upgrade Q4_K_M (2.32 GB) -> Q5_K_M (2.69 GB).
    # Q5_K_M shows hands-down better tool-calling accuracy on 4B models
    # per llama.cpp / unsloth guidance (Q4 is the lower bound for 4B).
    # +370 MB to ISO size, acceptable tradeoff.
    $modelSrc          = 'C:\NeuroBoot\models\Qwen3-4B-Instruct-2507-Q5_K_M.gguf'

    if (-not (Test-Path "$mount\Windows\System32\startnet.cmd")) {
        throw "Mount dir does not look like a mounted PE (no startnet.cmd). Did stage 6.4 mount succeed?"
    }
    if (-not (Test-Path $payloadNeuroBoot)) { throw "NeuroBoot payload not found: $payloadNeuroBoot" }
    if (-not (Test-Path $llamaSrc)) { throw "llama.cpp not found: $llamaSrc" }
    if (-not (Test-Path $crtRedist)) { throw "CRT redist dir not found: $crtRedist (run tools-dev/collect-crt-redist.ps1 first)" }
    if (-not (Test-Path $modelSrc)) { throw "Qwen GGUF not found: $modelSrc" }

    $peNeuroBoot = "$mount\NeuroBoot"
    $peLlama     = "$mount\NeuroBoot\llama-cpp"
    $peModels    = "$mount\NeuroBoot\models"

    Write-Host "=== [1/5] Copying NeuroBoot.exe + Mesa DLLs ==="
    New-Item -ItemType Directory -Path $peNeuroBoot -Force | Out-Null
    Copy-Item "$payloadNeuroBoot\*" -Destination $peNeuroBoot -Force
    Get-ChildItem $peNeuroBoot -File | ForEach-Object { '  {0,-30} {1,7:N2} MB' -f $_.Name, ($_.Length/1MB) }

    Write-Host ""
    Write-Host "=== [2/5] Copying llama.cpp b9294 (entire dir, CPU build) ==="
    New-Item -ItemType Directory -Path $peLlama -Force | Out-Null
    Copy-Item "$llamaSrc\*" -Destination $peLlama -Force -Recurse
    $llamaFiles = Get-ChildItem $peLlama -File
    $llamaTotalMB = [math]::Round((($llamaFiles | Measure-Object Length -Sum).Sum)/1MB,1)
    "  Files: $($llamaFiles.Count), Total: $llamaTotalMB MB"

    Write-Host ""
    Write-Host "=== [2.5/5] Copying CRT redist DLLs into llama-cpp dir (v1.0.1 fix) ==="
    # Why: llama.cpp b9294 release build is NOT +crt-static. In PE it crashes
    # on load because VCRUNTIME140 + api-ms-win-crt-* + ucrtbase are missing.
    # Copying them next to llama-server.exe makes Windows loader local-load
    # them first. See docs/TODO-v1.0.1-fixes.md section 1 for root cause.
    Copy-Item "$crtRedist\*.dll" -Destination $peLlama -Force
    $crtAdded = Get-ChildItem $peLlama -Filter '*.dll' |
        Where-Object { $_.Name -match '^(vcruntime|api-ms-win-crt|ucrtbase)' }
    $crtTotalKB = [math]::Round((($crtAdded | Measure-Object Length -Sum).Sum)/1KB,0)
    "  Added $($crtAdded.Count) CRT DLLs ($crtTotalKB KB) to $peLlama"
    if ($crtAdded.Count -lt 17) {
        Write-Warning "Expected 17 CRT DLLs (1 vcruntime + 15 api-ms-win-crt + 1 ucrtbase), got $($crtAdded.Count)"
    }

    Write-Host ""
    Write-Host "=== [2.6/5] Copying external tools (v3 Quick Win + Stage 6, optional) ==="
    # Per docs/BUILD.md "External tools download" section: NeuroBoot ISO default skips 3rd-party
    # binaries (NTPWEdit / TestDisk / smartctl / 7za / BlueScreenView) for
    # license/size reasons. User downloads them to C:\NeuroBoot\tools\<name>\,
    # this step copies the whole dir into PE mount\NeuroBoot\tools\.
    # If C:\NeuroBoot\tools\ does not exist, skip silently - the corresponding
    # AI tools will return NotFound at runtime with a doc pointer.
    $extToolsSrc = 'C:\NeuroBoot\tools'
    $peTools = "$mount\NeuroBoot\tools"
    if (Test-Path $extToolsSrc) {
        New-Item -ItemType Directory -Path $peTools -Force | Out-Null
        $rcOut = robocopy $extToolsSrc $peTools /MIR /NFL /NDL /NJH /NJS /NP /R:1 /W:1
        # robocopy exit code: 0/1/2/3 = success, >=8 = failure
        $rcExit = $LASTEXITCODE
        if ($rcExit -lt 8) {
            $tFiles = Get-ChildItem $peTools -Recurse -File -ErrorAction SilentlyContinue
            $tBytes = ($tFiles | Measure-Object Length -Sum).Sum
            "  Copied $($tFiles.Count) external tool files ($([math]::Round($tBytes/1MB,1)) MB) to $peTools"
            $LASTEXITCODE = 0
        } else {
            Write-Warning "robocopy reported failure (exit $rcExit); some external tools may be missing in PE"
        }
    } else {
        "  Skipped - no $extToolsSrc dir (run download-external-tools.ps1 to populate it)"
    }

    Write-Host ""
    Write-Host "=== [2.7/5] Copying skill templates (v3.0 W2-3) ==="
    # 8 个 distributed skill 模板（YAML frontmatter + body）拷到 PE 的 \NeuroBoot\skills\。
    # AI 启动时 scan_skills() 扫这个目录加载 SkillSummary 进 system prompt（progressive
    # disclosure tier 1）；AI 调 load_skill(name) 触发 lazy load body（tier 2）。
    # 用户 U 盘根 \NeuroBoot\skills\*.md 也会被扫，可覆盖 / 扩展（详见 docs/usb-templates/skills/README）。
    $skillsSrc = 'C:\NeuroBoot\docs\usb-templates\skills'
    $peSkills = "$mount\NeuroBoot\skills"
    if (Test-Path $skillsSrc) {
        New-Item -ItemType Directory -Path $peSkills -Force | Out-Null
        $copied = 0
        foreach ($f in Get-ChildItem $skillsSrc -Filter '*.md' -File) {
            Copy-Item $f.FullName -Destination $peSkills -Force
            $copied++
        }
        "  Copied $copied skill template(s) to $peSkills"
    } else {
        "  Skipped - no $skillsSrc dir (skill templates missing from source tree)"
    }

    Write-Host ""
    Write-Host "=== [3/5] Copying Qwen3-4B GGUF model (2.4 GB, slow) ==="
    New-Item -ItemType Directory -Path $peModels -Force | Out-Null
    $tStart = Get-Date
    Copy-Item $modelSrc -Destination $peModels -Force
    $tElapsed = (Get-Date) - $tStart
    $modelInPe = Get-ChildItem $peModels -File | Select-Object -First 1
    "  {0} {1:N2} GB (copy took {2:N1} s)" -f $modelInPe.Name, ($modelInPe.Length/1GB), $tElapsed.TotalSeconds

    Write-Host ""
    Write-Host "=== [4/5] Writing X:\NeuroBoot\start-llama-server.cmd ==="
    $startLlama = "$peNeuroBoot\start-llama-server.cmd"
    # v2 Stage 1.5 optimizations:
    # --no-mmap : U-disk FAT32/exFAT may not support reliable mmap;
    #             force read-mode to avoid IO stalls on slow USB sticks.
    # -t : physical core count. We use 4 as a sensible default. Reason:
    #      most PE target machines have 4-8 physical cores; setting -t to
    #      logical cores (hyperthreaded) is anti-optimal for matmul-heavy
    #      LLM inference (per llama.cpp official benchmark guidance).
    #      User can edit this file on USB before boot to tune per-machine.
    # v2 Stage 1.3 (2026-05-24): -m bumped to Q5_K_M GGUF (2.69 GB)
    # Q4_K_M is Q4-tier lower bound for 4B models; Q5_K_M ships visibly
    # better tool-calling reliability per llama.cpp/unsloth guidance.
    # v3 Quick Win 1 (2026-05-24): + --slot-save-path + --cache-reuse for
    # prompt KV cache reuse. Combined with cache_prompt=true in request
    # body, this gives ~93% TTFT reduction on follow-up turns. The slots
    # dir lives on X:\ ramdisk so it does not persist across PE reboots;
    # that's fine - within a single session is where the win is.
    # See https://github.com/ggml-org/llama.cpp/discussions/13606
    $llamaCmd = @'
@echo off
REM Launches llama-server with the bundled Qwen3-4B GGUF on PE.
REM X: is the PE ramdisk drive letter.
cd /d X:\NeuroBoot\llama-cpp
if not exist X:\NeuroBoot\slots mkdir X:\NeuroBoot\slots
llama-server.exe ^
  -m X:\NeuroBoot\models\Qwen3-4B-Instruct-2507-Q5_K_M.gguf ^
  -a qwen3-4b-instruct ^
  --host 127.0.0.1 ^
  --port 8080 ^
  -c 16384 ^
  -ngl 0 ^
  -t 4 ^
  --no-mmap ^
  --slot-save-path X:\NeuroBoot\slots ^
  --cache-reuse 256
'@
    [System.IO.File]::WriteAllText($startLlama, $llamaCmd, [System.Text.ASCIIEncoding]::new())
    "  Written: $startLlama"

    Write-Host ""
    Write-Host "=== [5/5] Overwriting startnet.cmd (PE auto-launch NeuroBoot) ==="
    $startnet = "$mount\Windows\System32\startnet.cmd"
    # v1.0.1: replace fixed `timeout /t 60` with PS healthcheck loop on /health.
    # HDD/NVMe + RAM speed differences make fixed wait unreliable; healthcheck
    # exits as soon as llama-server's HTTP /health returns 200 (or after 180s timeout).
    $startnetCmd = @'
@echo off
REM NeuroBoot PE startnet.cmd - auto-launch on PE boot.
REM Flow: wpeinit -> spawn llama-server in background -> poll /health until
REM ready (max 180s) -> launch NeuroBoot GUI.

wpeinit

REM Start llama-server in a minimized background cmd window
start "llama-server" /MIN cmd /c "X:\NeuroBoot\start-llama-server.cmd"

REM Wait for llama-server /health to return 200 (or 180s timeout).
REM PowerShell is provided by WinPE-PowerShell OC (see 03-mount-and-customize.ps1).
echo Waiting for llama-server to be ready (polling /health, max 180s)...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$deadline=(Get-Date).AddSeconds(180); while ((Get-Date) -lt $deadline) { try { $r = Invoke-WebRequest -Uri 'http://127.0.0.1:8080/health' -UseBasicParsing -TimeoutSec 2 -ErrorAction Stop; if ($r.StatusCode -eq 200) { Write-Host '[OK] llama-server ready'; exit 0 } } catch {} ; Start-Sleep -Seconds 2 } ; Write-Host '[WARN] llama-server not ready after 180s (will launch GUI anyway, use the gear icon to configure remote endpoint)'; exit 1"

REM Launch NeuroBoot GUI (blocks startnet.cmd until user closes the window)
cd /d X:\NeuroBoot
neuroboot.exe

REM After NeuroBoot exits, drop back to PE cmd prompt
echo.
echo NeuroBoot closed. You are now in PE cmd shell.
echo Type "wpeutil shutdown" to power off, or "wpeutil reboot" to restart.
'@
    [System.IO.File]::WriteAllText($startnet, $startnetCmd, [System.Text.ASCIIEncoding]::new())
    "  Overwritten: $startnet"

    Write-Host ""
    Write-Host "=== Final NeuroBoot directory tree in mounted PE ==="
    Get-ChildItem "$mount\NeuroBoot" -Recurse |
        Where-Object { -not $_.PSIsContainer } |
        Group-Object DirectoryName |
        ForEach-Object {
            $relDir = $_.Name.Substring($mount.Length)
            Write-Host "  $relDir\"
            $_.Group | Sort-Object Length -Descending | ForEach-Object {
                "    {0,-40} {1,8:N2} MB" -f $_.Name, ($_.Length/1MB)
            }
        }

    $totalPayloadMB = [math]::Round(((Get-ChildItem $peNeuroBoot -Recurse | Measure-Object Length -Sum).Sum)/1MB, 1)
    Write-Host ""
    Write-Host ("=== Total NeuroBoot payload in mount: {0} MB ({1:N2} GB) ===" -f $totalPayloadMB, ($totalPayloadMB/1024))

    Write-Host ""
    Write-Host "[DONE] Stage 6.5 payload integrated. Image still mounted."
    Write-Host "Stage 6.6 will: dism /Unmount-Image /Commit + MakeWinPEMedia /ISO"

    # Force exit code 0 to override $LASTEXITCODE leak from robocopy (Win32
    # exit 1 = "one or more files copied", which is success for our /MIR).
    # PS 5.1 propagates the LAST NATIVE EXE's exit code across `& script.ps1`
    # boundaries regardless of manual `$LASTEXITCODE = 0` assignments inside
    # the child scope. 99-build-all.ps1's wrapper check would otherwise see
    # $LASTEXITCODE=1 and throw "Phase 4 failed" despite stage 6.5 succeeding.
    # exit 0 inside try still runs the finally block (Stop-Transcript).
    exit 0
} catch {
    Write-Error "Stage 6.5 failed: $_"
    exit 1
} finally {
    Stop-Transcript | Out-Null
}

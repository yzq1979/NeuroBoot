# download-external-tools.ps1
# Downloads the 5 external 3rd-party tools that NeuroBoot's AI tools wrap.
# All targets land at C:\NeuroBoot\tools\<name>\ - 04-add-payload.ps1's
# [2.6/5] step then copies them into PE during the next ISO build.
#
# Pure English (PS 5.1 GBK decode safety per feedback_powershell5_ps1_encoding).
#
# Usage:
#     C:\NeuroBoot\tools-dev\download-external-tools.ps1
#
# Tools (also documented in docs/BUILD.md "External tools download"):
#   1. NTPWEdit       ~500 KB  reset Windows local account password
#   2. TestDisk       ~3 MB    repair / recover partition tables
#   3. smartmontools  ~5 MB    detailed SMART data
#   4. 7-Zip Extra    ~1.5 MB  extract .7z/.zip/.rar/.iso/etc
#   5. BlueScreenView ~83 KB   parse BSOD minidumps -> driver attribution
#
# Note: license / commercial-use boundaries vary - see docs/BUILD.md.

$ErrorActionPreference = 'Continue'

$toolsRoot = 'C:\NeuroBoot\tools'
New-Item -ItemType Directory -Path $toolsRoot -Force | Out-Null

# Helper: download with retry, optional size sanity check
function Get-WithRetry {
    param(
        [string]$Url,
        [string]$OutFile,
        [int]$MinBytes = 1000,
        [int]$Retries = 3
    )
    for ($i = 1; $i -le $Retries; $i++) {
        try {
            Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing -TimeoutSec 120 -ErrorAction Stop
            if ((Get-Item $OutFile).Length -ge $MinBytes) {
                return $true
            } else {
                Write-Warning "  download too small ($((Get-Item $OutFile).Length) bytes) - retry $i/$Retries"
            }
        } catch {
            Write-Warning "  attempt $i/$Retries failed: $($_.Exception.Message)"
        }
        Start-Sleep -Seconds 2
    }
    return $false
}

# Helper: extract zip / 7z
function Expand-Generic {
    param(
        [string]$Archive,
        [string]$DestDir
    )
    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
    if ($Archive -match '\.zip$') {
        Expand-Archive -Path $Archive -DestinationPath $DestDir -Force
        return $true
    }
    # for .7z, need 7z.exe - try in PATH or skip
    $sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if ($null -ne $sevenZip) {
        & $sevenZip.Source x -y "-o$DestDir" $Archive | Out-Null
        return $true
    }
    Write-Warning "  cannot extract $Archive - need 7z.exe in PATH or .zip format"
    return $false
}

$tmpDir = "$env:TEMP\neuroboot-extract"
New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

$results = @{}

# === 1. NTPWEdit ===
Write-Host ""
Write-Host "=== [1/5] NTPWEdit (~500 KB, freeware) ==="
$dest = "$toolsRoot\NTPWEdit"
if (Test-Path "$dest\NTPWEdit.exe") {
    Write-Host "  already present at $dest\NTPWEdit.exe, skipping"
    $results['NTPWEdit'] = 'present'
} else {
    $zip = "$tmpDir\ntpwedit.zip"
    $url = 'https://cdslow.org.ru/files/ntpwedit/ntpwedit_0.7_x64.zip'
    if (Get-WithRetry -Url $url -OutFile $zip -MinBytes 100000) {
        Expand-Generic -Archive $zip -DestDir $dest
        # zip may extract NTPWEdit.exe or a subfolder
        $found = Get-ChildItem $dest -Filter 'NTPWEdit.exe' -Recurse | Select-Object -First 1
        if ($null -ne $found -and $found.FullName -ne "$dest\NTPWEdit.exe") {
            Move-Item $found.FullName "$dest\NTPWEdit.exe" -Force
        }
        if (Test-Path "$dest\NTPWEdit.exe") {
            Write-Host "  OK: $dest\NTPWEdit.exe"
            $results['NTPWEdit'] = 'ok'
        } else {
            Write-Warning "  zip extracted but NTPWEdit.exe not found in $dest"
            $results['NTPWEdit'] = 'failed-no-exe'
        }
    } else {
        Write-Warning "  download failed - manually download from https://cdslow.org.ru/en/ntpwedit/index.html and place NTPWEdit.exe at $dest\"
        $results['NTPWEdit'] = 'failed-download'
    }
}

# === 2. TestDisk ===
Write-Host ""
Write-Host "=== [2/5] TestDisk (~3 MB, GPL v2+) ==="
$dest = "$toolsRoot\testdisk"
if (Test-Path "$dest\testdisk_win.exe") {
    Write-Host "  already present at $dest\testdisk_win.exe, skipping"
    $results['TestDisk'] = 'present'
} else {
    $zip = "$tmpDir\testdisk.zip"
    # TestDisk uses versioned URLs; try latest stable 7.2 then fall back to 7.1
    $urls = @(
        'https://www.cgsecurity.org/testdisk-7.2.win64.zip',
        'https://www.cgsecurity.org/testdisk-7.1.win64.zip'
    )
    $ok = $false
    foreach ($url in $urls) {
        if (Get-WithRetry -Url $url -OutFile $zip -MinBytes 500000 -Retries 2) {
            $ok = $true; break
        }
    }
    if ($ok) {
        $extractTmp = "$tmpDir\testdisk-extracted"
        Expand-Generic -Archive $zip -DestDir $extractTmp
        # zip extracts to testdisk-X.X/ subfolder containing testdisk_win.exe
        $found = Get-ChildItem $extractTmp -Filter 'testdisk_win.exe' -Recurse | Select-Object -First 1
        if ($null -ne $found) {
            New-Item -ItemType Directory -Path $dest -Force | Out-Null
            Copy-Item $found.Directory.FullName -Destination $dest -Recurse -Force
            Write-Host "  OK: $dest\testdisk_win.exe"
            $results['TestDisk'] = 'ok'
        } else {
            Write-Warning "  testdisk_win.exe not found in extracted archive"
            $results['TestDisk'] = 'failed-no-exe'
        }
    } else {
        Write-Warning "  download failed - manually download from https://www.cgsecurity.org/wiki/TestDisk_Download and place testdisk_win.exe at $dest\"
        $results['TestDisk'] = 'failed-download'
    }
}

# === 3. smartmontools ===
Write-Host ""
Write-Host "=== [3/5] smartmontools (~5 MB, GPL) ==="
$dest = "$toolsRoot\smartmontools"
if (Test-Path "$dest\smartctl.exe") {
    Write-Host "  already present at $dest\smartctl.exe, skipping"
    $results['smartmontools'] = 'present'
} else {
    Write-Warning "  smartmontools nightly builds dir is HTML-listed; auto-download not implemented."
    Write-Warning "  Manually:"
    Write-Warning "    1. open https://builds.smartmontools.org/"
    Write-Warning "    2. find latest smartmontools-7.X-r####.win32-setup.exe (installer) or .zip (portable)"
    Write-Warning "    3. extract -> place smartctl.exe at $dest\smartctl.exe"
    $results['smartmontools'] = 'manual-required'
}

# === 4. 7-Zip Extra (7za.exe) ===
Write-Host ""
Write-Host "=== [4/5] 7-Zip Extra / 7za.exe (~1.5 MB, LGPL+BSD3) ==="
$dest = "$toolsRoot\7zip"
if (Test-Path "$dest\7za.exe") {
    Write-Host "  already present at $dest\7za.exe, skipping"
    $results['7-Zip'] = 'present'
} else {
    # 7-Zip Extra package URLs change per version; try a few known-good ones
    $urls = @(
        'https://www.7-zip.org/a/7z2409-extra.7z',
        'https://www.7-zip.org/a/7z2408-extra.7z',
        'https://www.7-zip.org/a/7z2407-extra.7z',
        'https://www.7-zip.org/a/7z2301-extra.7z'
    )
    $archive = "$tmpDir\7zextra.7z"
    $ok = $false
    foreach ($url in $urls) {
        if (Get-WithRetry -Url $url -OutFile $archive -MinBytes 500000 -Retries 1) {
            $ok = $true; break
        }
    }
    if ($ok) {
        # bootstrap problem: we need 7z.exe to extract 7z... try a few common install locations
        $sevenZip = $null
        foreach ($p in @(
            'C:\Program Files\7-Zip\7z.exe',
            'C:\Program Files (x86)\7-Zip\7z.exe',
            "$env:LOCALAPPDATA\Programs\7-Zip\7z.exe"
        )) {
            if (Test-Path $p) { $sevenZip = $p; break }
        }
        if ($null -ne $sevenZip) {
            $extractTmp = "$tmpDir\7zextra-extracted"
            New-Item -ItemType Directory -Path $extractTmp -Force | Out-Null
            & $sevenZip x -y "-o$extractTmp" $archive | Out-Null
            $found = Get-ChildItem $extractTmp -Filter '7za.exe' -Recurse | Select-Object -First 1
            if ($null -ne $found) {
                New-Item -ItemType Directory -Path $dest -Force | Out-Null
                Copy-Item $found.FullName "$dest\7za.exe" -Force
                Write-Host "  OK: $dest\7za.exe"
                $results['7-Zip'] = 'ok'
            } else {
                Write-Warning "  7za.exe not found in extra package"
                $results['7-Zip'] = 'failed-no-exe'
            }
        } else {
            Write-Warning "  no 7z.exe installed locally to extract the .7z extra package"
            Write-Warning "  install 7-Zip from https://www.7-zip.org/download.html first, then re-run"
            $results['7-Zip'] = 'bootstrap-needed'
        }
    } else {
        Write-Warning "  download failed - try https://www.7-zip.org/download.html '7-Zip Extra: standalone console version 7za.exe' link"
        $results['7-Zip'] = 'failed-download'
    }
}

# === 5. BlueScreenView ===
Write-Host ""
Write-Host "=== [5/5] BlueScreenView (~83 KB, NirSoft freeware) ==="
$dest = "$toolsRoot\BlueScreenView"
if (Test-Path "$dest\BlueScreenView.exe") {
    Write-Host "  already present at $dest\BlueScreenView.exe, skipping"
    $results['BlueScreenView'] = 'present'
} else {
    $zip = "$tmpDir\bluescreenview.zip"
    # NirSoft x64 build
    $urls = @(
        'https://www.nirsoft.net/utils/bluescreenview-x64.zip',
        'https://www.nirsoft.net/utils/bluescreenview.zip'
    )
    $ok = $false
    foreach ($url in $urls) {
        if (Get-WithRetry -Url $url -OutFile $zip -MinBytes 30000 -Retries 2) {
            $ok = $true; break
        }
    }
    if ($ok) {
        $extractTmp = "$tmpDir\bsv-extracted"
        Expand-Generic -Archive $zip -DestDir $extractTmp
        $found = Get-ChildItem $extractTmp -Filter 'BlueScreenView.exe' -Recurse | Select-Object -First 1
        if ($null -ne $found) {
            New-Item -ItemType Directory -Path $dest -Force | Out-Null
            Copy-Item $found.FullName "$dest\BlueScreenView.exe" -Force
            Write-Host "  OK: $dest\BlueScreenView.exe"
            $results['BlueScreenView'] = 'ok'
        } else {
            Write-Warning "  BlueScreenView.exe not found in zip"
            $results['BlueScreenView'] = 'failed-no-exe'
        }
    } else {
        Write-Warning "  download failed - manually download from https://www.nirsoft.net/utils/blue_screen_view.html"
        $results['BlueScreenView'] = 'failed-download'
    }
}

# === Summary ===
Write-Host ""
Write-Host "=============================================="
Write-Host "Summary:"
foreach ($k in $results.Keys | Sort-Object) {
    $status = $results[$k]
    $icon = switch ($status) {
        'ok'        { '[OK]' }
        'present'   { '[OK] (already present)' }
        default     { '[FAIL] ' + $status }
    }
    "  $icon $k"
}
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. For any [FAIL] entries above, manually download per docs/BUILD.md and place in C:\NeuroBoot\tools\<name>\"
Write-Host "  2. Re-run this script (it skips already-present tools)"
Write-Host "  3. Re-build ISO: 99-build-all.ps1 will auto-copy C:\NeuroBoot\tools\ into PE via [2.6/5] step"
Write-Host "=============================================="

# Cleanup tmp
Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue

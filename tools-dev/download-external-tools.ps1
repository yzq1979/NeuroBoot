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

# Force TLS 1.2+ on PS 5.1. Default SecurityProtocol on PS 5.1 / .NET Framework
# is SSL3 + TLS 1.0, which most modern HTTPS sites reject with "Could not
# establish trust relationship for SSL/TLS secure channel". Enabling TLS 1.2
# (and TLS 1.1 fallback) fixes most cert chain negotiation issues.
try {
    [Net.ServicePointManager]::SecurityProtocol = `
        [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls11
} catch {
    Write-Warning "Could not set TLS 1.2 - $($_.Exception.Message)"
}

$toolsRoot = 'C:\NeuroBoot\tools'
New-Item -ItemType Directory -Path $toolsRoot -Force | Out-Null

# Helper: download with retry, optional size sanity check.
# -AllowCertErrors bypasses TLS cert validation (use only for known-trusted
# sites whose cert chain is not in Windows trust store, e.g., self-signed
# or cert-issued-by-unrecognized-CA scenarios). Scoped to the call only.
function Get-WithRetry {
    param(
        [string]$Url,
        [string]$OutFile,
        [int]$MinBytes = 1000,
        [int]$Retries = 3,
        [switch]$AllowCertErrors
    )
    $oldCallback = [System.Net.ServicePointManager]::ServerCertificateValidationCallback
    try {
        if ($AllowCertErrors) {
            [System.Net.ServicePointManager]::ServerCertificateValidationCallback = { $true }
        }
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
    } finally {
        [System.Net.ServicePointManager]::ServerCertificateValidationCallback = $oldCallback
    }
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
    # The actual filename on cdslow.org.ru is "ntpwed07.zip" (~135 KB,
    # dated 2017-09-26), not "ntpwedit_0.7_x64.zip" as previously guessed.
    # HTTPS is broken on this host (cert CN mismatch -> SEC_E_WRONG_PRINCIPAL)
    # so we use HTTP. The zip is small + signed-by-its-source-of-record,
    # MitM risk is mitigated by us re-checking the contained NTPWEdit.exe
    # at runtime (or by user obtaining a different copy if paranoid).
    $url = 'http://cdslow.org.ru/files/ntpwedit/ntpwed07.zip'
    $okDl = Get-WithRetry -Url $url -OutFile $zip -MinBytes 100000 -Retries 3
    if ($okDl) {
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
        # zip extracts to testdisk-X.X/ subfolder containing testdisk_win.exe.
        # Rust tool expects $dest\testdisk_win.exe directly (no version subdir),
        # so we copy the CONTENTS of the testdisk-X.X/ dir, not the dir itself.
        $found = Get-ChildItem $extractTmp -Filter 'testdisk_win.exe' -Recurse | Select-Object -First 1
        if ($null -ne $found) {
            New-Item -ItemType Directory -Path $dest -Force | Out-Null
            Copy-Item "$($found.Directory.FullName)\*" -Destination $dest -Recurse -Force
            Write-Host "  OK: $dest\testdisk_win.exe (flattened from $($found.Directory.Name)\)"
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
    # builds.smartmontools.org now redirects to GitHub releases. Use the
    # GitHub API to enumerate the latest release's assets. NOTES:
    # - /releases/latest returns 404 because all releases are pre-release
    #   (rolling nightly), so we use /releases?per_page=1
    # - Current releases only ship the Windows .exe installer (NSIS), no
    #   standalone .zip with binaries. We extract the installer using
    #   7za.exe (from our own tools dir if available, else system 7z).
    $apiUrl = 'https://api.github.com/repos/smartmontools/smartmontools-builds/releases?per_page=1'
    $assetUrl = $null
    $assetName = $null
    try {
        $rel = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing -TimeoutSec 60 -ErrorAction Stop
        if ($rel -and $rel.Count -gt 0) {
            Write-Host "  found release: $($rel[0].tag_name)"
            # Pick the Windows installer (.exe). win32-setup naming covers both archs.
            $a = $rel[0].assets | Where-Object { $_.name -match 'smartmontools-win(32|64)-setup-.*\.exe$' } | Select-Object -First 1
            if ($null -ne $a) {
                $assetUrl = $a.browser_download_url
                $assetName = $a.name
                Write-Host "  picked asset: $assetName ($([math]::Round($a.size/1MB,2)) MB)"
            } else {
                Write-Warning "  no smartmontools-winNN-setup-*.exe found in release assets"
            }
        }
    } catch {
        Write-Warning "  GitHub API fetch failed: $($_.Exception.Message)"
    }

    if ($null -ne $assetUrl) {
        $installer = "$tmpDir\smartmontools-installer.exe"
        if (Get-WithRetry -Url $assetUrl -OutFile $installer -MinBytes 500000 -Retries 2) {
            # Locate 7z to extract NSIS installer without running it. Prefer
            # the FULL 7-Zip install over our standalone 7za.exe: 7za 23.01
            # does not recognize this NSIS-3 Unicode variant, but full 7z does.
            $sevenZip = $null
            foreach ($p in @(
                'C:\Program Files\7-Zip\7z.exe',
                'C:\Program Files (x86)\7-Zip\7z.exe',
                'C:\NeuroBoot\tools\7zip\7za.exe'
            )) {
                if (Test-Path $p) { $sevenZip = $p; break }
            }
            if ($null -ne $sevenZip) {
                $extractTmp = "$tmpDir\smartmontools-extracted"
                if (Test-Path $extractTmp) { Remove-Item $extractTmp -Recurse -Force }
                New-Item -ItemType Directory -Path $extractTmp -Force | Out-Null
                & $sevenZip x -y "-o$extractTmp" $installer | Out-Null
                if ($LASTEXITCODE -ne 0) {
                    Write-Warning "  $sevenZip extract failed (exit $LASTEXITCODE) - if you used 7za standalone, install full 7-Zip from https://www.7-zip.org/"
                }
                # NSIS installer has smartctl.exe in TWO subdirs: bin\ (x64) and bin32\ (x86).
                # We pick bin\ (x64) since modern target machines are predominantly x64,
                # and SAFETY: even x64 PE can run x86 binaries via WoW64 (PE includes it),
                # but x64 native is preferred for performance + matches our llama-server x64.
                $allFound = Get-ChildItem $extractTmp -Filter 'smartctl.exe' -Recurse
                $found = $allFound | Where-Object { $_.Directory.Name -eq 'bin' } | Select-Object -First 1
                if ($null -eq $found) {
                    $found = $allFound | Select-Object -First 1
                }
                if ($null -ne $found) {
                    New-Item -ItemType Directory -Path $dest -Force | Out-Null
                    # Copy whole containing dir so smartctl.exe finds its dependencies
                    Copy-Item "$($found.Directory.FullName)\*" -Destination $dest -Recurse -Force
                    $count = (Get-ChildItem $dest -File).Count
                    Write-Host "  OK: $dest\smartctl.exe (NSIS extract via $sevenZip, $count files from $($found.Directory.Name)\)"
                    $results['smartmontools'] = 'ok'
                } else {
                    Write-Warning "  smartctl.exe not found inside installer (NSIS extraction layout changed?)"
                    $results['smartmontools'] = 'failed-no-exe'
                }
            } else {
                Write-Warning "  no 7z.exe found to extract NSIS installer"
                Write-Warning "  install full 7-Zip from https://www.7-zip.org/ then re-run"
                $results['smartmontools'] = 'bootstrap-needed'
            }
        } else {
            Write-Warning "  download of $assetName failed"
            $results['smartmontools'] = 'failed-download'
        }
    } else {
        Write-Warning "  Manual fallback:"
        Write-Warning "    1. open https://github.com/smartmontools/smartmontools-builds/releases"
        Write-Warning "    2. download latest smartmontools-winNN-setup-*.exe"
        Write-Warning "    3. install (or extract with 7-Zip) -> copy bin\ contents to $dest\"
        $results['smartmontools'] = 'manual-required'
    }
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

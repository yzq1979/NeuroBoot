# build-release.ps1
# 编译 NeuroBoot release 版本，自动启用 crt-static（PE 兼容关键）。
#
# 用法：在 PowerShell 里跑
#     C:\NeuroBoot\tools-dev\build-release.ps1
#
# 设计：通过 RUSTFLAGS 环境变量传 +crt-static。我们试过用 .cargo/config.toml
# 的 [build] / [target.*] section 配置 rustflags，但实测两种 section 都没让
# cargo 把 flag 传到 rustc（怀疑 Rust 1.92 + Cargo 行为变化或 config 文件
# UTF-8 BOM 解析问题）。RUSTFLAGS env var 是最可靠的传递方式。

$env:RUSTFLAGS = '-C target-feature=+crt-static'

Write-Host "[NeuroBoot] cargo build --release with +crt-static"
Write-Host "  RUSTFLAGS = $env:RUSTFLAGS"
Write-Host ""

cargo build --release --manifest-path C:\NeuroBoot\app\Cargo.toml

if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo build failed (exit $LASTEXITCODE)"
    exit $LASTEXITCODE
}

$exe = 'C:\NeuroBoot\app\target\release\neuroboot.exe'
if (Test-Path $exe) {
    $f = Get-Item $exe
    Write-Host ""
    Write-Host "[OK] neuroboot.exe: $([math]::Round($f.Length/1MB,2)) MB at $exe"
}

# 01-collect-neuroboot-payload.ps1
# 阶段 6 PE 集成第一步：把 NeuroBoot.exe + Mesa 软件渲染 DLL 收集到
# pe-build\payload\neuroboot\，作为 PE 集成时拷进 PE 镜像的源目录。
#
# 前置条件：
# - C:\NeuroBoot\app\target\release\neuroboot.exe 存在（先跑 build-release.ps1）
# - C:\NeuroBoot\tools-dev\mesa-extract\x64\ 含 mesa DLL（先解压 mesa3d-*.7z）
#
# 输出：pe-build\payload\neuroboot\ 含 3 个文件
#   neuroboot.exe        (~11 MB, crt-static, 自包含 VC runtime)
#   opengl32.dll         (0.13 MB, Mesa wrapper)
#   libgallium_wgl.dll   (58.7 MB, Mesa llvmpipe 主驱动)
# 总 ~70 MB

$src_exe = 'C:\NeuroBoot\app\target\release\neuroboot.exe'
$src_mesa = 'C:\NeuroBoot\tools-dev\mesa-extract\x64'
$dest = 'C:\NeuroBoot\pe-build\payload\neuroboot'

if (-not (Test-Path $src_exe)) {
    Write-Error "neuroboot.exe not found at $src_exe - run build-release.ps1 first"
    exit 1
}
if (-not (Test-Path "$src_mesa\opengl32.dll") -or -not (Test-Path "$src_mesa\libgallium_wgl.dll")) {
    Write-Error "Mesa DLLs not found in $src_mesa - extract mesa3d-26.1.1-release-msvc.7z to tools-dev\mesa-extract\"
    exit 1
}

New-Item -ItemType Directory -Path $dest -Force | Out-Null
Get-ChildItem $dest -File -ErrorAction SilentlyContinue | Remove-Item -Force

Copy-Item $src_exe -Destination "$dest\neuroboot.exe" -Force
Copy-Item "$src_mesa\opengl32.dll" -Destination "$dest\opengl32.dll" -Force
Copy-Item "$src_mesa\libgallium_wgl.dll" -Destination "$dest\libgallium_wgl.dll" -Force

Write-Host "[OK] Payload assembled at $dest"
Get-ChildItem $dest -File | Sort-Object Length -Descending | ForEach-Object {
    '  {0,-30} {1,8:N2} MB' -f $_.Name, ($_.Length/1MB)
}
$total = (Get-ChildItem $dest -File | Measure-Object Length -Sum).Sum
Write-Host ""
Write-Host ("Total: {0:N2} MB" -f ($total/1MB))

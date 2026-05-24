# start-llama-vision-server.ps1
# v2 Stage 5: launch a second llama-server for the Qwen3-VL-2B vision model
# on port 8081 (separate from the main text Qwen3-4B on port 8080).
#
# Pure English (PS 5.1 GBK decode safety).
#
# Prerequisites:
# - Qwen3-VL-2B-Instruct GGUF + mmproj at C:\NeuroBoot\models\:
#     Qwen3-VL-2B-Instruct-Q4_K_M.gguf   (~1.1 GB)
#     mmproj-Qwen3-VL-2B-Instruct-Q8_0.gguf  (~400 MB)
# - llama.cpp build b6907+ at C:\NeuroBoot\tools-dev\llama-cpp\bN\
#   (Qwen3-VL support landed in PR #16780, merged 2025-10-30)
# - VC++ Redist DLLs in same dir as llama-server.exe (already handled by
#   pe-build/payload/crt-redist/, but for dev usage make sure they're
#   present beside the llama-cpp build).
#
# Usage:
#     C:\NeuroBoot\tools-dev\start-llama-vision-server.ps1
#
# Stop with Ctrl+C. To make this start automatically in PE, add a line
# to pe-build/build-scripts/04-add-payload.ps1's startnet.cmd (after
# the text llama-server start) - see Stage 5.x lazy-spawn TODO.

$ErrorActionPreference = 'Stop'

$modelDir = 'C:\NeuroBoot\models'
$modelGguf = Join-Path $modelDir 'Qwen3-VL-2B-Instruct-Q4_K_M.gguf'
$mmproj    = Join-Path $modelDir 'mmproj-Qwen3-VL-2B-Instruct-Q8_0.gguf'

# pick the first llama-cpp dir that has llama-server.exe and Qwen3-VL support
$llamaCpp = Get-ChildItem 'C:\NeuroBoot\tools-dev\llama-cpp' -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    Where-Object { Test-Path (Join-Path $_.FullName 'llama-server.exe') } |
    Select-Object -First 1

if ($null -eq $llamaCpp) {
    throw "no llama-cpp build found under C:\NeuroBoot\tools-dev\llama-cpp\. Need b6907+ for Qwen3-VL."
}
$llamaServer = Join-Path $llamaCpp.FullName 'llama-server.exe'

foreach ($f in @($modelGguf, $mmproj)) {
    if (-not (Test-Path $f)) {
        throw "missing model file: $f`nDownload from ModelScope: https://modelscope.cn/models/Qwen/Qwen3-VL-2B-Instruct-GGUF"
    }
}

Write-Host "[NeuroBoot vision] starting llama-server on http://127.0.0.1:8081"
Write-Host "  llama-server: $llamaServer"
Write-Host "  model:        $modelGguf"
Write-Host "  mmproj:       $mmproj"
Write-Host ""

& $llamaServer `
    -m $modelGguf `
    --mmproj $mmproj `
    -a qwen3-vl-2b `
    --host 127.0.0.1 `
    --port 8081 `
    -c 8192 `
    -ngl 0 `
    -t 4 `
    --no-mmap

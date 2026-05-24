# start-llama-server.ps1
# 启动本地 llama-server，加载 Qwen3-4B-Instruct GGUF，监听 127.0.0.1:8080。
# 这是开发期工具脚本；阶段 6 PE 集成时会写生产版（放 pe-build/winpe-config/）。
#
# 用法（在 PowerShell 里）：
#     C:\NeuroBoot\tools-dev\start-llama-server.ps1
# 中止：Ctrl+C；或另开窗口跑：Get-Process llama-server | Stop-Process

$server = 'C:\NeuroBoot\tools-dev\llama-cpp\b9294\llama-server.exe'
$model  = 'C:\NeuroBoot\models\Qwen3-4B-Instruct-2507-Q4_K_M.gguf'

if (-not (Test-Path $server)) {
    Write-Error "llama-server.exe 未找到：$server"
    exit 1
}
if (-not (Test-Path $model)) {
    Write-Error "模型 GGUF 未找到：$model"
    exit 1
}

Write-Host "[NeuroBoot] 启动 llama-server..."
Write-Host "  server : $server"
Write-Host "  model  : $model"
Write-Host "  listen : http://127.0.0.1:8080"
Write-Host "  ctx    : 16384"
Write-Host "  gpu    : 0 (CPU only build)"
Write-Host ""

# 参数说明：
#   -m   模型 GGUF 路径
#   -a   API 协议里 model 字段的别名（NeuroBoot 代码里 DEFAULT_MODEL = "qwen3-4b-instruct"）
#   -c   context size。Qwen3-4B 原生支持 256K；
#        16384 是「能跑诊断多轮 + 工具结果不爆」与「KV cache RAM」之间的平衡。
#        阶段 3 测试发现 4096 在 3-4 轮诊断后会撞顶；改 16K 后正常。
#        若仍不够（如同时处理超长事件日志），可继续加到 32768 或更大。
#   -ngl GPU layers (0 = 全 CPU，与 PE 行为一致)
#   -t   CPU threads (-1 = 自动取最优)
& $server `
    -m $model `
    -a 'qwen3-4b-instruct' `
    --host 127.0.0.1 `
    --port 8080 `
    -c 16384 `
    -ngl 0 `
    -t -1

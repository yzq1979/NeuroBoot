# NeuroBoot 完整构建攻略 — 从零到可启动 U 盘

> 假设你刚装了一台 Windows 11 机器，从零做出可启动的 NeuroBoot U 盘。本攻略包含**完整步骤**、**可复制的脚本**、以及**一键自动化命令**。

---

## 0. 前置硬件 / 系统要求

### 0.1 构建机（编译 ISO 用）

| 项 | 要求 |
|---|---|
| OS | Windows 10 / 11（开发机；中文 Windows 11 25H2 26200 实测 OK） |
| RAM | ≥ 16 GB（编译 + 跑模型 + PE ramdisk） |
| 磁盘空间 | ≥ 30 GB 空闲（ADK 5 GB + ISO 6 GB + 模型 2.5 GB + workspace 5 GB + Rust target/ 5 GB + 杂） |
| CPU | 任意 x64（编译期不重要） |
| U 盘 | ≥ 16 GB（NeuroBoot.iso 是 2.89 GB；Kingston 32 GB DT 实测） |
| 网络 | 需要联网下载 ADK、模型、Mesa、Ventoy 等（国内可能要走代理） |

### 0.2 目标机（启动 PE 用）—— ⚠ 真测后明确硬限制

| 项 | 必须 | 推荐 |
|---|---|---|
| RAM | ≥ 4 GB | ≥ 8 GB（4B 模型推理） |
| 启动方式 | UEFI 或 Legacy BIOS | UEFI（Ventoy 自动适配） |
| Secure Boot | 需关闭（启动 PE 时） | — |
| **鼠标 / 键盘** | **有线 USB 或 2.4G USB receiver 无线** | — |
| Wi-Fi | 网卡有 inbox 驱动则可用（Intel/Realtek 大概率 OK） | 有线网口或手机 USB 共享 |

**⚠ 鼠标 / 键盘必须避开蓝牙：**
- ✅ 有线 USB 鼠标 / 键盘
- ✅ 自带 USB 小接收器的 2.4G 无线鼠标（罗技 Unifying / 普通 dongle）
- ❌ **蓝牙鼠标 / 键盘不支持** —— Windows PE 不内置蓝牙 stack（ADK 设计硬限制），软件无法绕过
- ❌ 笔记本触控板：大多支持，少数新平台 Precision Touchpad 需要 OEM 驱动（PE 没装）

**⚠ 中文输入：** PE 不带 IME 框架（ADK 设计硬限制）。NeuroBoot UI 内提供 6 个常见问题快捷按钮，并支持读 U 盘根 `NeuroBoot.prompts.txt`（每行一个候选问题）。完整拼音 IME 在 v1.1 路线图上。

## 1. 环境准备（一次性，约 1 小时）

### 1.1 装 Rust 工具链

下载并安装 [rustup](https://rustup.rs/) → 默认会装 `x86_64-pc-windows-msvc` target。然后：

```powershell
rustc --version  # 应该看到 rustc 1.92.0+ 或更新
cargo --version
```

如果有 Visual Studio Community 2026（或 Build Tools 2022+）含 **C++ 工作负载**，Rust msvc target 能链接。否则 rustup 安装时会引导你装。

### 1.2 装 Windows ADK 10.1.26100.2454 + WinPE add-on

下载（约 5 MB online installer 各）：
- ADK setup：https://go.microsoft.com/fwlink/?linkid=2289980
- WinPE add-on：https://go.microsoft.com/fwlink/?linkid=2289981

**关键：在 admin PowerShell 跑静默安装（仅勾 Deployment Tools，省 ~3 GB 空间）**：

```powershell
& 'C:\NeuroBoot\tools-dev\adksetup.exe' /passive /norestart /features OptionId.DeploymentTools
# 等第一个进度条窗口关闭，再跑第二个：
& 'C:\NeuroBoot\tools-dev\adkwinpesetup.exe' /passive /norestart /features OptionId.WindowsPreinstallationEnvironment
```

验证安装：

```powershell
Test-Path 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools'
Test-Path 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Windows Preinstallation Environment'
# 两个都应该 True
```

> ⚠ **坑**：ADK setup 在某些场景下默认只下载到 `Downloads\Windows Kits\10\ADK`（layout 模式）不实际安装。如果验证 path 不存在，看 KNOWN-ISSUES.md「ADK setup layout mode」。

### 1.3 装 7-Zip（用于解压 Mesa 7z 压缩包）

```powershell
winget install --id 7zip.7zip --exact --accept-package-agreements --accept-source-agreements --silent
```

### 1.4 下载并准备 llama.cpp（CPU build）

```powershell
$out = 'C:\NeuroBoot\tools-dev\llama-b9294-bin-win-cpu-x64.zip'
$urls = @(
    'https://github.com/ggml-org/llama.cpp/releases/download/b9294/llama-b9294-bin-win-cpu-x64.zip',
    'https://mirror.ghproxy.com/https://github.com/ggml-org/llama.cpp/releases/download/b9294/llama-b9294-bin-win-cpu-x64.zip'
)
foreach ($url in $urls) {
    try { Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing -TimeoutSec 180 -ErrorAction Stop; break } catch { }
}
Expand-Archive $out -DestinationPath 'C:\NeuroBoot\tools-dev\llama-cpp\b9294' -Force
# Verify
Test-Path 'C:\NeuroBoot\tools-dev\llama-cpp\b9294\llama-server.exe'
```

如果你想升级到更新版本，把 `b9294` 换成最新 build 号（查 https://github.com/ggml-org/llama.cpp/releases）。

### 1.5 下载并准备 Qwen3-4B-Instruct-2507 Q4_K_M GGUF (~2.4 GB)

```powershell
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\NeuroBoot\models\Qwen3-4B-Instruct-2507-Q4_K_M.gguf'
New-Item -ItemType Directory -Path (Split-Path $out) -Force | Out-Null
$urls = @(
    'https://hf-mirror.com/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf',
    'https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf'
)
foreach ($url in $urls) {
    try { Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing -TimeoutSec 1800 -ErrorAction Stop; break } catch { }
}
# Verify (~2.4 GB, magic "GGUF")
$bytes = Get-Content $out -Encoding Byte -TotalCount 4
[System.Text.Encoding]::ASCII.GetString($bytes)  # 应该输出 "GGUF"
```

> ⚠ **坑**：GitHub raw 直链 / jsdelivr 在国内可能不稳，**hf-mirror 优先**。详见 KNOWN-ISSUES.md「国内下载策略」。

### 1.6 下载并解压 Mesa3D（mesa-dist-win 26.1.1 MSVC）

```powershell
$ProgressPreference = 'SilentlyContinue'
$out = 'C:\NeuroBoot\tools-dev\mesa3d-26.1.1-release-msvc.7z'
Invoke-WebRequest -Uri 'https://github.com/pal1000/mesa-dist-win/releases/download/26.1.1/mesa3d-26.1.1-release-msvc.7z' -OutFile $out -UseBasicParsing -TimeoutSec 300
& 'C:\Program Files\7-Zip\7z.exe' x $out '-oC:\NeuroBoot\tools-dev\mesa-extract' -y
# Verify
Test-Path 'C:\NeuroBoot\tools-dev\mesa-extract\x64\opengl32.dll'
Test-Path 'C:\NeuroBoot\tools-dev\mesa-extract\x64\libgallium_wgl.dll'
```

如果有更新版 Mesa，去 https://github.com/pal1000/mesa-dist-win/releases 找最新 `mesa3d-X.X.X-release-msvc.7z` 替换版本号。

### 1.7 关闭 Smart App Control（Windows 11 25H2 默认开启会阻止未签名 NeuroBoot.exe）

**⚠ 这是不可逆操作**（实测在 25H2 上没弹"不可逆"警告也不需重启，但官方说关后想再开必须重置 Windows）。

设置 → 隐私和安全性 → Windows 安全中心 → 应用和浏览器控制 → Smart App Control 设置 → 选「关」。立即生效。

详见 KNOWN-ISSUES.md「Smart App Control」。

---

## 2. 编译 NeuroBoot Rust 应用（10~30 分钟首次）

阶段 1~4 的代码在 `app/src/` 已经写好。直接编译：

```powershell
C:\NeuroBoot\tools-dev\build-release.ps1
```

或者手动：

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --manifest-path C:\NeuroBoot\app\Cargo.toml
```

> ⚠ **坑**：必须用 `RUSTFLAGS` 环境变量传 `+crt-static`。`.cargo/config.toml` 的 `[build] rustflags` / `[target.*] rustflags` 在 Rust 1.92 / Cargo 1.92 **实测不生效**（见 KNOWN-ISSUES.md）。

**验证产物**：

```powershell
$exe = 'C:\NeuroBoot\app\target\release\neuroboot.exe'
Get-Item $exe | Select-Object Length  # ~11.4 MB
# 用 dumpbin 验证依赖：不应有 VCRUNTIME140 / api-ms-win-crt-*
$dumpbin = Get-ChildItem 'C:\Program Files\Microsoft Visual Studio' -Recurse -Filter 'dumpbin.exe' | Where-Object { $_.FullName -like '*Hostx64\x64*' } | Select-Object -First 1
& $dumpbin.FullName /dependents $exe | Select-String 'VCRUNTIME|api-ms-win-crt'
# 应该返回空（如果还有 VCRUNTIME140 说明 crt-static 没生效）
```

---

## 3. 一键 PE ISO 构建（5~20 分钟）

**在 admin PowerShell 跑**：

```powershell
PowerShell -NoProfile -ExecutionPolicy Bypass -File C:\NeuroBoot\pe-build\build-scripts\99-build-all.ps1
```

这个一键脚本会依次跑：

1. **Phase 0** —— `cargo build --release` with crt-static（如果已是 latest，几秒；首次 10~30 分钟）
2. **Phase 1** —— `01-collect-neuroboot-payload.ps1`：拷 NeuroBoot.exe + Mesa DLLs 到 `pe-build/payload/neuroboot/`
3. **Phase 2** —— `02-run-copype.cmd`：删旧 workspace + ADK copype amd64 初始化新 workspace
4. **Phase 3** —— `03-mount-and-customize.ps1`：DISM mount boot.wim + 加 5 个 WinPE OCs（WMI/NetFx/Scripting/PowerShell/StorageWMI）+ en-US 语言包
5. **Phase 4** —— `04-add-payload.ps1`：拷 70 MB NeuroBoot + 50 MB llama.cpp + 2.4 GB GGUF 到 mount/NeuroBoot/，写 start-llama-server.cmd + 覆盖 startnet.cmd
6. **Phase 5** —— `05-unmount-and-makemedia.ps1`：DISM unmount /Commit + MakeWinPEMedia /ISO

产物：`C:\NeuroBoot\pe-build\output\NeuroBoot.iso`（约 2.89 GB）

每个 phase 的 log 在 `pe-build/workspace/stageNN.log`（最后一次 run 的）。

---

## 4. Ventoy 写 U 盘（5 分钟）

### 4.1 下载 Ventoy（如未下）

```powershell
$rel = Invoke-RestMethod 'https://api.github.com/repos/ventoy/Ventoy/releases/latest' -UseBasicParsing
$asset = $rel.assets | Where-Object { $_.name -match '^ventoy-.*-windows\.zip$' } | Select-Object -First 1
$out = "C:\NeuroBoot\tools-dev\$($asset.name)"
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $out -UseBasicParsing -TimeoutSec 180
Expand-Archive -Path $out -DestinationPath 'C:\NeuroBoot\tools-dev\ventoy' -Force
```

### 4.2 半自动化：用 `setup-new-usb.ps1`

**推荐**：用项目自带的半自动化脚本，自动检测 USB + 区分"已是 Ventoy"和"全新 U 盘"两种情况：

```powershell
PowerShell -NoProfile -ExecutionPolicy Bypass -File C:\NeuroBoot\tools-dev\setup-new-usb.ps1
```

脚本流程：
1. 检测当前所有 USB removable 盘 + 列出
2. 你选盘号（防止误选其它盘）
3. 如果 USB 已经是 Ventoy（label = `Ventoy`），跳过 install **直接拷 ISO**
4. 如果是新 U 盘，要求你输入 `WIPE <盘符>` 字面字符串才继续（防误格式化），然后**打开 Ventoy GUI** 让你确认 Install
5. Ventoy GUI 关闭后，自动重新探测 Ventoy 数据分区，拷贝 NeuroBoot.iso

「下次插一个新 U 盘要做同样的事」时，跑这一条命令即可，比手动来回切窗口快。

### 4.3 手动方式（如果不想用脚本）

1. 插 U 盘（≥ 16 GB），**确保 U 盘上没有重要数据**（写盘会格式化整个盘）
2. 右键 `C:\NeuroBoot\tools-dev\ventoy\ventoy-X.X.X\Ventoy2Disk.exe` → 「以管理员身份运行」
3. **Device** 选你的 U 盘（小心选错盘符 → 误格式化其它盘）
4. 点 **Install** → 弹两次"清空 U 盘"确认 → 等 ~10 秒
5. U 盘变成两个分区：Ventoy 引导（隐藏）+ exFAT 数据分区（标签 `Ventoy`，可见盘符）
6. 拷 ISO：

```powershell
Copy-Item 'C:\NeuroBoot\pe-build\output\NeuroBoot.iso' -Destination '<U>:\NeuroBoot.iso'
```

2.89 GB 写 USB 3.0 约 1~3 分钟。

---

## 5. BIOS 启动 + PE 真测

### 5.1 BIOS 设置

重启时反复按 **F2 / Fn+F2 / Del / Esc**（看主板厂商）进 BIOS Setup。

**两个关键调整**：

1. **临时关 Secure Boot**（位置：Security 选项卡）
   > NeuroBoot.exe + Mesa DLL 未签名，Secure Boot 会拒绝启动
2. **确认 USB Boot 启用**（一般默认开）

F10 保存退出。

### 5.2 从 U 盘启动

重启时反复按 **F12 / Esc / F8 / F11**（看主板厂商 boot menu hotkey）→ 选 USB HDD / 你的 U 盘 → 回车

### 5.3 Ventoy → NeuroBoot.iso → PE

1. Ventoy 菜单出来，方向键选 `NeuroBoot.iso` → 回车
2. 第二个菜单选 **「Boot in normal mode」** → 回车
3. PE 加载（屏幕黑屏 + 转圈 ~30 秒）
4. `wpeinit` 自动跑（命令行可能闪一下）
5. 提示 `Waiting 60 seconds for llama-server to load Qwen3-4B model...`
6. 60 秒后 **NeuroBoot 窗口弹出**（标题「NeuroBoot 神启」）

### 5.4 PE 内验证

如果 NeuroBoot 窗口弹出 + 中文正常显示，试这些 prompt：

| Prompt | 应该看到 |
|---|---|
| `你好，介绍一下你自己` | LLM 中文回答（无工具调用） |
| `我的硬盘有几块？` | 调 `list_disks` + 中文总结 |
| `我的电脑配置？` | 调 `read_system_info` + 中文总结 |
| `最近 7 天系统错误` | 调 `read_event_log_errors({"hours":168})` |
| `删除 X:\test.txt` | 弹「确认执行危险工具」窗口 |

### 5.5 测试完关机回 Windows

PE 桌面 cmd 里跑 `wpeutil shutdown` 关机，拔 U 盘 → 重启回正常 Windows → BIOS 重新启用 Secure Boot（保护日常使用）。

---

## 6. 维护与更新

### 模型升级（Qwen 出新版）

```powershell
# 下新模型到 models/，跑 99-build-all.ps1 重新生成 ISO
# 注意：start-llama-server.cmd 里的模型文件名写死，需要同步更新
# 见 pe-build/build-scripts/04-add-payload.ps1 里 here-string $llamaCmd
```

### llama.cpp 升级（新 build 出来）

```powershell
# 删旧 tools-dev/llama-cpp/，下新版本，跑 99-build-all.ps1
```

### NeuroBoot 代码改动

```powershell
# 改 app/src/ 后跑 99-build-all.ps1，cargo build incremental 几秒
```

### Mesa 升级

```powershell
# 下新版 mesa3d-X.X.X-release-msvc.7z 到 tools-dev/，重新解压到 mesa-extract/
# 跑 99-build-all.ps1
```

### 换新 U 盘

如果换一个新 U 盘：
1. 跑 Ventoy2Disk.exe 装 Ventoy 到新 U 盘（**会格式化新 U 盘**，确认数据已备份）
2. 拷 `pe-build/output/NeuroBoot.iso` 到新 U 盘根目录
3. 直接启动测试（如果 ISO 没变，可重用现有 ISO，不用重 build）

---

## 7. 项目目录速查

```
C:\NeuroBoot\
├── README.md                       项目入口
├── .gitignore                      忽略 target/ models/ workspace/ 等大目录
├── app/                            Rust 应用源码
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 入口 + NeuroBootApp 状态机
│       ├── agent/                  Agent tool-use 循环
│       │   ├── mod.rs              AgentJob / AgentEvent / run_agent_loop
│       │   └── truncate.rs         token 估算 + 整 turn truncation
│       ├── llm/                    OpenAI 兼容客户端
│       │   ├── mod.rs
│       │   ├── client.rs           blocking_chat_completion
│       │   ├── endpoint.rs         A/C 双备路由探测
│       │   └── openai.rs           ChatCompletionRequest/Response, ToolDefinition, ToolCall 等
│       ├── tools/                  Agent 可调工具
│       │   ├── mod.rs
│       │   ├── registry.rs         Tool trait + SafetyClass + ToolRegistry
│       │   ├── safe/               只读诊断（自动执行）
│       │   │   ├── list_disks.rs
│       │   │   ├── read_system_info.rs
│       │   │   └── read_event_log_errors.rs
│       │   └── dangerous/          破坏性工具（必须确认）
│       │       └── delete_path.rs
│       └── ui/                     egui 渲染
│           ├── mod.rs
│           ├── chat.rs             ChatMessage / Role / render_message
│           └── fonts.rs            install_chinese_fonts (Noto Sans SC 嵌入)
├── docs/                           本文档目录
│   ├── BUILD.md                    （本文件）
│   └── KNOWN-ISSUES.md             所有踩坑 + 工作绕过
├── pe-build/
│   ├── build-scripts/              全部构建脚本
│   │   ├── 01-collect-neuroboot-payload.ps1
│   │   ├── 02-run-copype.cmd
│   │   ├── 03-mount-and-customize.ps1
│   │   ├── 04-add-payload.ps1
│   │   ├── 05-unmount-and-makemedia.ps1
│   │   └── 99-build-all.ps1                  一键串起所有 phase
│   ├── payload/
│   │   └── neuroboot/              ← 阶段 5 产物（NeuroBoot.exe + Mesa DLLs）
│   ├── source-iso/                 Win11 ISO 备用（实际 build 不需要）
│   ├── workspace/                  ← ADK copype 输出（含 mount/, media/）
│   ├── output/
│   │   └── NeuroBoot.iso           ← 最终产物
│   └── winpe-config/               （未用，给 ADK 替代方案预留）
├── models/                         Qwen GGUF（git ignored，2.4 GB）
└── tools-dev/                      开发期辅助工具（git ignored）
    ├── adksetup.exe                ADK online installer
    ├── adkwinpesetup.exe           WinPE add-on online installer
    ├── install-adk.ps1             ADK 静默安装脚本
    ├── build-release.ps1           cargo build with crt-static
    ├── start-llama-server.ps1      本地开发期启动 llama-server
    ├── llama-cpp/b9294/            llama.cpp CPU build 解压
    ├── mesa-extract/x64/           Mesa 解压（含 opengl32.dll + libgallium_wgl.dll）
    ├── ventoy/ventoy-X.X.X/        Ventoy GUI 工具
    └── mesa3d-26.1.1-release-msvc.7z
```

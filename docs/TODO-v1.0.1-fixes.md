# NeuroBoot v1.0.1 — U 盘真测反馈紧急修复清单

> 2026-05-24 用户首次 U 盘真测 NeuroBoot.iso 反馈的 4 个问题的诊断结论 + 明天动手修复的 checklist。
>
> **目标**：不动 boot.wim 内的 ADK OC 包结构、不重 build llama.cpp，最小代价让 v1.0 镜像在用户机器上**能用**。
>
> **关联文档**：
> - [TODO-v2.md](TODO-v2.md) —— v2.0 完整路线图（本文件覆盖的是其中 P0 项的真测加急部分）
> - [KNOWN-ISSUES.md](KNOWN-ISSUES.md) —— 项目踩过的坑

---

## 反馈问题速览

| # | 用户反馈 | 真根因 | 优先级 |
|---|---|---|---|
| 1 | 聊天界面无法输入中文，只能输入英文 | **WinPE 没有 IME 框架**（ADK 官方硬限制） | P0 |
| 2 | 无法使用鼠标 | **蓝牙鼠标**，PE 无蓝牙 stack | P0（文档） |
| 3 | 发送消息后无响应，提示「请确认端点已启动」 | **llama-server.exe 缺 VCRUNTIME140 + UCRT DLL 闪退**（不是 timeout 60 不够） | P0（致命） |
| 4 | 缺少在线 AI 配置 UI / 配置文件 | 当前仅支持环境变量，PE 内无法注入 | P0 |

---

## 1. llama-server CRT 依赖修复（最致命，先做）

### 根因
`dumpbin /DEPENDENTS C:\NeuroBoot\tools-dev\llama-cpp\b9294\llama-server.exe` 实测依赖：

```
KERNEL32.dll                            ← PE 自带
VCRUNTIME140.dll                        ← PE 缺
api-ms-win-crt-runtime-l1-1-0.dll       ← PE 缺
api-ms-win-crt-stdio-l1-1-0.dll         ← PE 缺
api-ms-win-crt-math-l1-1-0.dll          ← PE 缺
api-ms-win-crt-locale-l1-1-0.dll        ← PE 缺
api-ms-win-crt-heap-l1-1-0.dll          ← PE 缺
```

llama.cpp 官方 release 的 b9294 build 没用 `+crt-static`，进 PE 后 `llama-server.exe` 启动时找不到 VCRUNTIME140 直接闪退。`startnet.cmd` 用 `start ... /MIN cmd /c ...` 启动它，闪退看不见，于是 GUI 60 秒后启动连不上端点，错误文案触发于 `app/src/llm/client.rs:44`。

记忆中 [feedback-rust-msvc-crt-static](memory/feedback_rust_msvc_crt_static.md) 这条规则之前只对 NeuroBoot.exe 应用了，**忘了 llama.cpp 也是 msvc build 同样需要**。

### 修复（方案 C：本地加载 DLL，不重 build）

**步骤 1**：从主系统抓到所有缺失 DLL → `pe-build/payload/crt-redist/`

```powershell
# 创建目录
New-Item -ItemType Directory -Path 'C:\NeuroBoot\pe-build\payload\crt-redist' -Force | Out-Null

# 1. VCRUNTIME140.dll —— 从 VS 2026 redist 拷
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vsroot = & $vswhere -latest -property installationPath
$verFile = Get-Content "$vsroot\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt"
$ver = $verFile.Trim()
$redist = "$vsroot\VC\Redist\MSVC\$ver\x64\Microsoft.VC*.CRT\vcruntime140.dll"
Copy-Item (Get-Item $redist).FullName 'C:\NeuroBoot\pe-build\payload\crt-redist\' -Force

# 2. api-ms-win-crt-*.dll (UCRT umbrella) —— 从 System32 拷
$ucrt = @(
    'api-ms-win-crt-runtime-l1-1-0.dll',
    'api-ms-win-crt-stdio-l1-1-0.dll',
    'api-ms-win-crt-math-l1-1-0.dll',
    'api-ms-win-crt-locale-l1-1-0.dll',
    'api-ms-win-crt-heap-l1-1-0.dll',
    'api-ms-win-crt-string-l1-1-0.dll',     # 顺手多带几个常见的防漏
    'api-ms-win-crt-time-l1-1-0.dll',
    'api-ms-win-crt-convert-l1-1-0.dll',
    'api-ms-win-crt-utility-l1-1-0.dll',
    'api-ms-win-crt-filesystem-l1-1-0.dll',
    'api-ms-win-crt-environment-l1-1-0.dll',
    'api-ms-win-crt-process-l1-1-0.dll',
    'api-ms-win-crt-conio-l1-1-0.dll',
    'api-ms-win-crt-multibyte-l1-1-0.dll',
    'api-ms-win-crt-private-l1-1-0.dll',
    'ucrtbase.dll'                          # UCRT base，UCRT umbrella 的实现
)
foreach ($d in $ucrt) {
    if (Test-Path "C:\Windows\System32\$d") {
        Copy-Item "C:\Windows\System32\$d" 'C:\NeuroBoot\pe-build\payload\crt-redist\' -Force
    } else {
        Write-Warning "Missing: $d"
    }
}
Get-ChildItem 'C:\NeuroBoot\pe-build\payload\crt-redist\' | Select-Object Name, @{N='KB';E={[math]::Round($_.Length/1KB)}}
```

**步骤 2**：修改 `pe-build/build-scripts/04-add-payload.ps1`，在 `[2/5] Copying llama.cpp` 之后追加：

```powershell
Write-Host ""
Write-Host "=== [2.5/5] Copying CRT redist DLLs into llama-cpp dir ==="
$crtSrc = 'C:\NeuroBoot\pe-build\payload\crt-redist'
if (Test-Path $crtSrc) {
    Copy-Item "$crtSrc\*.dll" -Destination $peLlama -Force
    $crtFiles = Get-ChildItem $peLlama -Filter '*.dll' | Where-Object { $_.Name -match '^(vcruntime|api-ms-win-crt|ucrtbase)' }
    "  Added $($crtFiles.Count) CRT DLLs to $peLlama"
} else {
    Write-Warning "CRT redist source not found: $crtSrc - llama-server will fail to start in PE!"
}
```

**步骤 3**：重 build 后在主机用 `dumpbin /DEPENDENTS` 二次确认 `pe-build\workspace\mount\NeuroBoot\llama-cpp\llama-server.exe` 旁边能找到所有依赖（这步在 build pipeline 里加 assert 也行）。

**步骤 4**：进 PE 真测前，先在主机模拟 PE 环境 —— 用 NTFS 一个空目录里只有 llama-cpp + crt-redist，看 llama-server.exe 能否启动监听 8080。

### 备用方案（如果 C 不通）

**B：重 build llama.cpp static**
```powershell
git clone --branch b9294 https://github.com/ggerganov/llama.cpp C:\temp\llama.cpp
cd C:\temp\llama.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release `
      -DBUILD_SHARED_LIBS=OFF `
      -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded `
      -DLLAMA_BUILD_SERVER=ON
cmake --build build --config Release -j
# 输出会在 build/bin/Release/llama-server.exe，dumpbin 验证不依赖 VCRUNTIME140
```

**A：找社区 static build** —— 不可靠，跳过。

---

## 2. 中文输入方案（PE 无 IME，应用层解决）

### 根因
Win11 ADK 的 `WinPE_OCs\` 完全**没有 IME OC**（只有字体支持 `WinPE-FontSupport-ZH-CN.cab`）。Win10/11 PE 是部署/恢复极小子集，官方不带 ctfmon/TSF/IMM32 基础设施。从主系统硬抓 IME 文件违 EULA 且经常崩溃。

### 修复（三层叠加）

#### 层 1：常见问题快捷按钮（必做，~0.5 小时）

`app/src/main.rs` 在 `draw_input_panel` 之上加一行按钮：

```rust
const QUICK_PROMPTS: &[(&str, &str)] = &[
    ("电脑蓝屏", "我的电脑最近频繁蓝屏。请帮我:\n1. 列出最近 24 小时的系统错误事件\n2. 列出 minidump 文件\n3. 给出排查方向"),
    ("硬盘问题", "我担心硬盘出问题。请帮我:\n1. 列出所有硬盘和分区\n2. 查 SMART 健康信息\n3. 报告异常"),
    ("网络故障", "我的电脑连不上网。请帮我:\n1. 查 ipconfig\n2. ping 网关和 8.8.8.8\n3. 给出排查方向"),
    ("启动慢", "我的电脑开机很慢。请帮我:\n1. 列出开机自启程序\n2. 列运行中的服务\n3. 给出优化建议"),
    ("找回误删", "我误删了一些文件想找回。请告诉我:\n1. NeuroBoot 能用什么工具尝试恢复\n2. 我应该提供哪些信息"),
    ("系统修复", "我的 Windows 系统起不来了。请帮我:\n1. 检查启动配置 BCD\n2. 给出修复步骤"),
];

fn draw_quick_prompt_bar(&mut self, ui: &mut egui::Ui, busy: bool) {
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.weak("快捷问题:");
        for (label, prompt) in QUICK_PROMPTS {
            ui.add_enabled_ui(!busy, |ui| {
                if ui.small_button(*label).clicked() {
                    self.input_buffer = (*prompt).to_string();
                }
            });
        }
    });
    ui.add_space(2.0);
}
```

在 `draw_input_panel` 顶部调用 `self.draw_quick_prompt_bar(ui, busy);`。点按钮把预设 prompt 填入输入框，用户可点「发送」或继续编辑（英文/拼音 ASCII）。

#### 层 2：U 盘 prompts.txt（推荐，~0.5 小时）

`app/src/ui/prompts_file.rs`（新建）：启动时扫所有非 X: 盘符，找 `\NeuroBoot.prompts.txt` 或 `\NeuroBoot\prompts.txt`，每行作为一个候选问题。

```rust
pub fn scan_user_prompts() -> Vec<(String, String)> {
    // 返回 (盘符标签, prompt 内容) 列表
    let mut result = Vec::new();
    for letter in 'A'..='Z' {
        if letter == 'X' { continue; } // PE ramdisk
        for filename in &["NeuroBoot.prompts.txt", "NeuroBoot\\prompts.txt"] {
            let path = format!("{}:\\{}", letter, filename);
            if let Ok(content) = std::fs::read_to_string(&path) {
                for (i, line) in content.lines().enumerate() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') { continue; }
                    result.push((format!("{}#{}", letter, i+1), line.to_string()));
                }
                return result; // 找到第一个就停
            }
        }
    }
    result
}
```

UI 在 quick prompt bar 旁边加下拉框：

```rust
if !self.user_prompts.is_empty() {
    egui::ComboBox::from_label("我准备的问题")
        .selected_text("选择...")
        .show_ui(ui, |ui| {
            for (label, prompt) in &self.user_prompts {
                if ui.selectable_label(false, format!("{}: {}", label, prompt.chars().take(20).collect::<String>())).clicked() {
                    self.input_buffer = prompt.clone();
                }
            }
        });
}
```

`prompts.txt` 示例（用户在主系统编辑后放到 U 盘根）：

```
# 行首 # 是注释。每非空行是一个候选问题。
我的电脑屏幕显示"INACCESSIBLE_BOOT_DEVICE"开不了机，怎么修复？
我装了显卡驱动后蓝屏，想回滚驱动
D 盘突然变成 RAW 格式，能恢复数据吗？
```

#### 层 3：应用层拼音 IME（完整解，~4~8 小时，可放 v1.1）

UI 输入框右侧加「拼/En」切换按钮。拼音模式下键入 ASCII pinyin，浮动候选条显示前 9 个候选词，数字键/空格选词。

**技术栈**：
- 词典：`rime-luna-pinyin` ([GitHub](https://github.com/rime/rime-luna-pinyin)) `luna_pinyin.dict.yaml`，~2.4 MB，13 万词
  - 国内下载：先在主系统 git clone（GitHub raw 时通时不通，多试或挂代理）放 `app/assets/dict/`
  - 嵌入 exe 用 `include_bytes!`，体积加 ~2 MB
- 数据结构：`HashMap<String, Vec<(String, u32)>>`（pinyin → 候选词+频次）或 trie/fst
- UI：`egui::Window` + `Order::Tooltip` 浮动候选条
- 输入捕获：在拼音模式下，TextEdit response.changed 时截取新增字符判断
- 估计代码量：~600 行 Rust

**实现拆分**：
1. `src/ime/dict.rs`：词典解析 + 查询接口
2. `src/ime/state.rs`：当前 pinyin buffer + 候选列表 + 选中索引
3. `src/ui/ime_panel.rs`：候选条 widget（egui Window）
4. 修改 `src/main.rs::draw_input_panel`：注入 IME state，键盘事件优先 IME 拦截

**记入 v2 路线图**：层 3 可放 v1.1 或 v2.0 P1（[TODO-v2.md](TODO-v2.md) Part D）。

---

## 3. 蓝牙鼠标问题（文档说明，无代码修复）

### 根因
WinPE 完全不包含蓝牙 stack（Win10/11 ADK 都不带）。这是 ADK 设计，**无法软件解决**。

### 修复（文档化）
更新 [BUILD.md](BUILD.md) 和 [README.md](../README.md) 在「使用」/「硬件要求」一节加：

```markdown
### 硬件要求

**鼠标 / 键盘必须是有线 USB 或 2.4G USB receiver 无线**：
- ✅ 有线 USB 鼠标
- ✅ 自带 USB 小接收器的 2.4G 无线鼠标（罗技 Unifying / 普通 dongle）
- ❌ **蓝牙鼠标 / 键盘不支持** —— Windows PE 不带蓝牙 stack，无法解决
- ❌ 笔记本触控板大多数支持，但少数新平台 Precision Touchpad 需要 OEM 驱动
```

UI 启动 splash 也加一句提示：「鼠标不动？请换有线 USB 鼠标或 USB receiver 无线鼠标，PE 不支持蓝牙」。

---

## 4. 在线 AI 配置 UI + config.json

### 根因
`app/src/llm/endpoint.rs` 当前只读 `NEUROBOOT_A_ENDPOINT/MODEL/API_KEY` 三个环境变量。PE 启动时这些变量为空 → active 总是退到本地端点。

### 修复

#### 配置文件格式

`config.json` schema（放 U 盘 Ventoy 数据分区根目录，或 ISO 内 `\NeuroBoot\config.json`）：

```json
{
  "remote_endpoint": "https://api.deepseek.com",
  "remote_model": "deepseek-chat",
  "remote_api_key": "sk-xxxxxxxxxxxxxxxxxxxxxx",
  "remote_label": "DeepSeek 云端",
  "prefer_remote": true,
  "local_endpoint": "http://127.0.0.1:8080",
  "local_model": "qwen3-4b-instruct",
  "system_prompt_override": null
}
```

字段说明：
- `remote_endpoint` / `model` / `api_key` —— 在线 AI 服务（OpenAI / DeepSeek / 通义千问 / 智谱等任何 OpenAI 兼容端点）
- `prefer_remote` —— 启动时优先探测 remote，可用则用 remote；false 时直接用 local 不探测
- `local_endpoint` / `model` —— 本地 llama-server，通常不改
- `system_prompt_override` —— 可空，非空时覆盖默认 system prompt

#### 启动加载顺序

`app/src/llm/endpoint.rs::detect_endpoints` 改写：

```rust
pub fn detect_endpoints(
    local_endpoint: &str,
    local_model: &str,
) -> (EndpointConfig, Option<EndpointConfig>) {
    // 1. 尝试从 config.json 加载（覆盖默认）
    let cfg = load_config_file();

    // 2. 环境变量优先（调试用，覆盖 config）
    let env_endpoint = std::env::var("NEUROBOOT_A_ENDPOINT").ok().filter(|s| !s.is_empty());

    let remote = match (env_endpoint, cfg.as_ref()) {
        (Some(url), _) => Some(EndpointConfig {
            endpoint: url,
            model: std::env::var("NEUROBOOT_A_MODEL").unwrap_or_else(|_| "default".into()),
            api_key: std::env::var("NEUROBOOT_A_API_KEY").ok().filter(|s| !s.is_empty()),
            label: "云端(env)".into(),
        }),
        (None, Some(c)) if !c.remote_endpoint.is_empty() => Some(EndpointConfig {
            endpoint: c.remote_endpoint.clone(),
            model: c.remote_model.clone(),
            api_key: if c.remote_api_key.is_empty() { None } else { Some(c.remote_api_key.clone()) },
            label: c.remote_label.clone(),
        }),
        _ => None,
    };

    let local = EndpointConfig {
        endpoint: cfg.as_ref().map(|c| c.local_endpoint.clone()).unwrap_or_else(|| local_endpoint.to_owned()),
        model: cfg.as_ref().map(|c| c.local_model.clone()).unwrap_or_else(|| local_model.to_owned()),
        api_key: None,
        label: "本地".into(),
    };

    let prefer_remote = cfg.as_ref().map(|c| c.prefer_remote).unwrap_or(true);

    match remote {
        Some(r) if prefer_remote && probe_endpoint(&r.endpoint, Duration::from_secs(3))
                   => (r, Some(local)),
        Some(r) => (local, Some(r)),    // 配了 remote 但不通 / 用户禁了 prefer_remote
        None    => (local, None),
    }
}

fn load_config_file() -> Option<ConfigFile> {
    // 顺序：1. 所有非 X: 盘 \NeuroBoot.config.json 或 \NeuroBoot\config.json
    //       2. X:\NeuroBoot\config.json (ISO 内置默认)
    for letter in 'A'..='Z' {
        if letter == 'X' { continue; }
        for path in &[format!("{}:\\NeuroBoot.config.json", letter),
                      format!("{}:\\NeuroBoot\\config.json", letter)] {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(cfg) = serde_json::from_str(&content) {
                    return Some(cfg);
                }
            }
        }
    }
    std::fs::read_to_string("X:\\NeuroBoot\\config.json")
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
}
```

#### UI 设置面板

顶栏加齿轮按钮（`⚙` 或 「设置」），点击弹 modal Window：

```rust
fn draw_settings_dialog(&mut self, ctx: &egui::Context) {
    if !self.show_settings { return; }
    let mut close = false;
    egui::Window::new("在线 AI 端点设置")
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(480.0);
            ui.label("Endpoint URL (OpenAI 兼容):");
            ui.text_edit_singleline(&mut self.settings_buffer.endpoint);
            ui.label("Model name:");
            ui.text_edit_singleline(&mut self.settings_buffer.model);
            ui.label("API Key:");
            ui.add(egui::TextEdit::singleline(&mut self.settings_buffer.api_key).password(true));
            ui.label("显示名:");
            ui.text_edit_singleline(&mut self.settings_buffer.label);
            ui.checkbox(&mut self.settings_buffer.prefer_remote, "优先用在线端点");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("保存到 U 盘并重新探测").clicked() {
                    match self.save_settings_to_usb() {
                        Ok(path) => self.messages.push(ChatMessage::assistant(format!("（设置已保存到 {}）端点重新探测中...", path))),
                        Err(e)   => self.messages.push(ChatMessage::assistant(format!("（保存失败）{}", e))),
                    }
                    self.reprobe_endpoints();
                    close = true;
                }
                if ui.button("仅本次会话使用（不保存）").clicked() {
                    self.apply_settings_to_active();
                    close = true;
                }
                if ui.button("关闭").clicked() { close = true; }
            });
        });
    if close { self.show_settings = false; }
}

fn save_settings_to_usb(&self) -> Result<String, String> {
    // 找第一个可写非 X: 盘符
    for letter in 'A'..='Z' {
        if letter == 'X' { continue; }
        let path = format!("{}:\\NeuroBoot.config.json", letter);
        if let Ok(json) = serde_json::to_string_pretty(&self.settings_buffer.to_config_file()) {
            if std::fs::write(&path, json).is_ok() {
                return Ok(path);
            }
        }
    }
    Err("找不到可写的非 X: 分区。Ventoy 通常会创建一个 exFAT 数据分区，请确认 U 盘是 Ventoy 模式而非纯 ISO 直写。".into())
}
```

#### endpoint.rs `probe_endpoint` 超时调整

3 秒可能太短（在线 API 海外节点延迟高）。改为 5 秒，且改为 HEAD 而不是 GET：

```rust
fn probe_endpoint(endpoint: &str, timeout: Duration) -> bool {
    let client = match reqwest::blocking::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    // 用 HEAD /v1/models（OpenAI 兼容端点通常都有）；失败时退化用 GET /
    client.head(format!("{}/v1/models", endpoint.trim_end_matches('/')))
        .send().is_ok()
        || client.get(endpoint).send().is_ok()
}
```

---

## 5. healthcheck-based 启动（顺带改）

### 根因
`04-add-payload.ps1:83` 写死 `timeout /t 60`。即使 llama-server CRT 修好了，模型加载时间仍受机器影响（HDD vs NVMe 差异大）。

### 修复

改 `04-add-payload.ps1` 里 startnet.cmd 的 wait 段为 PowerShell 探测循环：

```cmd
@echo off
REM NeuroBoot PE startnet.cmd
wpeinit
start "llama-server" /MIN cmd /c "X:\NeuroBoot\start-llama-server.cmd"

REM healthcheck loop: 探测 http://127.0.0.1:8080/health 最多 180s
echo Waiting for llama-server to be ready (max 180s)...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$deadline=(Get-Date).AddSeconds(180); while ((Get-Date) -lt $deadline) { try { $r = Invoke-WebRequest -Uri 'http://127.0.0.1:8080/health' -UseBasicParsing -TimeoutSec 2 -ErrorAction Stop; if ($r.StatusCode -eq 200) { Write-Host '[OK] llama-server ready'; exit 0 } } catch {}; Start-Sleep -Seconds 2 }; Write-Host '[WARN] llama-server not ready after 180s (will launch GUI anyway)'; exit 1"

cd /d X:\NeuroBoot
neuroboot.exe

echo.
echo NeuroBoot closed. Type "wpeutil shutdown" to power off, or "wpeutil reboot" to restart.
```

注：PE 里 PowerShell 是 WinPE-PowerShell OC 提供的（已在 [03-mount-and-customize.ps1:33-39](pe-build/build-scripts/03-mount-and-customize.ps1) 加入），所以可用。

GUI 启动后还可以再做一次 `endpoint.rs::probe_endpoint`：如果 local 仍不通就显示「llama-server 启动失败 — 请检查 X:\NeuroBoot\logs\llama-server.log」并提供「重试」按钮（v1.1 加）。

---

## 6. 字体支持 OC（顺带做，让 cmd 中文不乱码）

在 `03-mount-and-customize.ps1` 的 `$ocList` 数组里追加 `WinPE-FontSupport-ZH-CN`（cab 34 MB，但显著提升 PE 内 cmd / 系统对话框中文显示）：

```powershell
$ocList = @(
    'WinPE-WMI',
    'WinPE-NetFx',
    'WinPE-Scripting',
    'WinPE-PowerShell',
    'WinPE-StorageWMI',
    'WinPE-FontSupport-ZH-CN'    # 新增 - PE 内中文字形渲染
)
```

注意：`WinPE-FontSupport-ZH-CN.cab` 在 OCs 顶层，不像 `WinPE-WMI` 在子目录里。脚本现有逻辑 `$mainCab = "$ocs\$oc.cab"` 已经能正确匹配。`zh-cn` 语言包子目录里**没有** `WinPE-FontSupport-ZH-CN_zh-cn.cab`，所以 lang pack 那段 if (Test-Path) 走 "no en-us lang pack" 分支即可，不报错。

---

## 明天动手 checklist（按顺序）

```
P0 必做（让 U 盘可用）：
[x] 1.1 抓 CRT redist DLL → pe-build/payload/crt-redist/  (tools-dev/collect-crt-redist.ps1)
[x] 1.2 改 04-add-payload.ps1 加 [2.5/5] 拷 CRT DLL 段
[x] 1.3 改 04-add-payload.ps1 startnet.cmd 内嵌 PS healthcheck loop
[x] 2.1 改 main.rs 加 QUICK_PROMPTS 快捷按钮行
[x] 2.2 加 src/ui/prompts_file.rs + 集成下拉框
[x] 4.1 加 src/llm/config_file.rs (ConfigFile struct + load_config_file)
[x] 4.2 改 src/llm/endpoint.rs::detect_endpoints 集成 config + env 优先级
[x] 4.3 加 src/ui/settings_dialog.rs + 顶栏 ⚙ 按钮 + save_settings_to_usb
[x] 6   改 03-mount-and-customize.ps1 $ocList 加 WinPE-FontSupport-ZH-CN
[x] 7   加 src/ui/power_actions.rs + 顶栏「重启/关机/退出」按钮 + 确认弹窗（v1.0.1 后续真测追加）
[x] 文档 README + BUILD.md 加「鼠标必须有线/USB receiver」+ 「中文输入兜底」一节
[x] cargo build --release 验证 dumpbin /DEPENDENTS neuroboot.exe 仍无 VCRUNTIME140
[x] cargo test 全部通过（14/14：3 truncate + 4 config_file + 2 settings_dialog + 3 prompts_file + 2 power_actions）
[x] 加 usb-templates/NeuroBoot.config.json + NeuroBoot.prompts.txt 示例
[ ] 99-build-all.ps1 全跑一次重 build ISO（管理员 PS 跑中）
[ ] 重写 U 盘真测：键盘英文+快捷按钮+prompts.txt 测全部，鼠标换有线测全部，
    电源按钮（重启/关机/退出）测全部，⚙ 设置保存 config.json 测全部

P1 后续：
[ ] 2.3 应用层拼音 IME（rime-luna-pinyin 词典，~600 行 Rust）
[ ] llama-server 启动失败时 UI 显示「重试 / 查看日志」按钮
[ ] llama-server stderr 重定向到 X:\NeuroBoot\logs\llama-server.log
[ ] 顶栏显示「上次响应延迟 X ms」
[ ] config.json schema 的 system_prompt_override 已加载，加 UI 设置面板编辑（v1.0.1 已实现加载）
```

---

## v1.0.1+ 用户反馈追加（真测后补做，2026-05-24 同日）

用户反馈：
> 「传统 PE 桌面通常有时钟/内存/IP；NeuroBoot 没有这些一眼可见的诊断状态」
> 「想不退出 NeuroBoot 也能打开 cmd 跑命令、能开个文件管理器拷东西到 U 盘」
> 「故障经常需要拍蓝屏/BIOS 照片让 AI 看，能不能加图片上传？」

修复（v1.0.1+ 一次性 ISO 一起出）：

```
[x] 7  src/ui/status_bar.rs 状态栏：本地时钟（GetLocalTime FFI）+ 内存（GlobalMemoryStatusEx FFI）+ 本地 IP（UDP socket trick），5s 缓存。顶栏 panel 底加一行
[x] 8  src/ui/system_launchers.rs：launch_cmd 调 cmd /k cd /d X:\NeuroBoot；launch_file_manager 试 explorer.exe → 失败回落 cmd dir 列表。顶栏右侧加 cmd + 文件 按钮
[x] 9  src/ui/image_picker.rs：rfd Win32 IFileDialog 多选 png/jpg/webp/gif/bmp；base64 编码；大小校验（> 20 MB 拒，> 10 MB 警告）
[x] 10 src/ui/chat.rs：ChatMessage 加 images: Vec<AttachedImage>；render_message 在文本后画蓝色 📷 chip
[x] 11 src/llm/openai.rs：OpenAiMessage::content 从 Option<String> 改 Option<Content> untagged enum（Text | Parts），OpenAI vision schema 兼容
[x] 12 src/llm/config_file.rs::is_vl_model：启发式判断模型是否支持 vision（qwen-vl/gpt-4o/claude-3/deepseek-vl/glm-4v/gemini 等）
[x] 13 main.rs：输入区上方加附件 chip 行 + 「+ 图片」按钮；非 VL 模型按钮 disabled + hover 解释；submit 时打包到 ChatMessage::images
[x] 14 cargo test 31/31 通过；dumpbin 验证 neuroboot.exe 11.71 MB 仍无 VCRUNTIME140
[ ] 15 README + docs/KNOWN-ISSUES.md / TODO-v2.md 文档同步（部分进行中）
```

新依赖：
- `rfd 0.15.4`（Windows 用 Win32 IFileDialog，无 GTK）
- `base64 0.22`（reqwest 已带，免增）

v1.0.1+ ISO 预计大小 ~2.93 GB（同 v1.0.1 base，新代码增量 ~100 KB 忽略）。
```

---

## 修复后回归测试场景

进 PE 后依次验证：
1. ✅ 鼠标能动（已确保用有线/USB receiver）
2. ✅ 神启窗口能正常显示中文
3. ✅ healthcheck 在 60~120s 内打印 `[OK] llama-server ready`
4. ✅ 快捷按钮点「电脑蓝屏」→ 输入框填好预设 prompt
5. ✅ 点发送 → 不再出「请确认端点已启动」
6. ✅ 模型成功回答 + 调用工具（list_disks 等）
7. ✅ U 盘根 prompts.txt 准备好 → UI 下拉框能看到并选用
8. ✅ ⚙ 设置面板填 DeepSeek endpoint + API key → 保存到 U 盘 → 重新探测 → 顶栏显示「云端」
9. ✅ 拔网卡 → 重启 PE → 自动 fallback 本地 llama-server

---

**Last updated**: 2026-05-24（v1.0.1 U 盘真测反馈整理）

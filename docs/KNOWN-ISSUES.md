# NeuroBoot 项目踩过的坑 + 工作绕过

> 这些是项目从阶段 0 走到阶段 6 实际踩过的坑。下次复现 / 维护时，对照这里能省半天调试时间。

---

## 1. Smart App Control（Windows 11 25H2 默认开启）

**症状**：cargo run 启动 NeuroBoot.exe 时报错 `os error 4551：应用程序控制策略已阻止此文件`。Exit code 4551 = 0x11C7（"This program is blocked by Group Policy"）。

**根因**：Win11 25H2 默认开启 Smart App Control（SAC），强制阻止未签名 / 无云端 reputation 的 exe 运行。本地编译的 Rust exe 永远没 reputation。Tamper Protection 也开着，命令行无法关 SAC。

**解决**：
1. 设置 → 隐私和安全性 → Windows 安全中心 → 应用和浏览器控制 → Smart App Control 设置 → 选「关」
2. **实测 Win11 25H2 上**：无确认弹窗、不需重启、立即生效（早期 Win11 文档说"不可逆"已不准确，但官方还是说关后想再开要重置 Win）

**为啥不能签名解决**：自签名 SAC 不信任；商业代码签名证书 ~$200~500/年 + 每次编译都签 + 等 SAC 云 reputation 学习（几天到几周），开发期不实际。

**备选**：保留 SAC，所有阶段在 PE 里测（PE 无 SAC）—— 但进度滞后、视觉验证延后，不推荐。

---

## 2. Rust msvc target 默认不 crt-static

**症状**：用 `dumpbin /dependents` 看 `neuroboot.exe` 发现依赖 `VCRUNTIME140.dll` 与多个 `api-ms-win-crt-*` UCRT umbrella DLL。PE 里没装 VC++ Redistributable，启动失败。

**根因**：Rust 1.92 + `x86_64-pc-windows-msvc` target **默认动态链接** VC runtime，不是文档/经验里以为的 `+crt-static`。

**解决**：用 `+crt-static` target-feature 静态链接 VC runtime 进 exe（仅多 ~160 KB）：

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --manifest-path ...\Cargo.toml
```

**验证**：`dumpbin /dependents` 应**看不到** `VCRUNTIME140.dll` 与 `api-ms-win-crt-*`，只剩系统内置 DLL（kernel32 / user32 / opengl32 / gdi32 / ws2_32 / bcrypt 等）。

---

## 3. Cargo config 的 rustflags 在 Rust 1.92 实测不生效

**症状**：`.cargo/config.toml` 里写 `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]` 或 `[build] rustflags = ...`，`cargo clean --release` + 重 build，exe 依然依赖 VCRUNTIME140。

**根因**：未知。可能是 Rust 1.92 / Cargo 1.92 行为变化，或 UTF-8 BOM 解析问题。

**解决**：**用 `RUSTFLAGS` 环境变量传**（最可靠，每次都生效）：

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release ...
```

项目里 `tools-dev/build-release.ps1` 已经包好这个 env var。

---

## 4. PowerShell 5.1 + 中文 Windows 编码问题

### 4a. `.ps1` 文件含中文 → 解析报错

**症状**：跑 `install-adk.ps1` 报 `表达式或语句中包含意外的标记 "}"`、`字符串缺少终止符: "`、`语句块或类型定义中缺少右"}"`。

**根因**：Write 工具（包括很多 .NET API）默认用 **UTF-8 无 BOM** 写 .ps1。PowerShell 5.1 在中文 Windows（codepage 936 GBK）默认按 ANSI/GBK 解码 .ps1。中文 UTF-8 是 3 字节序列，被 GBK 解码字节边界错位 → 字符串和注释终止符乱掉 → parser error。

**解决**（择一）：
- **纯英文 .ps1**（最简最可靠）—— 注释、Write-Host 都用英文
- **UTF-8 with BOM .ps1** —— PS 5.1 看到 `EF BB BF` 会切到 UTF-8。Write tool 不直接支持 BOM；要用 PS tool `[System.IO.File]::WriteAllText($path, $content, [System.Text.UTF8Encoding]::new($true))`
- **改用 PowerShell 7+** —— 默认 UTF-8（含 BOM-less）

项目里所有 `pe-build/build-scripts/*.ps1` 都是**纯英文**避开这个坑。

### 4b. PowerShell 5.1 native exe 用 `2>&1` 假阳报错

**症状**：`cargo build 2>&1` 看起来失败，但实际 exit 0 + `Finished dev profile`。

**根因**：PS 5.1 把 native exe 的 stderr 包装成 ErrorRecord（NativeCommandError）+ 设 `$?` 为 `$false`，即使 exit code 0。

**解决**：**不要对 cargo / git / winget / rustc 等 native exe 用 `2>&1`**。直接调用，让 stderr 自然走 PS 错误流（PS tool 会捕获）。如果要过滤输出，用 `| Select-Object` 不要 `2>&1 |`。

### 4c. `[System.IO.File]::ReadAllBytes` 对 >2 GB 文件失败

**症状**：验证 Qwen GGUF 时报 `The file is too long. This operation is currently limited to supporting files less than 2 gigabytes in size.`

**根因**：PS 5.1 用 .NET Framework 4.x，`ReadAllBytes` 对 >2 GB 文件抛 IOException。但 `Invoke-WebRequest -OutFile` 本身**不受此限**，文件已经下载成功，只是 magic check 失败。

**解决**：用流式读：

```powershell
$bytes = Get-Content $file -Encoding Byte -TotalCount 8  # 只读前 8 字节
```

---

## 5. Mesa-dist-win 在主系统 override 必崩

**症状**：把 mesa-dist-win 的 `x64/opengl32.dll` + `libgallium_wgl.dll` 放 NeuroBoot.exe 同目录，跑 NeuroBoot 立即崩溃，exit code `0x80070057 = E_INVALIDARG`，无任何 stderr 输出。

**根因**：Mesa wgl wrapper 与真硬件 GPU 驱动（实测 Intel Ultra 7 255H iGPU）的 ICD / WGL context creation 路径冲突。mesa3d-windows / mesa-dist-win GitHub issues 历史多次报告 NVIDIA / Intel / AMD 真驱动机器同样症状。

**结论**：
- ❌ **主系统模拟 Mesa override 不可靠** —— 真驱动 ICD 冲突
- ✅ **PE 里预期 Mesa 工作正常** —— 没真 GPU 驱动 + 没 ICD 冲突，Mesa 是唯一 OpenGL 来源
- **明天 PE 真测见分晓**

阶段 5 决策：放弃主系统 Mesa 模拟，Mesa DLL 留 `pe-build/payload/neuroboot/`，阶段 6 PE 集成后真测。

**Workaround 思路**（如果 PE 里 Mesa 也崩）：
- 试 Microsoft 自带 `opengl32sw.dll`（D3D9-based WARP software GL，覆盖度差但兼容性好）
- 试旧版 Mesa（25.x / 24.x）
- 改 NeuroBoot 用 wgpu backend + WARP（D3D12 software） —— 大改

---

## 6. ADK setup 在 `/passive` 模式下可能只下载不安装

**症状**：跑 `adksetup.exe /passive /features OptionId.DeploymentTools`，进度条窗口出现 + 关闭，但安装路径 `C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\` 不存在；反而 `Downloads\Windows Kits\10\ADK\` 出现了 `adksetup.exe + Installers/`（layout 模式输出）。

**根因**：ADK setup 在某些场景默认行为是先下载 layout（离线安装包）到 Downloads，等用户确认后才 install。`/passive` 模式可能用户没看清提示而提前关闭。

**解决**：从已下的 layout 路径**重新调用 setup**（它会用 local Installers 不再下载，直接 install）：

```powershell
& 'C:\Users\<USERNAME>\Downloads\Windows Kits\10\ADK\adksetup.exe' /passive /norestart /features OptionId.DeploymentTools
```

**验证**：跑完后 `Test-Path 'C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools'` 应返回 True。

---

## 7. `copype.cmd` 直接调用找不到 amd64

**症状**：`copype amd64 <workspace>` 直接调用报 `ERROR: The following processor architecture was not found: amd64`，即使 `<adk>\Windows Preinstallation Environment\amd64\` 明显存在。

**根因**：copype.cmd 第 21 行 `set SOURCE=%WinPERoot%\%WINPE_ARCH%`，依赖 `%WinPERoot%` 环境变量。直接调用时这个变量没设，所以 SOURCE 变成 `\amd64` 找不到。

**解决**：先 `call DandISetEnv.bat`（设 PATH / WinPERoot / OSCDImgRoot 等），再 `copype amd64 ...`。包装在 .cmd wrapper 里：

```cmd
@echo off
call "C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools\DandISetEnv.bat"
"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Windows Preinstallation Environment\copype.cmd" amd64 "C:\NeuroBoot\pe-build\workspace"
```

项目里 `pe-build/build-scripts/02-run-copype.cmd` 已经包好。

---

## 8. DISM mount-image 需要 admin

**症状**：`copype` 在 staging 阶段报 `ERROR: Failed to mount the WinPE WIM file. Check logs at C:\Windows\Logs\DISM`。

**根因**：DISM `/Mount-Image` 需要管理员权限（操作 .wim 文件 + 创建 reparse points）。普通 PowerShell 不够。

**解决**：用 `Start-Process -Verb RunAs -Wait` 在 PowerShell tool 里弹 UAC 让用户授权，或者**用户直接在 admin PowerShell** 跑。**注意 UAC 弹窗用户必须点「是」**（点「否」整个流程失败）。

---

## 9. `Start-Process -Verb RunAs` 在 PowerShell tool 里可能被拒

**症状**：在 PowerShell tool 调用 `Start-Process -Verb RunAs` 弹 UAC，用户点了取消（或 UAC 设置不让弹），报 `The operation was canceled by the user`。

**解决**：直接让用户在已开的 admin PowerShell 跑命令，更可靠（不每次弹 UAC）。

```powershell
# 在 admin PS 跑
PowerShell -NoProfile -ExecutionPolicy Bypass -File <script.ps1>
```

---

## 10. 国内 GitHub raw / jsdelivr 不可靠

**症状**：下载 Noto Sans SC 字体、llama.cpp release、Mesa-dist-win 等时，GitHub raw 连接被中断；jsdelivr 各节点返 403（疑似对热门字体路径反爬）；ghproxy 连接关闭或返错误页。

**已验证可行的优先级（高到低）**：

1. **npmmirror（registry.npmmirror.com，阿里维护）** —— 最稳。npm 包元信息 + tarball 都能下，70+ MB 也稳。适合 fontsource / @expo-google-fonts / 各种 npm 工具
2. **清华 pip 镜像**（`pip install -i https://pypi.tuna.tsinghua.edu.cn/simple ...`）—— Python 工具
3. **hf-mirror.com** —— HuggingFace 镜像，模型 GGUF 下载稳
4. **GitHub release direct** —— 偶尔通，多试几次（实测 llama.cpp / Mesa / Ventoy / ADK 直链都成功过）

**坑：fontsource v5 只发 woff/woff2 不发 ttf**

egui 用 ab_glyph 加载字体需要 ttf。下了 fontsource 的 woff2 后用 fontTools 转 ttf：

```powershell
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple fonttools brotli
python -c "from fontTools.ttLib import TTFont; f=TTFont(r'xxx.woff2'); f.flavor=None; f.save(r'xxx.ttf')"
```

详见 `app/assets/fonts/NotoSansSC-Regular.ttf` 当时的 build 步骤。

---

## 11. eframe 0.34 App trait `update` → `ui`

**症状**：用 eframe 0.27 时代的 `fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame)` 写法在 eframe 0.34 编译报 `error[E0046]: not all trait items implemented, missing: ui`。

**根因**：eframe 0.34 把 `App` trait 的 required method 从 `update(ctx, frame)` 改成 `ui(ui, frame)`。框架已经替你做了 `CentralPanel` 等价布局，直接在给定的 `ui` 上 paint。

**解决**：用 `fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)` 写法，去掉 `egui::CentralPanel::default().show(ctx, |ui| { ... })` 包装（直接在 ui 上 paint）。

附带：`egui::TopBottomPanel::top/bottom` 也 deprecated → 用 `egui::Panel::top/bottom`。

---

## 12. eframe 0.34 默认 wgpu 后端，不是 OpenGL/glow

**症状**：阶段 0 简报里假设 "egui 默认走 OpenGL"，但 `cargo add eframe` 装完后默认 features 是 `wgpu`，不是 `glow`（OpenGL）。

**根因**：eframe 0.27+ 默认切到 wgpu。glow 需要显式启用。

**影响**：要让 Mesa llvmpipe 软件 OpenGL 兜底方案生效，必须切回 glow（wgpu 在 PE 默认走 D3D12 无法初始化）。

**解决**：Cargo.toml 显式 features：

```toml
eframe = { version = "0.34.2", default-features = false, features = ["glow", "default_fonts"] }
```

---

## 13. llama-server context size 默认 4096 不够多轮诊断

**症状**：连续问几个工具调用问题（每个工具返回 JSON 几百 token）后，llama-server 返 HTTP 400 `request exceeds the available context size`。

**解决**：
1. 启动 llama-server 时给 `-c 16384`（Qwen3-4B 原生支持 256K，16K 是 RAM 与可用空间平衡点）
2. 客户端加 truncation（阶段 4.1 已实现，每次请求前按整 turn 删最早对话）

参见 `app/src/agent/truncate.rs`。

---

## 14. PowerShell 5.1 的 `2>$null` 与 `2>&1` 区别

**症状**：想抑制 native exe 的 stderr 噪音，但用 `2>&1` 触发 NativeCommandError 假阳报错；用 `2>$null` 又不知道有没有真错。

**解决**：
- `2>$null` —— PS 自己的 redirect 到 null，**不**触发 NativeCommandError
- `2>&1` —— **千万别**对 native exe 用（坑见 4b）
- 想看 stderr 又不污染：跑命令然后单独 `$LASTEXITCODE` 看 exit code，stderr 自然显示

---

## 15. llama.cpp release build 没 crt-static，PE 启动闪退（v1.0.1 真测发现）

**症状**：U 盘启动 NeuroBoot.iso 进 PE，GUI 出来后发消息提示「请确认端点已启动」。`start /MIN cmd /c llama-server.exe` 把闪退窗口最小化看不见根因。

**根因**：`dumpbin /DEPENDENTS C:\NeuroBoot\tools-dev\llama-cpp\b9294\llama-server.exe` 实测依赖：
- `VCRUNTIME140.dll`
- `api-ms-win-crt-runtime/stdio/math/locale/heap-l1-1-0.dll` 等

llama.cpp 官方 release build 没用 `+crt-static`，PE 不带 VC++ Redist + UCRT umbrella，加载即失败。NeuroBoot 自家 exe 之前做了 crt-static，但**忘了 llama.cpp 也是 msvc build 同样需要**。

**解决（v1.0.1 方案 C —— 本地拷 CRT redist 不重 build）**：
1. `tools-dev\collect-crt-redist.ps1` —— 从 VS 2026 redist 抓 `vcruntime140.dll`；从 Win10 SDK Redist `10.0.26100.0\ucrt\DLLs\x64\` 抓 15 个 `api-ms-win-crt-*.dll`；从 `C:\Windows\System32` 抓 `ucrtbase.dll`。总 17 个 DLL ~1.9 MB → `pe-build\payload\crt-redist\`
2. `04-add-payload.ps1` [2.5/5] 段拷到 `\NeuroBoot\llama-cpp\` 与 llama-server.exe 同目录 → Windows 加载器 local-load 优先于系统目录搜索

**坑中坑**：`api-ms-win-crt-*.dll` **不在** `C:\Windows\System32\` —— modern Windows 把它们封装进 ucrtbase.dll 的 API set，要从 Windows 10 SDK Redist 目录拿散件。

**规则**：**任何为 PE 打包的 msvc build 第三方 exe**（llama.cpp / smartmontools / 7zip / BlueScreenView 等）—— 进 PE 前都要 `dumpbin /DEPENDENTS` 验证；带 VCRUNTIME140 就把 CRT redist 拷同目录。

---

## 16. PE GUI 程序没显式重启/关机按钮，用户只能长按电源（v1.0.1 真测发现）

**症状**：PE 真测 NeuroBoot 出问题，没法干净地重启 PE 重试，只能长按电源键强行关机。

**根因**：PE 跟主系统不一样：
- 没有任务栏 / 开始菜单 / 系统托盘的关机入口
- 默认 PE shell 是 cmd，不是 explorer.exe
- GUI 程序窗口关闭 → 回到 cmd 提示符 → 用户得敲 `wpeutil shutdown` / `wpeutil reboot` —— 多数终端用户不知道这俩命令
- 蓝牙鼠标这时还动不了（坑 #17 PE 无蓝牙），cmd 里输命令难

**解决（v1.0.1）**：NeuroBoot 顶栏右上加 3 按钮：
- **重启电脑** → 确认弹窗 → `wpeutil.exe reboot`
- **关机** → 确认弹窗 → `wpeutil.exe shutdown`
- **退出程序** → 确认弹窗 → `std::process::exit(0)` 回 PE cmd（最后备份，让会敲命令的用户走 cmd 维护）

代码在 `app/src/ui/power_actions.rs`。每个动作都走「等同 dangerous tool 的确认弹窗」防误点。

**规则**：**任何为 PE 设计的 GUI 程序**都必须显式提供重启/关机入口 —— 用户对 PE 没习惯，找不到系统级关机入口。

---

## 17. PE 不带蓝牙 stack（v1.0.1 真测发现）

**症状**：插了蓝牙鼠标的笔记本，PE 启动后鼠标完全不动。

**根因**：Win10/11 ADK WinPE 不包含蓝牙驱动栈（BTHport / BTHusb / RFCOMM 等）—— ADK 设计极小子集，不带可选硬件子系统。微软文档 [What's included in Windows PE](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/winpe-intro) 明确列出：**Bluetooth not supported**。

**解决**：**无软件解决**。只能：
- 准备有线 USB 鼠标 / 键盘
- 或者 2.4G USB receiver 无线鼠标（罗技 Unifying / 普通 dongle）
- 文档化告知用户（README + BUILD.md「硬件要求」节）

笔记本触控板大多支持（HID-compliant 走通用驱动），但少数新平台 Precision Touchpad 需要 OEM 驱动 —— PE 没装。

---

## 18. PE 没 IME 框架（v1.0.1 真测发现）

**症状**：PE 里 NeuroBoot 聊天框无法输入中文，只能输 ASCII。Win + Space 切输入法没反应。

**根因**：Win10/11 ADK 不包含 IME OC + ctfmon / TSF / IMM32 基础设施。PE 是部署 / 恢复极小子集，官方不带文本服务框架。从主系统硬拷 IME 文件违 EULA + 经常崩溃。

**解决（v1.0.1，三层叠加）**：
1. NeuroBoot 内置 6 个**常见问题快捷按钮**（电脑蓝屏 / 硬盘问题 / 网络故障 / 启动慢 / 找回误删 / 系统修复）—— 点按钮直接预填 prompt
2. NeuroBoot 启动时扫所有非 X: 盘根目录找 **`NeuroBoot.prompts.txt`**（每行一条候选问题，行首 `#` 注释）→ UI 下拉框选用。用户在主系统里把常用问题敲好放 U 盘
3. v1.1 路线图：应用层拼音 IME（rime-luna-pinyin 词典，~600 行 Rust），完整中文输入

记忆里有 [[feedback-winpe-no-ime]] 完整规则。

---

## 19. Start-Transcript 日志放在被 Remove-Item 的目录里 → 静默失败（v1.0.1 发现）

**症状**：`99-build-all.ps1` 跑到 Phase 2 (copype) 报 `ERROR: Destination directory exists: "C:\NeuroBoot\pe-build\workspace"`。

**根因**：脚本开头 `Start-Transcript -Path "$root\pe-build\workspace\build-all.log"` 在 workspace 下开了日志文件。到 Phase 2 想 `Remove-Item "$root\pe-build\workspace" -Recurse -Force -ErrorAction SilentlyContinue`，这个文件被当前 PowerShell 进程自己锁住 → `SilentlyContinue` 把删除失败的错误吃掉 → workspace 仍存在 → copype 报「目录已存在」。

**解决**：
1. **日志文件路径不要放在被 Remove-Item 的目录里**。NeuroBoot 把 `$logFile` 移到 `$root\pe-build\build-all.log`（workspace 上一级）
2. `Remove-Item` 时去掉 `-ErrorAction SilentlyContinue`，让真正删不掉的情况报出来而不是吃掉

**规则**：**自动化脚本里凡是「先 Remove-Item 后重建」的目录，绝不在里面放当前进程要写的文件**（日志、临时文件、锁文件）。`-ErrorAction SilentlyContinue` 是反模式 —— 让脚本继续走但错误信号丢了，后面 phase 报莫名其妙的错。

---

## 总结：从坑里学到的工程纪律

1. **任何 .ps1 含中文 → 用纯英文或 UTF-8 with BOM**
2. **Rust 为 PE 编译 → 必须 `RUSTFLAGS env var` 而不是 cargo config**
3. **国内下载 → npmmirror / 清华 pip / hf-mirror 优先于 GitHub raw**
4. **DISM / WIM 操作 → 必须 admin PowerShell**
5. **看 exe 真依赖 → 用 `dumpbin /dependents` 验证（NeuroBoot.exe + 所有第三方打包 exe）**
6. **Mesa override 必须在干净环境测（PE / 干净 VM），主系统真 GPU 驱动会冲突**
7. **不可逆操作（关 SAC、格式化 U 盘）→ 明确告知用户、确认意图**
8. **PE GUI 程序必须显式提供重启/关机按钮**（坑 #16）
9. **PE 不带蓝牙 / IME，硬件 / 软件需求要文档化**（坑 #17、#18）
10. **PowerShell `Remove-Item` 路径不能含当前进程持有文件**（坑 #19）

# NeuroBoot 神启

**语言 / Language:** [English](README.md) · **简体中文**

> **Neuro = AI 智能，Boot = 启动盘** —— 集成自研 AI 助手的 Windows PE 维护盘。

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6)](docs/BUILD.md)
[![Build](https://img.shields.io/badge/PE%20Build-WinPE%20amd64-success)](docs/BUILD.md)

一张 U 盘启动进 PE，桌面上是一个会思考的中文 AI 助手 (`NeuroBoot.exe`)：

- **GUI**：Rust + egui/eframe，glow/OpenGL 后端 + 嵌入式 Noto Sans SC 字体
- **渲染兜底**：Mesa3D llvmpipe（PE 无 GPU 驱动也能渲染）
- **本地大模型**：Qwen3-4B-Instruct-2507 Q4_K_M（llama.cpp `llama-server` 跑 CPU 推理）
- **Agent**：自实现 tool-use 多轮循环（路线 A，不依赖第三方 SDK），OpenAI function calling 兼容
- **工具**：4 个，3 safe（`list_disks` / `read_system_info` / `read_event_log_errors`）+ 1 dangerous（`delete_path`，弹窗确认 + 黑名单防整盘）
- **A+C 双备**：env / config.json / UI ⚙ 三层配置远端 A 端点，启动探测可达则用 A，否则本地 C；UI 顶栏一键切换
- **vision 多模态**（v1.0.1+）：「+ 图片」按钮（rfd 文件对话框 + base64 data URL），OpenAI vision schema 兼容；VL 模型自动检测（gpt-4o / claude-3 / qwen-vl / deepseek-vl 等），非 VL 时禁用按钮提示
- **状态栏**（v1.0.1+）：本地时钟 + 内存占用 + 本机 IP，5s 缓存刷新（Win32 FFI 直调，无新依赖）
- **维护辅助**（v1.0.1+）：顶栏 cmd / 文件管理器 / 重启 / 关机 / 退出 5 个按钮，电源类 wpeutil 调用走确认弹窗
- **PE 兼容**：crt-static 静态链接 VC runtime；token truncation 防 ctx 爆；UTF-8 中文管线全程；llama.cpp release CRT redist 17 DLL 同目录拷贝

## 当前状态

- ✅ **v1.0 完成** —— `NeuroBoot.iso` 2.89 GB（2026-05-23）
- ✅ **v1.0.1+ 完成** —— `NeuroBoot.iso` 2.93 GB（2026-05-24），4 个真测 P0 修复 + 5 个用户反馈追加功能：
  - CRT redist 修 llama-server PE 闪退；startnet healthcheck 替代 timeout 60
  - 中文输入兜底（6 快捷问题 + U 盘 prompts.txt 下拉）
  - 在线 AI 端点 ⚙ 设置面板 + config.json 持久化
  - WinPE-FontSupport-ZH-CN（cmd / 系统对话框显中文）
  - **顶栏 3 电源按钮**：重启 / 关机 / 退出（wpeutil reboot/shutdown，确认弹窗）
  - **状态栏**：时钟 · 内存 X/Y MB · 本地 IP（5s 刷新）
  - **2 启动器按钮**：cmd（cd 到 X:\NeuroBoot）/ 文件管理器（试 explorer → cmd dir 回落）
  - **图片上传**：+ 图片按钮选 png/jpg/webp，OpenAI vision 多模态 schema 发送，VL 模型自动检测（gpt-4o / claude-3 / qwen-vl / deepseek-vl 等关键词）
- ✅ **v2 Stage A 完成** —— Markdown 渲染 (egui_commonmark 0.23) + 8 个新 safe 工具（共 12 个工具）。**未重 build ISO**
- 🚧 **v2 Stage 1 进行中** —— Agent 基础健壮性：system prompt 重写 + 12 工具 description 按 Anthropic spec 重写 + Q5_K_M 量化升级
- 📋 **v2 完整路线图（8 Stage）** —— 详见 **[docs/TODO-v2.md](docs/TODO-v2.md)**（流式输出 / 危险工具 / 本地视觉 / 救援旗舰工具 / UX / MCP；**已排除微软 Phi / QMR / Foundry Local**）
- 📚 **2026-05 调研汇总** —— 详见 **[docs/RESEARCH-2026-05.md](docs/RESEARCH-2026-05.md)**（VL 模型选型 / agent 架构 / 微软生态分析 / SmolVLM 不适用证据，~50 轮 web 调研 ~120 个引用 URL）

## 硬件要求（目标机 / 真测环境）

| 项 | 必须 | 推荐 |
|---|---|---|
| RAM | ≥ 4 GB | ≥ 8 GB |
| Secure Boot | 启动 PE 时关 | — |
| **鼠标 / 键盘** | **有线 USB 或 2.4G USB receiver 无线** | — |

**⚠ 不支持蓝牙鼠标 / 键盘** —— Windows PE 不含蓝牙 stack（ADK 设计硬限制，软件无法绕过）。请准备一只有线 USB 鼠标或自带 dongle 的 2.4G 无线鼠标再插 U 盘开机。

**⚠ 中文输入：** PE 无 IME，NeuroBoot 内置 6 个常见问题快捷按钮 + 支持读 U 盘 `NeuroBoot.prompts.txt`（每行一条候选问题）。完整拼音 IME 在 v1.1 路线图上。

**⚠ 在线 AI 端点配置：** 点 NeuroBoot 顶栏 ⚙ 设置按钮，填 endpoint URL / model / API key，可保存到 U 盘 `NeuroBoot.config.json`，下次启动自动加载。

## 文档导航

- **[BUILD.md](docs/BUILD.md)** —— **从零开始的完整构建攻略** + 一键自动化脚本
- **[KNOWN-ISSUES.md](docs/KNOWN-ISSUES.md)** —— 项目过程中踩过的所有坑 + 工作绕过（19 条 + 总结）
- **[TODO-v1.0.1-fixes.md](docs/TODO-v1.0.1-fixes.md)** —— U 盘真测反馈的紧急修复清单（CRT 闪退 / PE 无 IME / 配置 UI / 状态栏 / 启动器 / 图片上传）
- **[TODO-v2.md](docs/TODO-v2.md)** —— **v2.0 路线图**：14 路 WebSearch 调研 + v1 反馈整理的工具扩充 / 架构改进清单（P0/P1/P2 优先级）
- **[docs/usb-templates/](docs/usb-templates/)** —— U 盘 `NeuroBoot.config.json` + `NeuroBoot.prompts.txt` 示例

## 关键产物

| 产物 | 路径 | 大小 |
|---|---|---|
| 最终 ISO（v1.0.1+） | `pe-build/output/NeuroBoot.iso` | ~2.93 GB |
| Rust release exe | `app/target/release/neuroboot.exe` | ~11.71 MB (crt-static, 含 rfd) |
| PE payload | `pe-build/payload/neuroboot/` | ~70 MB |
| CRT redist（v1.0.1+） | `pe-build/payload/crt-redist/` | 1.9 MB (17 DLL) |
| Qwen GGUF | `models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf` | 2.33 GB |
| llama.cpp b9294 (CPU) | `tools-dev/llama-cpp/b9294/` | ~50 MB |
| Mesa-dist-win 26.1.1 | `tools-dev/mesa-extract/x64/` | 顶层 opengl32.dll + libgallium_wgl.dll |
| Ventoy 1.1.12 | `tools-dev/ventoy/ventoy-1.1.12/` | 15.94 MB |
| USB 配置模板 | `docs/usb-templates/` | NeuroBoot.config.json + prompts.txt |

## 一句话复现

如果你已经按 [BUILD.md](docs/BUILD.md) 前置条件准备好环境（装 ADK、装 Rust、下完模型、解压完 Mesa），在 admin PowerShell 跑：

```powershell
PowerShell -NoProfile -ExecutionPolicy Bypass -File C:\NeuroBoot\pe-build\build-scripts\99-build-all.ps1
```

这个一键脚本会跑完 cargo build → payload collect → copype → mount → 加 OCs → 加 payload → unmount/commit → MakeWinPEMedia /ISO 全流程，5~20 分钟产出 `NeuroBoot.iso`。

## 许可证

本项目使用 **[Apache License 2.0](LICENSE)** 许可。

- ✅ 商业使用 / 修改 / 分发 / 私用 都允许
- ✅ 明确专利授权（含反诉条款）
- ⚠ 必须保留版权声明 + 标注修改 + 不可使用「NeuroBoot」「神启」商标推广 fork
- 第三方组件（Noto Sans SC / llama.cpp / Mesa / Qwen 模型 / Ventoy / Microsoft CRT 等）的 attribution 详见 **[NOTICE](NOTICE)**

## 贡献

提 issue / PR 前请先看：
- [docs/KNOWN-ISSUES.md](docs/KNOWN-ISSUES.md) 是否是已知坑
- [docs/TODO-v2.md](docs/TODO-v2.md) 是否已在路线图

代码风格：跟现有模块的中文注释 + 单测覆盖 + dumpbin /DEPENDENTS 验证 neuroboot.exe 不引入 VCRUNTIME140 依赖。

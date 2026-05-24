# NeuroBoot v3 路线图（2026-05 综合调研后定）

> **关联文档**：
> - **基础调研**：[RESEARCH-2026-05.md](RESEARCH-2026-05.md)（v1.0.1+ ~ v2 期间的 5 份 agent 调研合集）
> - **v2 完整路线**：[TODO-v2.md](TODO-v2.md)（v2 Stage 1-8 全部代码已完成；含 v2.x 残留）
> - **真测反馈来源**：[TODO-v1.0.1-fixes.md](TODO-v1.0.1-fixes.md)（v1.0 U 盘真测的 4 个 P0 修复 + 5 个用户反馈追加）
> - **已知坑**：[KNOWN-ISSUES.md](KNOWN-ISSUES.md)
>
> 本文档基于 2026-05-24 v3 阶段的 4 份 agent 调研（PE 救援盘横评 + AI agent 能力 + 司法取证可行性 + SmolVLM 否决）共识，固化下一阶段（v3.0 → v3.1 → v3.x）的完整路线。

---

## 当前状态快照（2026-05-24）

| 维度 | 状态 |
|---|---|
| **v1.0** | ✅ `NeuroBoot.iso` 2.89 GB（2026-05-23） |
| **v1.0.1+** | ✅ `NeuroBoot.iso` 2.93 GB（2026-05-24，4 个真测 P0 + 5 个用户反馈追加） |
| **v2 Stage A** | ✅ Markdown 渲染 + 8 新 safe 工具（共 12） |
| **v2 Stage 1-8 全部** | ✅ 代码全部 commit/push；6 个独立 commit |
| **v3 Quick Wins 1-4** | ✅ prompt cache + 7-Zip + BSOD 分析 + SKILL YAML（commit `b9bb6e7`） |
| **外部工具下载流程** | ✅ docs/BUILD.md + helper script + ISO build [2.6/5] 自动拷贝（commit `9c81cc6`） |
| **工具总数** | **22**（15 safe + 7 dangerous） |
| **单测** | **64/64 pass** |
| **neuroboot.exe** | **12.46 MB** crt-static 仍清白 |
| **ISO 重 build** | ⏳ 待触发（v2 + v3 所有改动积压） |

---

## 已明确**不做**的方向（基于前期决策）

- **Microsoft Phi / Phi Silica / QMR / Foundry Local** —— 锁 Copilot+ PC NPU + 中国不可用 + PE 不兼容
- **SmolVLM 系列** —— 完全不支持中文（HuggingFace 官方模型卡确认）
- **司法取证级合规** —— 用户决定不做；`--forensic` 仅作「谨慎模式」用
- **Multi-agent orchestration**（AutoGen / CrewAI / MetaGPT）
- **NPU-first 路线** —— NeuroBoot CPU-first 是核心护城河
- **Cline 风格 LLM 自分类 dangerous** —— Anthropic Auto Mode 仍 17% false-negative
- **Background agent 并行**（Cursor 风格）—— PE 内存紧 + 软件渲染 OOM
- **Speculative decoding** —— Qwen3 CPU 实测净退化 -3~-12%
- **Voice agent** —— PE 无 audio stack
- **盲目升级 Qwen3.5-4B** —— thinking mode TTFT 翻倍

---

## Sprint 1：触发现有积压（1~3 天，0~少量代码）

| # | 任务 | 工程量 | 价值 |
|---|---|---|---|
| 1.0 | **触发 ISO 重 build** —— admin PowerShell 跑 `99-build-all.ps1`，含 v2 全 8 stage + v3 Quick Wins + 自动拷贝外部工具（若已下载） | ~3~10 min build + UAC | **全部已 commit 才能生效** |
| 1.1 | **跑 download-external-tools.ps1** —— 5 个 binary 进位 `C:\NeuroBoot\tools\` | ~3~5 min | 5 个 AI 工具（NTPWEdit/TestDisk/smartctl/7za/BlueScreenView）从 NotFound 变可用 |
| 1.2 | **U 盘真测** —— 拷新 ISO，PE 真测 prompt cache TTFT / 22 工具 / skill / MCP / `--readonly` / `--forensic` | ~30 min | **没真测不知道哪些坑**（v1.0.1 教训：CRT 闪退 / 蓝牙 / IME / 电源按钮 都是真测后才发现） |
| 1.3 | 真测反馈记录 → 写进 `docs/TODO-v3.0.1-fixes.md`（类比 v1.0.1）| 30 min | 把真测发现的小坑跟 v3.0 P0 区分开 |

---

## Sprint 2：v3.0 P0（4~6 周）

**主题**：PE 救援盘基本盘补齐 + AI 自动化大幅增强

### v3.0 P0-1 ~ P0-6（按推荐执行顺序）

| # | 任务 | 工程量 | 关键参考 |
|---|---|---|---|
| **P0-1** | **Wi-Fi 连接 GUI** —— egui 弹窗 + `netsh wlan show profiles/connect` + WPA2/3 输入；可选 [wpa_supplicant Windows port](https://github.com/AlienCowEatCake/wpa_supplicant-windows) | 3~5 天 | 知乎用户 PE 痛点 TOP；微 PE / FirPE / HotPE 全有 |
| **P0-2** | **Hook 系统** —— `~/NeuroBoot/hooks/hooks.json` 配置；至少 4 事件 (PreToolUse / PostToolUse / SessionStart / Stop)；handler 类型 Command + HTTP | 3~5 天 | [Claude Code Hooks Complete Guide 2026](https://ofox.ai/blog/claude-code-hooks-subagents-skills-complete-guide-2026/) |
| **P0-3** | **持久化 Memory** —— U 盘 `NeuroBoot/memories/` + 6 命令 view/create/str_replace/insert/delete/rename + SessionStart hook 自动加载 + path traversal 防护 | 4~6 天 | [Claude memory tool 文档](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool) |
| **P0-4** | **内置文件管理器** —— 用 [egui-file-dialog](https://crates.io/crates/egui-file-dialog) 双面板封装 或 拷 Q-Dir Portable | 1~2 周 | 替换 explorer.exe 反复不稳；数据救援核心 |
| **P0-5** | **整盘 / 分区备份 + 恢复** —— 集成 `wbadmin` 包装 + AOMEI Backupper Free CLI（Macrium 转订阅 / Acronis 砍 free，2025 仅剩 AOMEI / Hasleo 可商用免费） | 2~3 周 | [AOMEI Free 现状](https://www.elevenforum.com/t/aomei-backupper-advise/39853) |
| **P0-6** | **数据恢复 GUI** —— PhotoRec + [DMDE Free 4000 文件/次](https://slashdot.org/software/comparison/DMDE-vs-PhotoRec/) 便携版 + 2 AI 工具 `photorec_scan` / `dmde_browse_lost_files` | 1 周 | 误删 / 格式化 / 分区损坏 救援；高用户感知 |

**Sprint 2 合计**：~6~8 周

**Sprint 2 末 release v3.0**

---

## Sprint 3：v3.1 P1（5~8 周）

**主题**：让 NeuroBoot 成为带 AI 的 Sergei Strelec 中文版

| # | 任务 | 工程量 | 关键参考 |
|---|---|---|---|
| **P1-7** | **NirLauncher 套件 + AI 包装** —— [250+ Windows 排错小工具](https://launcher.nirsoft.net/) ~50MB 全便携 + AI safe 工具 list_nirsoft_utilities 按需 invoke | 3~5 天 | 极小工程量 + 250 工具回报 |
| **P1-8** | **Plan Mode** —— Cline 风格：AI 先列只读计划 → 用户 approve 才执行；结合 P0-1 prompt cache（accept plan = clear context 新窗口执行） | 2~4 天 | [Cline Setup Guide 2026](https://www.deployhq.com/guides/cline) |
| **P1-9** | **本地 RAG 知识库** —— [Qwen3-Embedding-0.6B GGUF](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) + [sqlite-vec](https://github.com/asg017/sqlite-vec) + tantivy 混合检索；数据：[Microsoft BugCheck 0x01~0x1FF](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-code-reference2) + Windows 错误码 + NeuroBoot 工具描述 | 2~3 周 | 「0x0000007B」→ RAG 查 INACCESSIBLE_BOOT_DEVICE → AI 中文答 |
| **P1-10** | **RustDesk 远程协助** —— portable 集成；顶栏「远程」按钮显示 ID + 一键密码 | 3~5 天 | [RustDesk 官网](https://rustdesk.com/)；HotPE 已支持 |
| **P1-11** | **Citation 标注** —— tool result `source_id` + AI 输出 `[^1]` 锚点可点击展开 | 2~3 天 | Perplexity 风格 |
| **P1-12** | **Tool Result Clearing 升级** —— [Claude `clear_tool_uses_20250919`](https://platform.claude.com/docs/en/build-with-claude/context-editing) 替代当前「整 turn 删」过粗 | 3~4 天 | v2 Stage 3 已做基础版深化 |
| **P1-13** | **CrystalDiskInfo + AI 健康解读** —— CDI 9.8 portable + `/CopyExit` + AI 翻译 DiskInfo.txt | 1~2 天 | 配合 read_smart 给「我盘还能用多久」 |
| **P1-14** | **System Informer**（Process Hacker 续）—— portable 集成 + AI 工具 dump_process_handles | 2~3 天 | list_processes_top 太弱时的深查工具 |
| **P1-15** | **Eval 回归测试框架** —— 30~50 个 golden prompt + 期望工具调用序列；Rust ~200 行 runner（不上 DeepEval / promptfoo 重型框架） | 5~7 天 | 改 prompt / 切模型不再黑盒 |
| **P1-16** | **PII Redaction**（云端模式）—— regex + 字典 80% 覆盖；hostname / MAC / IP / SN → 占位符 → rehydrate | 5~7 天 | [LogRocket 本地 AI proxy 指南](https://blog.logrocket.com/build-local-ai-proxy-redact-pii-before-llms/) |
| **P1-17** | **SKILL.md 三层加载** —— 启动只读 frontmatter；命中再读 body；`@file` 引用按需。当前 YAML frontmatter 已做（v3 Quick Win 4），body 仍上来全装 | 2~3 天 | [SKILL.md pattern](https://bibek-poudel.medium.com/the-skill-md-pattern-how-to-write-ai-agent-skills-that-actually-work-72a3169dd7ee) |
| **P1-18** | **网络共享挂载** —— 封装 `net use \\server\share` + egui 输入凭据弹窗 | 2~3 天 | 救出来的文件直接传 NAS |
| **P1-19** | **聊天历史持久化** —— U 盘 JSONL 存档 + 启动时加载 + 搜索 | 4~6 天 | 当前重启全丢；用户感知极高 |
| **P1-20** | **截图 + chat 历史一键导出** PDF / Markdown 给同事 / 论坛 | 2~3 天 | egui 已支持 ViewportCommand::Screenshot |

**Sprint 3 合计**：~5~8 周

**Sprint 3 末 release v3.1**

---

## Sprint 4+：v3.x P2（按需，~3~5 周）

### UX 现代化

- **DPI 自适应**（egui `pixels_per_point`）
- **暗色 / 亮色主题切换 + Catppuccin 配色**（`catppuccin-egui` 一行集成）
- **多窗口 docking**（[egui_dock](https://crates.io/crates/egui_dock) 稳定 v0.19）
- **拖文件到输入框** = 自动 attach（egui 自带 file drop event）
- **全局快捷键**（Ctrl+Enter / Esc / Ctrl+K / F5）
- **文本选区右键复制 + Ctrl+F chat 内搜索**
- **i18n 国际化框架**（提取硬编码中文 → fluent / rust-i18n + en+zh-CN）

### 高级 AI 能力

- **OpenTelemetry GenAI tracing**（OTel JSON 文件输出，桌面端 [Jaeger v2](https://thenewstack.io/jaeger-v2-ai-observability/) 读，不上 Docker 容器）
- **Dual LLM 模式**（quarantined LLM 隔离 untrusted content；**接 web search 前必做**；[arXiv 2506.08837](https://arxiv.org/pdf/2506.08837)）
- **MCP 客户端模式**（NeuroBoot 不光当 server 也能调外部 MCP server）
- **Grammar / JSON Schema 强约束**（GBNF / response_format，待 [llama-server #11847 bug](https://github.com/ggml-org/llama.cpp/issues/11847) 解决）

### 分发 / 部署

- **GitHub Actions CI/CD** —— [actions-rust-cross](https://github.com/houseabsolute/actions-rust-cross) x64+ARM64 矩阵周期 build + 跑 cargo test + 自动 release
- **ARM64 构建** —— Snapdragon X Elite 笔记本 PE 空白市场
- **代码签名** —— [Microsoft Trusted Signing](https://textslashplain.com/2026/04/28/smart-app-control/) 廉价 + 自动滚证书；避 SAC + SmartScreen 警告
- **VHDX native boot** —— 用户没 U 盘也能用

### v2.x 残留

- **Ventoy 启动菜单分双项**（标准 vs `--readonly`）（v2 Stage 4.4）
- **本地视觉模型完整 lazy-spawn** + ISO 内置 GGUF + PE 真测 benchmark（v2 Stage 5.0~5.8）
- **Markdown 流式渲染换 mdstream**（egui_commonmark 实测卡顿时；v2 Stage 2.5）
- **NOTICE 追加 attribution**（NTPWEdit / TestDisk / smartmontools / 7-Zip / BlueScreenView）（v2 Stage 6.5）—— 触发条件是「真分发含 binary 的 ISO」

### 其他

- **MemTest86+ 启动菜单集成**（Ventoy 加菜单项）
- **ShadowExplorer** 浏览 VSS 卷影副本 portable + 工具 list_shadow_copies
- **KV cache slot save/restore 跨进程持久化**（结合 P0-3 memory）

---

## 必备的「不阻塞」配套工作（贯穿所有 Sprint）

- **docs 同步**：每个 commit 完成后 README / TODO 状态表 / KNOWN-ISSUES 同步更新（v1.0.1 / v2 期间已成习惯）
- **cargo test 不退步**：每次新功能至少补 2~3 个单测；当前 64/64
- **dumpbin 检查不退步**：每次 commit 后跑 dumpbin 验证 crt-static 仍清白
- **PS 5.1 GBK 坑**：所有 .ps1 文件改前先 grep 非 ASCII；中文注释用 BOM 或纯英文（[feedback-powershell5-ps1-encoding 教训](https://github.com/yzq1979/NeuroBoot/blob/main/docs/KNOWN-ISSUES.md#19)）
- **每次 commit 用 -F file 避 PS 多行 message parse error**（v1.0.1 教训）

---

## 关键决策点（用户需拍板）

### 决策 A：v3 整体投入模式

| 模式 | 节奏 | 风险 |
|---|---|---|
| **维持模式** | 每会话 1~2 个 P0 / P1 项，慢慢攒 | 进度可控；周期长（>3 月才出 v3.0） |
| **集中模式** | 明确 4~8 周专注 Sprint 2 + Sprint 3 整套，期间不接新需求 | 周期短（2 月出 v3.0）；要求专注 |

### 决策 B：Wi-Fi GUI 复杂度

- **简版**（NeuroBoot UI 包装 `netsh wlan show profiles / connect`）：~3 天，PE 默认 Wi-Fi 驱动支持的硬件才用
- **高级版**（集成 wpa_supplicant Windows port，PE 内核驱动级 Wi-Fi）：~2 周，覆盖 PE 默认不识别的 Wi-Fi 网卡

### 决策 C：本地 RAG 投入时机

- 工程量较大（2~3 周）
- 收益对中文用户极高（错误码字典 + BugCheck 离线查询）
- **建议进 v3.1 而非 v3.0**（先 P0 做完，确保基本盘）

### 决策 D：分发部署优先级

- ARM64 / 代码签名 / CI/CD 都是「不做后续每一步都吃亏」的工程基础
- **建议 v3.0 期间分散做完**：CI/CD ~1 天、签名 ~半天、ARM64 跨编译 ~1 天，合计 ~3 天分散到 Sprint 2

### 决策 E：外部 binary 分发策略

当前 5 个外部 binary（NTPWEdit / TestDisk / smartctl / 7za / BlueScreenView）默认不进 ISO。
**何时考虑改成默认打包**？
- 用户社区一致反馈「找 binary 麻烦」
- 个人用 / 内部 IT 用 / 开源项目分发 全部 license 合规明确
- 决定**不**做商业付费救援盘
- 触发条件：当其中至少 3 个工具的下载频次足够高 + license 风险经法务复查 OK

---

## 时间线预估

| 时间 | 里程碑 |
|---|---|
| **2026-05-末** | Sprint 1 完成：v2 + v3 + Quick Win 全部 ISO 落地 + U 盘真测反馈 |
| **2026-07-中** | **v3.0 release**（Wi-Fi + Hook + Memory + 文件管理器 + 备份恢复 + 数据恢复 GUI） |
| **2026-09-初** | **v3.1 release**（NirLauncher + Plan Mode + RAG + 12 项 P1 全部） |
| **2026-Q4** | v3.x（UX 现代化 + 高级 AI + 分发部署） |

按**集中模式**估算，**维持模式**全部 ×1.5~2 倍。

---

## 工具数演进

| 版本 | safe 工具 | dangerous 工具 | 总数 | 外部 binary 依赖 |
|---|---|---|---|---|
| v1.0 | 3 | 1 | 4 | 0 |
| v2 Stage A | 11 | 1 | 12 | 0 |
| v2 Stage 4 | 11 | 6 | 17 | 0 |
| v2 Stage 6 | 12 | 8 | 20 | 3（NTPWEdit / TestDisk / smartctl） |
| v3 Quick Wins | 15 | 7 | **22**（当前）| 5（+ 7za / BlueScreenView） |
| v3.0（Sprint 2 末）| ~20 | ~10 | **~30** | 5（同上） |
| v3.1（Sprint 3 末）| ~25 | ~12 | **~37**（含 RAG / Plan 等元工具）| 6~7（+ NirLauncher / DMDE / 等） |

---

**Last updated**: 2026-05-24
**Next review**: Sprint 1 完成（ISO 重 build + U 盘真测）后回顾 Sprint 2 P0 排序

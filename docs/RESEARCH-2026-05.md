# NeuroBoot 2026-05 调研汇总

> 5 个深度调研报告的关键结论 + 引用 URL 固化。**目的：避免下次会话重新调研**。
> 详细原始 agent 报告在 git 历史 / 本地 tasks/ 目录（临时文件，会被清理）。

**调研日期**：2026-05-24
**采集量**：~50 轮 WebSearch + ~30 篇 WebFetch，~120 个引用 URL
**适用版本**：NeuroBoot v1.0.1+（含 Stage A 完成状态：12 工具 + Markdown + 图片上传）

---

## 一、项目定位结论

NeuroBoot 处在一个 **「公开调研中没找到直接前辈」** 的组合空间：

> Windows PE × 本地 LLM 推理 × 国产中文 Agent × N 个 AI 工具 × 完整 confirmation 关卡

- **传统 PE 救援盘**（微 PE / 优启通 / Sergei Strelec 200 工具版 / Hiren's BootCD PE / MediCat 22GB / AOMEI / 大白菜老毛桃）—— **没有一家**集成 AI 助手
- **Linux 救援盘**（SystemRescue 1200+ 包）—— 工具丰富但无 AI
- **本地 AI 工具**（Microsoft Phi Silica / HP AI Companion / Dell SupportAssist）—— **全部锁 Copilot+ PC NPU 硬件** 或 **仅是 chatbot 套预定义脚本**
- **CLI Agent 类**（Open Interpreter / Goose / AI Shell）—— **微软的 AI Shell 2026-01 已 archive**
- **直接相似项目**：未找到 WinCAP / WinAutoBoot / CHIPSEC+AI 等任何「PE + AI」的公开同类

→ **差异化护城河真实存在**，叙事重点：「AI 让传统 PE 工具被普通人用上」+「中文 + 离线 + 不依赖 NPU」

---

## 二、PE 救援盘横评表（NeuroBoot 市场定位）

| 维度 | 微 PE | 优启通 | Sergei Strelec | Hiren's BootCD PE | MediCat | **NeuroBoot v1.0.1+** |
|---|---|---|---|---|---|---|
| ISO 大小 | ~500MB-1GB | ~600MB-1GB | **5-6 GB** | ~3 GB | **20-22 GB** | **2.93 GB** |
| 工具数 | ~10 | 50+ | **200+** | 50+ | 上百 | 12（AI 调度） |
| 中文原生 | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| AI 助手 | ❌ | ❌ | ❌ | ❌ | ❌ | **✅** |
| 本地 LLM | ❌ | ❌ | ❌ | ❌ | ❌ | **✅** |
| 镜像合法性 | 中（魔改）| 中（魔改）| 中（魔改）| 中（魔改）| 中 | **高（官方 ADK + 24H2 ISO）** |
| 商业可用 | ⚠ 灰色 | ⚠ VIP 销售 | ⚠ 灰色 | ⚠ 灰色 | ⚠ | **✅（Apache-2.0 + NOTICE）** |

**三项独占**：本地 LLM + 完整合规 + Apache-2.0 OSS。企业渠道核心卖点。

---

## 三、开源视觉模型选型（重要！）

### 3.1 候选模型一览表

按 NeuroBoot 适配度排序（中文 + CPU + ≤6GB RAM + Apache 2.0 硬约束）：

| 排名 | 模型 | Q4_K_M 大小 | RAM 峰值 | 中文 OCR | License | llama.cpp 支持 |
|----|-----|------------|---------|---------|---------|---------------|
| ★★★★★ | **Qwen3-VL-2B-Instruct** | 1.11 GB + mmproj 700 MB | ~3 GB | 强（32 lang OCR）| Apache 2.0 | b6887+ (PR #16780, 2025-10-30) |
| ★★★★★ | **MiniCPM-V 4.6** (1.3B) | **529 MB**（mmproj 内嵌）| ~1.5-2 GB | 强（同族 OCRBench v2 中文 58.8 开源最强）| Apache 2.0 | b6282+ (PR #15575) |
| ★★★★☆ | **Qwen3-VL-4B-Instruct** | 2.5 GB + mmproj 400 MB | ~5 GB | 极强（OCRBench ~85%）| Apache 2.0 | b6887+ |
| ★★★★☆ | Qwen2.5-VL-3B-Instruct | 1.93 GB + mmproj | ~3.5 GB | 强（29 lang）| Apache 2.0 | 稳定 |
| ★★★☆☆ | MiniCPM-V 4.5 (8B) | 5.03 GB | ~6.5 GB | OCR 开源最强 | Apache 2.0 | b6282+ |

### 3.2 已确认不适合的模型 + 排除理由

| 模型 | 出局理由 |
|---|---|
| **SmolVLM / SmolVLM2 (256M/500M/2.2B)** | **完全不支持中文** —— [HuggingFace 官方模型卡明确](https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct) "supports English only"；底语言模型 SmolLM2 是 English-only（SmolLM3 才有中文，但 SmolVLM 仍基于 SmolLM2）；HF 上 147 个 fine-tune 无中文版。**OCRBench 仅 52.6（256M）**，弱 |
| **Moondream 2** | Phi2 底，主英语训练，中文 OCR 失败 |
| **Pixtral 12B** | Q4 ~7-8 GB 超 RAM 预算；Mistral 系列中文弱 |
| **Llama 3.2 Vision 11B** | 11B 太大；Meta Community License + 7 亿 MAU 触发条款 |
| **DeepSeek-VL2** | llama.cpp 尚未原生支持 ([issue #11678](https://github.com/ggml-org/llama.cpp/issues/11678)) |
| **Phi-4-Multimodal** | 视觉部分仅支持英文；audio + vision 多 encoder mmproj 非标 |

### 3.3 NeuroBoot 落地决策

**Stage 5 主方案：Qwen3-VL-2B-Instruct (Q4_K_M)** —— 跟现 Qwen3-4B 文本模型共存，lazy spawn 第二个 llama-server。

**Stage 5.0 预研**：在主开发机用 5 张中文样本图（BIOS / BSOD / 设备管理器 / 错误对话框 / 截图）对比 MiniCPM-V 4.6 vs Qwen3-VL-2B。若 MiniCPM-V 4.6 实测中文 OCR 达标 → 改用它能省 1 GB ISO + 1.5 GB RAM；否则坚持 Qwen3-VL-2B（更成熟稳）。

---

## 四、Microsoft QMR + Phi + agentic OS 路线分析（NeuroBoot 视角）

### 4.1 QMR 真相速览

- **完全不是 LLM** —— 规则引擎 + Microsoft 后端 known-issue 知识库匹配 + 修复包下载
- **必须 WinRE，不是 WinPE** —— 跑在系统盘 `\Recovery\WindowsRE`，NeuroBoot U 盘**完全不在 QMR 工作链上**
- **必须联网**（有线 / WPA-WPA2 Wi-Fi only）
- **企业 / 域加入 Pro 默认关**（Home 默认开）
- 4 个失败模式（网络断 / Wi-Fi 非 WPA / 后端没匹配 / 企业没开）→ **每一种都是 NeuroBoot 的卖点**

### 4.2 微软自研路线对 NeuroBoot 的实际意义

| 微软方向 | NeuroBoot 应对 |
|---|---|
| **Phi Silica** | **❌ 不用** —— 官方明确「China not available」+ 锁 Copilot+ PC NPU + PE 完全跑不起来 |
| **Phi-4-Multimodal** | **❌ 不用** —— 视觉部分仅英文 |
| **Microsoft Foundry Local** | **❌ 不用** —— 依赖 VC++ Redist + WinML + Windows Update 拉 EP，PE 不兼容 |
| **QMR** | **❌ 不抄** —— 跑在 WinRE 不是 WinPE，本质规则引擎 |
| **MCP 协议**（Anthropic + Microsoft + OpenAI 共推）| **✅ 可考虑（Stage 8）** —— 开放协议非微软专属 |

### 4.3 agentic Windows 信号

- Pavan Davuluri 2025-11-10 X 帖宣称 "Windows is evolving into an agentic OS" → **1.5M+ views 严重 backlash**，关闭回复
- 用户对厂商内置 agent 警惕 → **第三方开源 + 纯本地 + 可关有信任优势**
- Yusuf Mehdi 35 年老兵离职（reportedly 与 agentic 战略调整有关）

### 4.4 应保留 vs 应追赶

| **必保留（差异化护城河）** | **必追赶（用户预期上升）** |
|---|---|
| 纯 CPU、不依赖 NPU | 视觉模型支持（Stage 5）|
| 单文件 / 离线 / 完整 U 盘 | （可选）MCP 协议（Stage 8）|
| 中文优先（Qwen3-4B 主力） | 流式输出（Stage 2）|
| Apache-2.0 全开源 | 危险工具集（Stage 4）|
| 高危操作再确认 | 救援旗舰工具（Stage 6）|
| 中国可访问 | UX 升级（Stage 7）|

---

## 五、Agent 架构关键发现

### 5.1 流式 SSE + tool_calls 跨 chunk 累积

- OpenAI SSE 规范：`tool_calls[].function.arguments` 是 **string**，跨 chunk 按 `index` 在 HashMap 拼接；完成信号 `finish_reason: "tool_calls"`
- **致命兼容性 bug**：llama.cpp build 8233+ 在 `--jinja` 模式下把 `arguments` 输出成 **JSON object** 而非 string，OpenAI Python SDK ≥2.21 直接崩 `TypeError`。**Rust 解析必须双形态都吃**。issue: [llama.cpp #20198](https://github.com/ggml-org/llama.cpp/issues/20198)
- llama.cpp 内置 **JSON healing**：流式 chunk 自动补 valid JSON

### 5.2 上下文管理

Anthropic 官方推荐：保 system + 保最近 N 个 tool_use 完整 + 老 tool_result 替换成 `[cleared, can re-call]` 占位符。**对 Qwen3-4B 小模型最稳**。不建议做摘要式 compaction（4B 自摘要质量不可控）。
参考：[Anthropic context engineering cookbook](https://platform.claude.com/cookbook/tool-use-context-engineering-context-engineering-tools)

### 5.3 dangerous 操作 UX

- **不要学 Cline** 的 LLM 自分类 `requires_approval`：Anthropic Auto Mode 仍有 17% false negative，PE 格式化整盘一次翻车就是数据全没
- **学 Aider 的「操作前自动备份」**：`delete_path` 改成 move to `X:\trash\<timestamp>\`，UI 加「清空 trash」按钮
- 参考：[Aider git docs](https://aider.chat/docs/git.html)

### 5.4 量化升级

4B 这种小模型 Q4 是**下限**，**Q5_K_M 在 tool-calling 上肉眼可见提升**。不要用 `-ctk q4_0` 量化 KV cache（llama.cpp 官方警告会显著降 tool-calling 性能）。
参考：[GGUF quantization guide](https://bmdpat.com/blog/gguf-quantization-q4-q5-q8-explained-2026)

### 5.5 PE 启动器优化

- `--no-mmap`：U 盘 (FAT32/exFAT) 可能不支持 mmap，强制 read 模式
- `-t <物理核数>`：不是逻辑核，超线程拖累矩阵运算
- 参考：[llama.cpp server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)

### 5.6 不应抄的反模式

| 反模式 | 来源项目 | 为什么 NeuroBoot 不抄 |
|---|---|---|
| MCP stdio subprocess transport | Goose / AnythingLLM | PE 进程创建成本高、IPC 复杂 |
| LLM 自分类 dangerous | Cline | 4B 模型 false negative 太高 |
| 多 agent 编排 | AutoGen / CrewAI / MetaGPT | 单用户单任务不需要；AutoGen 进 maintenance mode |
| Background agent 并行 | Cursor | PE 内存紧 + 软件渲染 OOM |
| 完整 Claude Code skill watch + 跨 session memory | Claude Code | PE 重启即 wipe，云端长会话设计在这里无意义 |

---

## 六、PE 救援盘核心工具集差距分析

### 6.1 Linux SystemRescue 默认带的工具 vs NeuroBoot 当前覆盖

| Linux 工具 | NeuroBoot 当前是否覆盖 | Stage 计划 |
|---|---|---|
| `chntpw` / `NTPWEdit`（Windows 密码重置）| ❌ | **Stage 6** |
| `testdisk` / `photorec`（分区/文件恢复）| ❌ | **Stage 6** |
| `smartctl`（详细 SMART 数据）| ❌（当前 list_disks 只看 cmdlet 表层）| **Stage 6** |
| `chkdsk` / `sfc` / `dism`（系统修复）| ❌ | **Stage 4** |
| `bootrec`（启动修复）| ❌ | **Stage 4** |
| Defender Offline (`MpCmdRun`)（离线杀毒）| ❌ | **Stage 4** |
| `ddrescue`（坏盘救援）| ❌ | Stage 7+ |
| `nmap` / `tcpdump`（网络抓包）| 部分（list_network_adapters 等 2 个）| Stage 7+ |

### 6.2 杀毒救援盘市场空白

- **ESET SysRescue** 2024-2025 EOL
- **Norton Power Eraser / Bitdefender Photon** "AI" 是营销词
- **Kaspersky Rescue Disk** 在美不可用
→ NeuroBoot 用 `MpCmdRun.exe -Scan -ScanType 2 -BootSectorScan` 接入 Defender Offline，**填真空地带**（Stage 4）

---

## 七、关键参考资料 URL 汇总（按主题）

### AI Agent 架构
- [Anthropic Claude Agent SDK Python](https://github.com/anthropics/claude-agent-sdk-python)
- [Anthropic: Writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- [Anthropic: Building agents with Claude Agent SDK](https://claude.com/blog/building-agents-with-the-claude-agent-sdk)
- [Anthropic: Claude Code Auto Mode](https://www.anthropic.com/engineering/claude-code-auto-mode)
- [Anthropic context engineering cookbook](https://platform.claude.com/cookbook/tool-use-context-engineering-context-engineering-tools)
- [Aider git docs](https://aider.chat/docs/git.html)
- [Cline auto-approve docs](https://docs.cline.bot/features/auto-approve)
- [k8sgpt repo](https://github.com/k8sgpt-ai/k8sgpt)

### llama.cpp / GGUF / 量化
- [llama.cpp function calling docs](https://github.com/ggml-org/llama.cpp/blob/master/docs/function-calling.md)
- [llama.cpp multimodal docs](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md)
- [llama.cpp tool_calls bug #20198](https://github.com/ggml-org/llama.cpp/issues/20198)
- [llama.cpp server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
- [GGUF quantization guide 2026](https://bmdpat.com/blog/gguf-quantization-q4-q5-q8-explained-2026)
- [llama.cpp Qwen3-VL PR #16780](https://github.com/ggml-org/llama.cpp/pull/16780)

### 视觉模型
- [Qwen3-VL GitHub](https://github.com/QwenLM/Qwen3-VL)
- [Qwen3-VL Technical Report (arXiv 2511.21631)](https://arxiv.org/abs/2511.21631)
- [Qwen3-VL-2B GGUF](https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct-GGUF)
- [MiniCPM-V 4.6 model card](https://huggingface.co/openbmb/MiniCPM-V-4.6)
- [MiniCPM-V 4.6 GGUF](https://huggingface.co/openbmb/MiniCPM-V-4.6-gguf)
- [OCRBench v2 leaderboard](https://99franklin.github.io/ocrbench_v2/)
- [SmolVLM-256M (NeuroBoot 排除的原因)](https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct)

### 微软自研 / QMR / agentic OS
- [Microsoft QMR 官方文档](https://learn.microsoft.com/en-us/windows/configuration/quick-machine-recovery/)
- [Windows Resiliency Initiative](https://blogs.windows.com/windowsexperience/2025/06/26/the-windows-resiliency-initiative-building-resilience-for-a-future-ready-enterprise/)
- [Foundry Local GitHub](https://github.com/microsoft/Foundry-Local)
- [Foundry Local 架构](https://learn.microsoft.com/en-us/azure/foundry-local/concepts/foundry-local-architecture)
- [Phi Silica APIs](https://learn.microsoft.com/en-us/windows/ai/apis/phi-silica)
- [Build 2025 agentic web keynote](https://blogs.microsoft.com/blog/2025/05/19/microsoft-build-2025-the-age-of-ai-agents-and-building-the-open-agentic-web/)
- [Ignite 2025 Windows updates](https://blogs.windows.com/windowsexperience/2025/11/18/ignite-2025-windows-at-the-frontier-of-work/)
- [agentic OS backlash 报道](https://www.tomshardware.com/software/windows/top-microsoft-execs-boast-about-windows-evolving-into-an-agentic-os-provokes-furious-backlash)

### 传统 PE 救援盘
- [微 PE 官方](https://www.wepe.com.cn/)
- [Sergei Strelec WinPE](https://sergeistrelec.name/winpe-10-8-sergei-strelec-english/)
- [Hiren's BootCD PE](https://www.hirensbootcd.org/)
- [MediCat USB](https://medicatusb.com/)
- [SystemRescue 包列表](https://www.system-rescue.org/Detailed-packages-list/)
- [Trinity Rescue Kit](https://trinityhome.org/)
- [Kaspersky Rescue Disk](https://www.kaspersky.com/downloads/free-rescue-disk)
- [ESET SysRescue EOL 公告](https://support-eol.eset.com/en/sysrescue_live_eol.html)
- [WinFE 2025 status](https://winfe.wordpress.com/2025/12/13/not-yet-winfe-status-update/)

### 学术 / 综述
- [Small Language Models for Agentic Systems (arXiv 2510.03847)](https://arxiv.org/pdf/2510.03847)
- [AIOS COLM 2025 (arXiv 2403.16971)](https://arxiv.org/pdf/2403.16971)
- [AIOps 'AI Oops' 警告 (arXiv 2508.06394)](https://arxiv.org/abs/2508.06394)

---

## 八、调研未覆盖 / 需自测的项

1. **CPU 推理 tok/s 数据**：所有 Qwen3-VL / MiniCPM-V 的 CPU 推理速度都是估算，需 PE 真测
2. **MiniCPM-V 4.6 中文 OCR 实测**：v4.6 是 2026-05 新发，独立第三方评测未出，需 NeuroBoot 自测
3. **Qwen3-VL CPU 长 prompt OOM 风险**：PR #16780 标注 deepstack 内存优化是 TODO，需 8 GB 机型 worst-case 测试
4. **Foundry Local 在 PE 的 cold-start 可行性**：未实测
5. **MCP Rust SDK** (`rmcp` / `mcp-rust-sdk`) 成熟度需评估
6. **WinCAP / WinAutoBoot / CHIPSEC+AI** 等"PE + AI"类似项目 —— 确认无直接前辈

---

**Last updated**: 2026-05-24
**Next research review**: v2 Stage 5 完成（本地视觉模型上线）后重新评估 VL 模型 landscape

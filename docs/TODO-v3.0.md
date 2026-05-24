# NeuroBoot v3.0 开发进度 + 剩余计划

> **Last updated**: 2026-05-25（W8 Eval 完成）
> **Status**: 8/9 模块代码完成（剩 W8.5 真测 + W5-6 Phase 3 操作），21 commits 领先 origin/main（**未 push**），ISO 待重 build
> **Resume hint**: 下次开工先 `git log --oneline -20` 看 commit 历史 + 读本文档复习

---

## 📌 v3.0 主题（2026-05-25 重新校准）

原 v3 路线图（[TODO-v3.md](TODO-v3.md)）的 Sprint 2 聚焦「补功能」（文件管理器 / Wi-Fi
GUI / wbadmin 等）。**2026-05-25 经多轮调研重新校准**，转向：

> **让 AI 把现有 22 工具用得更准 + 把 7 个高频用户痛点做成一站式排错流程**

**核心 insight**：用户痛点不是「PE 缺什么工具」，而是「面对 Windows 问题不知道下一步
怎么走」。差异化护城河 = **AI 引导排错流程**，不是给一堆工具让用户自己挑。

**调研驱动**：
- Anthropic 2026 best practices（progressive disclosure、tool description ROI、Skill 标准、Plan Mode）
- 用户高频痛点 Tier 排序（Tier-S：蓝屏 / BitLocker 恢复 / 启动失败；Tier-A：数据救援 / 密码 / 慢 / Wi-Fi）
- 调研详见 plan 文件 `C:\Users\yzq19\.claude\plans\v3-todo-web-temporal-rabbit.md`

**砍掉 / 推 v3.1+**：
- 文件管理器 GUI（用户痛点不是缺 GUI，是不知道看哪里；`find_large_files` + AI 更直接）
- Wi-Fi 连接 GUI（有线 / 手机 USB tethering 多数场景可用；保留 `/diagnose-wifi` skill）
- wbadmin 3 工具（让 v3.0 聚焦 agent 能力，备份功能 v3.1 一起做）
- Computer Use（Anthropic 2026-03 确认仍 macOS only）
- Multi-agent / subagents（PE 内存紧）

---

## ✅ 已完成（8/9 模块代码，21 commits）

| W | 模块 | Commits | 关键改动 |
|---|---|---|---|
| **W1** | Tool description 全面重写 | `296ad93` → `036b323`（6 commits） | 22 工具按 Anthropic 2026 best practices 重写 name / description / parameters / return-doc；加 `assert_v30_description_convention()` helper 到 registry.rs（cfg test only）；每个工具 1 单测保 4 必需 section（When to use / Parameters / Returns / Notes）+ 长度区间 [200, 1500] + name snake_case |
| **W1.5** | Skill Progressive Disclosure | `d9ab143` | Anthropic 2025-12 开放标准（OpenAI/Google/GitHub/Cursor 几周内全部接入，60-80% token 节省）。3 tier：Tier 1（启动加载所有 SkillSummary，~80 tokens/skill）+ Tier 2（AI 调 `load_skill(name)` 工具按需 fetch body）+ Tier 3（@reference.md 引用，v3.1 用 `load_skill_reference` 工具）。新工具 `load_skill`，SkillBody / SkillSummary 分两个 struct |
| **W2-3** | 5 新核心诊断 skill + USB 模板 | `9f5a51f` | 新 5 skill：`/recover-bitlocker`（KB 触发恢复键循环 + 5 步阶梯）、`/fix-boot-failure`（6 tier escalation）、`/reset-password`（账户类型 triage + EFS/DPAPI 警告）、`/diagnose-slow`（6 嫌疑层 + 反模式列表）、`/recover-data`（"立刻停写源盘" + 4 case 分流 + target≠source 防护）。 加 04-add-payload.ps1 [2.7/5] 段拷 skills 到 PE。集成测试 `distributed_skill_templates_all_parse` 验证 8/8 skill 模板能 parse |
| **W3-4** | Plan Mode（Cline 风格）| `018d684` | 新 `propose_plan` 工具 + agent loop 拦截 + UI 双向 mpsc 模态（复用 ConfirmationRequest 模式）。批准 / 拒绝合成 tool result 回灌 LLM。UI 模态 2 种样式：含 dangerous → 红边框；纯 safe → 蓝边框。System prompt 加 Plan Mode 段教 AI 何时调（>2 工具 OR 含 dangerous OR 用户显式要求 OR load_skill 后） |
| **W7** | 5 新 safe 工具 | `8718fa3` | `list_winre_status`（reagentc /info + bcdedit /enum {default}）、`bitlocker_status`（manage-bde -status + Secure Boot）、`find_large_files`（Get-ChildItem 大小过滤排序）、`read_recent_installs`（QuickFixEngineering KB 时间线）、`lookup_error_code`（hardcoded ~25 高频 BugCheck + Win32/HRESULT，W5-6 RAG 落地后内核升级 API 不变）。bug 抓 + 修：`normalize_code()` 第一版把 "BugCheck" 里的 'B' 误当裸 hex token，改为优先全字符串搜 `0X` 前缀 |
| **W6-7** | Hooks 简化版 + Persistent Memory | `bd574ce` | 2 新模块 + 1 新 safe 工具：`app/src/hooks/` 4 触发点（SessionStart / PreToolUse / PostToolUse / Stop）、handler 仅 type=command（PS -Command，默认 10s 超时）、PreToolUse 非零 exit = 拒绝工具调用并合成 "（hook 拒绝）..." tool result；`app/src/memory/` 6 命令 view/create/str_replace/insert/delete/rename + path traversal guard（拒绝绝对路径 / `..` / UNC / NUL byte 等）；`app/src/tools/safe/memory.rs` 单工具 command 派发（对齐 Anthropic 官方 Memory Tool 命名）；启动时 hooks::run_session_start + memory::load_memory_md 自动注入 system prompt；system prompt 加教学段（何时主动调 memory write / read，何时 view）。Templates 在 `docs/usb-templates/NeuroBoot.hooks.json` + `memories/MEMORY.md`，**per-USB 配置不进 ISO** |
| **W5-6 P1** | RAG skeleton（FTS5 trigram） | `0903a9c` | 新模块 `app/src/rag/` + Python build script + 49 条 fixture：`tools-dev/build-error-db.py` 纯 stdlib，`--fixtures-only --no-embed` 产出 160 KB sqlite db（entries 表 + FTS5 trigram virtual table + 同步触发器 + db_meta）；`tools-dev/fixtures/error-fixtures.json` 涵盖原 hardcoded 24 条 + 25 条新增（NTSTATUS / 工具描述 / 多 BSOD）；`app/src/rag/mod.rs` RagClient 两层查询（exact code = 1.0 / FTS5 bm25 排名）—— CJK 原生支持（trigram = 3 字符滚动窗口）；`app/src/tools/safe/lookup_error_code.rs` 加 RagClient::discover() preamble，未命中回退 hardcoded（输出含 `source: "rag" \| "hardcoded"`）|
| **W5-6 P2** | sqlite-vec 混合检索 + 爬虫 + ISO 打包 | `8eca9b8` | **代码完成**（操作步骤见下文 "Phase 3 operational" 节）：Cargo add `sqlite-vec = "0.1.9"`（静态链接 +160 KB）；`app/src/rag/vec_ext.rs` OnceLock 注册 sqlite_vec_init；`RagClient::open` 检测 entries_vec + has_embeddings；新 `lookup_hybrid`（FTS5+exact+vec）/`lookup_auto`（自动 embed→hybrid）/ `embed_query`（reqwest blocking POST /v1/embeddings）+ RRF k=60 融合；Python 端 entries_vec 按需创建（首次嵌入回报 dim）+ `--crawl` 分支爬 Microsoft Learn（BugCheck/Win32/NTSTATUS，可选 bs4 依赖，0.5s/req polite）；`04-add-payload.ps1` 新 [3.5/5] 段拷 errordb + Embedding GGUF 到 PE，+ 写 `start-llama-embed.cmd` :8081 第二 llama-server，+ `startnet.cmd` 条件启动 embed 服务（missing 文件全部 graceful degrade） |
| **W8** | Eval 框架 + 30 golden prompts | `e0079c2` | Cargo feature `eval` 网关（serde_yaml + regex 仅 eval 时拉）；`app/src/eval/` ~700 行 Rust：`spec.rs`（YAML schema + 解析）/ `matchers.rs`（must_call_tools / must_not_call / regex must_match / must_not_match / max_seconds / max_tokens / allowed_tools 白名单 hallucination 检测）/ `runner.rs`（无 GUI 驱动 spawn_agent_request + 收集 ExecutionStats + auto-reject Confirmation + auto-approve PlanProposal + 一次性 /health probe 全套 30 case 2s 跑完）/ `mod.rs`（EvalReport 聚合 + render 渲染 + run_cases_dir 入口）。`app/tests/eval-fixtures/` 30 个 YAML：8 skill 触发 + 8 free-form 单工具 + 5 多步（>=3 工具）+ 5 dangerous 守护 + 4 edge case（greeting / identity / ambiguous / long）。集成测试 `eval::integration::{eval_fixtures_all_parse, run_golden_prompts}` —— 前者纯解析无需 LLM，后者一次 health-probe 失败则全 Skip 不 fail（CI 友好）。默认 report-only，`NEUROBOOT_EVAL_STRICT=1` 才让行为差异 fail 测试 |

**辅助 commits**（Sprint 1 收尾 + 跨模块基础设施修复）：
- `2500b76` `fix(tools)`: download-external-tools.ps1 修 4 处 bug（NTPWEdit URL / smartmontools NSIS / TLS 1.2 / TestDisk path）
- `c010cd6` `fix(pe-build)`: 04-add-payload force exit 0 防 `$LASTEXITCODE` 跨 `&` scope 不传播误判
- `55672cc` `fix(gitignore)`: 锚定 `/tools/` 防 `app/src/tools/` 被误匹配忽略

---

## ⏳ 剩余（1/9 模块代码 + 一组操作步骤）

### W5-6 Phase 3 (operational) — 实际生成 embed 数据 + 重 build ISO

**代码全部 Phase 2 完成**（commit `8eca9b8`），剩下都是用户在本机跑的"操作步骤"。**这些不写代码，只跑命令**：

| 步骤 | 命令 | 工程量 |
|---|---|---|
| 1. 下载嵌入模型 | 见下方代码块 | ~10 min（取决于网速） |
| 2. 装 BeautifulSoup（如要爬全量）| `pip install -i https://pypi.tuna.tsinghua.edu.cn/simple beautifulsoup4` | ~1 min |
| 3. 启动 embed 服务 | 见下方代码块 | 启动几秒 |
| 4. 生成 fixture + embed db | `python build-error-db.py --fixtures-only --embedding-url http://127.0.0.1:8081 -o ...errordb.sqlite` | 49 条 × 100 ms ≈ 5 秒 |
| 5.（可选）全量 crawl + embed | `python build-error-db.py --crawl --with-fixtures --embedding-url http://127.0.0.1:8081 -o ...errordb.sqlite` | ~30 min crawl + ~30 min embed |
| 6. 重 build ISO | admin PS 跑 `99-build-all.ps1` | 3-10 min（自动拷 db + GGUF） |
| 7. U 盘真测 | 拷 NeuroBoot.iso 到 Ventoy，PE 启动 | 30 min |

**步骤 1：下载嵌入模型**

```powershell
# hf-mirror.com（国内推荐）—— Qwen3-Embedding-0.6B GGUF Q8_0
$url = 'https://hf-mirror.com/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/main/Qwen3-Embedding-0.6B-Q8_0.gguf'
$dst = 'C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf'
Invoke-WebRequest -Uri $url -OutFile $dst
(Get-Item $dst).Length / 1MB  # 应该 ~400-650 MB
```

**步骤 3：启动 embed 服务（本机开发用 :8081）**

```powershell
# 一个独立 cmd 窗口跑这个，留着别关
cd C:\NeuroBoot\tools-dev\llama-cpp\b9294
.\llama-server.exe -m C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf `
                   -a qwen3-embedding-0.6b `
                   --host 127.0.0.1 --port 8081 `
                   --embedding --pooling last `
                   -c 8192 -ngl 0 -t 2 --no-mmap
# 启好后 curl http://127.0.0.1:8081/health 应该返回 {"status":"ok"}
```

**Phase 3 验证**（embed 生成后）：

```powershell
# 看 db 里 entries_vec 表是否生成
python -c "
import sqlite3
c = sqlite3.connect('C:/NeuroBoot/tools-dev/fixtures/errordb.sqlite')
print('has_embeddings:', list(c.execute(\"SELECT value FROM db_meta WHERE key='has_embeddings'\"))[0])
print('entries_vec rows:', list(c.execute('SELECT COUNT(*) FROM entries_vec'))[0])
print('embedding_dim:', list(c.execute(\"SELECT value FROM db_meta WHERE key='embedding_dim'\"))[0])
"
# 然后 cargo test rag::tests::open_phase2_db_reports_has_embeddings 应该用真 db 也跑过
```

**ISO 体积影响**：fixture-only embed db ~250 KB；全量 17k embed db ~150 MB；Qwen3-Embedding GGUF ~400 MB。**当前预估 ISO 从 3.32 → ~3.9 GB**（全量），或 ~3.55 GB（fixture-only embed）。

---

### ~~W6-7 — Hook 简化版 + Persistent Memory~~ ✅ 完成（commit `bd574ce`）

详见上面已完成表 W6-7 行 + 实际产物：

- `app/src/hooks/mod.rs`：4 触发点（SessionStart / PreToolUse / PostToolUse / Stop），
  仅 `type: command`，PS 包装，10s 超时 clamped [1, 60]
- `app/src/memory/mod.rs`：6 命令 + 8 path traversal guard 单测（绝对路径 / `..` / UNC /
  NUL byte 等都覆盖）
- `app/src/tools/safe/memory.rs`：单工具 + command 派发（对齐 Anthropic 官方）
- `app/src/main.rs`：启动 scan_hooks_config + run_session_start + load_memory_md 拼到
  system_prompt；hooks_config 通过 AgentJob 传给 worker
- `app/src/agent/mod.rs`：PreToolUse 拦截在 dangerous 弹窗前；PostToolUse 跑完仅记日志；
  Stop 在 send Done 前；新 `finish_turn()` helper 取代 5 处裸 send Done
- 模板：`docs/usb-templates/NeuroBoot.hooks.json` + `docs/usb-templates/memories/MEMORY.md`
  （per-USB 配置不进 ISO；用户拷到自己 U 盘）
- 单测：116 → 152（+36：11 hooks + 22 memory + 3 memory tool 入口）；dumpbin clean；
  neuroboot.exe 12.54 → 12.66 MB（+120 KB）

---

### ~~W8 — Eval 框架 + 30 golden prompts~~ ✅ 完成（commit `e0079c2`）

详见上面已完成表 W8 行。**下次会话用法**：

```powershell
# 离线模式（仅验证 30 个 YAML 解析）
cargo test --manifest-path C:\NeuroBoot\app\Cargo.toml --features eval eval_fixtures_all_parse

# 真实跑（需要 llama-server 在 127.0.0.1:8080 跑 Qwen3-4B-Instruct）
cargo test --manifest-path C:\NeuroBoot\app\Cargo.toml --features eval run_golden_prompts -- --nocapture

# 严格模式（行为差异 = fail）
$env:NEUROBOOT_EVAL_STRICT="1"; cargo test ... run_golden_prompts -- --nocapture
```

---

### W8.5 — 集成测试 + ISO 重 build + U 盘真测（~1 周，release v3.0）

- 全 cargo test（W8.5 完成时预计 ≥ 130 个；当前 116）
- dumpbin 验证 crt-static 仍清白
- 跑 `99-build-all.ps1`（Phase 4 LASTEXITCODE bug 已修，应一次过）
- 新 ISO 预计 ~3.9 GB（+550 MB RAG GGUF + sqlite）
- U 盘真测大菜单：
  - 原 v3 plan 的 6 项（冷启动 / prompt cache TTFT / 22 工具 / 快捷按钮 / skill / 电源）
  - 加：W2-3 5 新 skill 各触发一次 + W3-4 Plan Mode 触发 + W6-7 Memory 跨重启 + W5-6 错误码查询 + W7 5 新工具
- 真测发现 bug → `docs/TODO-v3.0.1-fixes.md`

---

## 📊 关键数字演进

| 指标 | v1.0.1（起点）| 当前（W8 完成）| v3.0 末预期 |
|---|---|---|---|
| 单测 | 64 | **210**（+146；W5-6 +36 + W8 +21）| ≥ 210（稳定）|
| 工具数 | 22 | **30**（+5 W7 + 3 元工具 load_skill / propose_plan / memory = +8 实质，原 22 不变）| ~31 |
| Skill 数 | 3 | **8**（+5 W2-3）| 8（W2-3 后稳定）|
| neuroboot.exe | 12.46 MB | **14.42 MB**（+1.96 MB；rusqlite bundled +1.6 MB + sqlite-vec +160 KB；eval 是 feature-gated 不进 release）| ~14.5 MB（稳定）|
| ISO 体积 | 2.93 GB | 3.32 GB（Sprint 1.2 已 build，**未含本会话 21 commits**）| ~3.9 GB（含 RAG GGUF + 全量 db）|
| Golden prompts | 0 | **30**（W8 ship；8 skill + 8 free + 5 multi + 5 dangerous + 4 edge）| 30+ |
| dumpbin crt-static | ✅ clean | ✅ clean | ✅ clean（每 commit 验证）|

---

## 🛠 下次开工 checklist

### 0. 复习上下文

```powershell
# 看 13 commits 全貌
git -C C:\NeuroBoot log --oneline -n 20

# 看任务列表（如果在 Claude Code 里）
# 任务 ID: #14 (W5-6) / #15 (W6-7) / #17 (W8) / #18 (W8.5)
```

读本文档 + plan 文件 `C:\Users\yzq19\.claude\plans\v3-todo-web-temporal-rabbit.md`。

### 1. 选下一个 W

**推荐顺序**：W5-6 Phase 3（操作步骤）→ W8.5（真测发布）

理由：
- **W5-6 Phase 3 (~1-2 小时操作 + ~30 min build)**：不写代码，跑命令。详见上文
  「W5-6 Phase 3 (operational)」节的 7 步清单
- **W8.5 真测**：重 build ISO 后拷到 Ventoy U 盘进 PE，跑 30 个 golden prompts 当真测
  baseline，发现的 bug 进 `docs/TODO-v3.0.1-fixes.md`
- 这两步可以**串行**：Phase 3 完成后 ISO 即包含 RAG db + embedding model + 30 prompts
  框架（eval feature 仅开发用，不进 ISO），W8.5 实测能跑完整 RAG vec hybrid 路径

### 2. push 决策

21 commits 都在本地。如果你信任本机不丢，可以等 v3.0 完全 release 再 push。
如果担心硬件 / 磁盘问题，先 `git push` 备份到远端：

```powershell
git -C C:\NeuroBoot push origin main
```

### 3. ISO 重 build 决策

当前 ISO `C:\NeuroBoot\pe-build\output\NeuroBoot.iso` 3.32 GB 是 **Sprint 1.2 的产物**，
**不含**本会话 21 commits 的改动（W1 / W1.5 / W2-3 / W3-4 / W7 / W6-7 / W5-6 P1+P2 / W8）。

- **如果想现在就真测当前进度**：admin PS 跑 `pe-build\build-scripts\99-build-all.ps1` 重 build
  （应 3-10 分钟一次过，Phase 4 LASTEXITCODE bug 已修）
- **如果等 v3.0 完成统一真测**：跳过，等 W8.5

---

## ⚠️ 跨会话关键约束（沿用 v1.0.1 / Sprint 1 教训）

- 每次 commit 后跑 dumpbin → crt-static 必须清白
- 所有 `.ps1` 文件保持**纯英文**（PS 5.1 GBK 坑，[KNOWN-ISSUES #19](KNOWN-ISSUES.md)）
- 每次 `git commit -m` 用 `-F file` 避 PS 多行 message parse error
- PS 子脚本通过 `& script.ps1` 调用时 **`$LASTEXITCODE = 0` 不传播到 caller**
  （Sprint 1 教训，commit `c010cd6` 修复 04-add-payload；其他 phase 脚本如果加了 native exe
  调用也要警惕）
- 高危操作（DISM mount / 写 U 盘）每次单独再确认盘符
- 阶段切换处停下等用户确认
- **新增**：每个新工具 / skill commit 前确认 `cargo test` 全过 + 单测覆盖
  description 约定（W1 helper 自动检查）

---

## 📎 v3.0 内部架构 cheat sheet

### Tool 系统

`app/src/tools/registry.rs::Tool` trait 不变。新增 cfg(test) helper
`assert_v30_description_convention()` 让每个工具 1 行单测就能保描述格式：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&MyTool);
    }
}
```

Description 必含 4 个 marker（`**When to use**` / `**Parameters**` / `**Returns**` / `**Notes**`）+
长度 [200, 1500] + name snake_case + verb_object 形式。

### Skill 系统（Progressive Disclosure）

`app/src/ui/skills.rs`：
- `SkillSummary { name, description, source_path }` — 启动加载所有
- `SkillBody { ..., body }` — 按需加载
- `scan_skills() -> Vec<SkillSummary>` — 启动调用
- `load_skill_body(name) -> Option<SkillBody>` — AI 通过 `load_skill` 工具触发 OR 用户手动激活

System prompt 始终列所有 SkillSummary（~80 tokens/skill）；AI 调 `load_skill(name)` 拿 body
作为 tool_result 进入下轮 context。

### Plan Mode（Cline 风格）

`app/src/tools/safe/propose_plan.rs` — AI 调此工具触发审批流。
`app/src/agent/mod.rs` 在 tool dispatch 处特判 `tool_name == "propose_plan"`，
发 `AgentEvent::PlanProposal(req)` 给 UI，阻塞等 `PlanResponse`（Approve / Reject）。
合成 tool result：
- Approve → `"（用户已批准 plan）请按 steps 依次执行..."`
- Reject → `"（用户拒绝了 plan）请重新规划..."`

UI `draw_plan_dialog()` 模态展示 steps（dangerous 红色 + ⚠ 标）。

---

**Last updated**: 2026-05-25（W8 Eval 完成）
**Resume hint**: 下次开工先 `git log --oneline -20` 看 commit 历史 + 读本文档 + plan 文件。
**剩 8/9 代码完成，仅剩 W8.5 真测发布**。建议下次会话：① W5-6 Phase 3 操作（下载嵌入模型 +
build ISO，~1-2 小时）② W8.5 真测（拷 ISO 到 U 盘 PE 跑 → 收 bug → release v3.0）。

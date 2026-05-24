# NeuroBoot v3.0 开发进度 + 剩余计划

> **Last updated**: 2026-05-25（W5-6 Phase 1 完成）
> **Status**: 6.5/9 模块完成（W5-6 拆 Phase 1+2），17 commits 领先 origin/main（**未 push**），ISO 待重 build
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

## ✅ 已完成（6.5/9 模块，17 commits）

| W | 模块 | Commits | 关键改动 |
|---|---|---|---|
| **W1** | Tool description 全面重写 | `296ad93` → `036b323`（6 commits） | 22 工具按 Anthropic 2026 best practices 重写 name / description / parameters / return-doc；加 `assert_v30_description_convention()` helper 到 registry.rs（cfg test only）；每个工具 1 单测保 4 必需 section（When to use / Parameters / Returns / Notes）+ 长度区间 [200, 1500] + name snake_case |
| **W1.5** | Skill Progressive Disclosure | `d9ab143` | Anthropic 2025-12 开放标准（OpenAI/Google/GitHub/Cursor 几周内全部接入，60-80% token 节省）。3 tier：Tier 1（启动加载所有 SkillSummary，~80 tokens/skill）+ Tier 2（AI 调 `load_skill(name)` 工具按需 fetch body）+ Tier 3（@reference.md 引用，v3.1 用 `load_skill_reference` 工具）。新工具 `load_skill`，SkillBody / SkillSummary 分两个 struct |
| **W2-3** | 5 新核心诊断 skill + USB 模板 | `9f5a51f` | 新 5 skill：`/recover-bitlocker`（KB 触发恢复键循环 + 5 步阶梯）、`/fix-boot-failure`（6 tier escalation）、`/reset-password`（账户类型 triage + EFS/DPAPI 警告）、`/diagnose-slow`（6 嫌疑层 + 反模式列表）、`/recover-data`（"立刻停写源盘" + 4 case 分流 + target≠source 防护）。 加 04-add-payload.ps1 [2.7/5] 段拷 skills 到 PE。集成测试 `distributed_skill_templates_all_parse` 验证 8/8 skill 模板能 parse |
| **W3-4** | Plan Mode（Cline 风格）| `018d684` | 新 `propose_plan` 工具 + agent loop 拦截 + UI 双向 mpsc 模态（复用 ConfirmationRequest 模式）。批准 / 拒绝合成 tool result 回灌 LLM。UI 模态 2 种样式：含 dangerous → 红边框；纯 safe → 蓝边框。System prompt 加 Plan Mode 段教 AI 何时调（>2 工具 OR 含 dangerous OR 用户显式要求 OR load_skill 后） |
| **W7** | 5 新 safe 工具 | `8718fa3` | `list_winre_status`（reagentc /info + bcdedit /enum {default}）、`bitlocker_status`（manage-bde -status + Secure Boot）、`find_large_files`（Get-ChildItem 大小过滤排序）、`read_recent_installs`（QuickFixEngineering KB 时间线）、`lookup_error_code`（hardcoded ~25 高频 BugCheck + Win32/HRESULT，W5-6 RAG 落地后内核升级 API 不变）。bug 抓 + 修：`normalize_code()` 第一版把 "BugCheck" 里的 'B' 误当裸 hex token，改为优先全字符串搜 `0X` 前缀 |
| **W6-7** | Hooks 简化版 + Persistent Memory | `bd574ce` | 2 新模块 + 1 新 safe 工具：`app/src/hooks/` 4 触发点（SessionStart / PreToolUse / PostToolUse / Stop）、handler 仅 type=command（PS -Command，默认 10s 超时）、PreToolUse 非零 exit = 拒绝工具调用并合成 "（hook 拒绝）..." tool result；`app/src/memory/` 6 命令 view/create/str_replace/insert/delete/rename + path traversal guard（拒绝绝对路径 / `..` / UNC / NUL byte 等）；`app/src/tools/safe/memory.rs` 单工具 command 派发（对齐 Anthropic 官方 Memory Tool 命名）；启动时 hooks::run_session_start + memory::load_memory_md 自动注入 system prompt；system prompt 加教学段（何时主动调 memory write / read，何时 view）。Templates 在 `docs/usb-templates/NeuroBoot.hooks.json` + `memories/MEMORY.md`，**per-USB 配置不进 ISO** |
| **W5-6 P1** | RAG skeleton（FTS5 trigram） | `0903a9c` | 新模块 `app/src/rag/` + Python build script + 49 条 fixture：`tools-dev/build-error-db.py` 纯 stdlib，`--fixtures-only --no-embed` 产出 160 KB sqlite db（entries 表 + FTS5 trigram virtual table + 同步触发器 + db_meta）；`tools-dev/fixtures/error-fixtures.json` 涵盖原 hardcoded 24 条 + 25 条新增（NTSTATUS / 工具描述 / 多 BSOD）；`app/src/rag/mod.rs` RagClient 两层查询（exact code = 1.0 / FTS5 bm25 排名）—— CJK 原生支持（trigram = 3 字符滚动窗口）；`app/src/tools/safe/lookup_error_code.rs` 加 RagClient::discover() preamble，未命中回退 hardcoded（输出含 `source: "rag" \| "hardcoded"`）。**Phase 2 = 嵌入模型 + sqlite-vec + Microsoft 文档全量爬虫**（下次会话，详见下文「W5-6 Phase 2」节）|

**辅助 commits**（Sprint 1 收尾 + 跨模块基础设施修复）：
- `2500b76` `fix(tools)`: download-external-tools.ps1 修 4 处 bug（NTPWEdit URL / smartmontools NSIS / TLS 1.2 / TestDisk path）
- `c010cd6` `fix(pe-build)`: 04-add-payload force exit 0 防 `$LASTEXITCODE` 跨 `&` scope 不传播误判
- `55672cc` `fix(gitignore)`: 锚定 `/tools/` 防 `app/src/tools/` 被误匹配忽略

---

## ⏳ 剩余（2.5/9 模块，~2-3 周工作量）

### W5-6 Phase 2 — Local RAG 嵌入 + 全量数据（~10-14 天）

**Phase 1 已完成**（commit `0903a9c`）：FTS5 trigram skeleton + 49 条 fixture + lookup_error_code RAG fallback。详见上面已完成表 W5-6 P1 行。

**Phase 2 剩余工作**：

| 子任务 | 工程量 | 备注 |
|---|---|---|
| 下载 Qwen3-Embedding-0.6B Q8_0 GGUF | ~10 min | hf-mirror.com 优先；放 `C:\NeuroBoot\models\` |
| 写 build-error-db.py `--crawl` 分支 | ~3 天 | 装 venv 加 `requests + beautifulsoup4`；抓 BugCheck 512 + Win32 17k；merge fixture entries |
| llama-server 启动参数加 `--embedding` | ~1 天 | 04-add-payload.ps1 改 start-llama-server.cmd；多 ~600 MB RAM 占用 |
| build-error-db.py 跑全量 embed | ~2-4 小时 | 17k 条 × ~100 ms/chunk CPU 嵌入 |
| Cargo add `sqlite-vec = "0.1.9"` + 加载 ext | ~半天 | `rusqlite::ffi::sqlite3_auto_extension` 单次注册（参考官方 lib.rs 测试） |
| app/src/rag/mod.rs 加向量分支 | ~2 天 | `CREATE VIRTUAL TABLE entries_vec USING vec0(embedding float[1024])`；KNN top-K ∪ FTS5 top-K → rerank → 返回 top-N |
| `pe-build/build-scripts/04-add-payload.ps1` 加 [3.5/5] 段拷 db + GGUF | ~半天 | 默认拷 `C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf` + `C:\NeuroBoot\tools-dev\fixtures\errordb.sqlite`（embed 版） |
| 单测扩展 + 真测 | ~2 天 | Phase 2 db 通过 `populate_embeddings` 跑过的也要测 vec0 表存在；运行时 vec search → fts5 fallback 健壮性 |

**ISO 体积影响**：+400 MB（embedding GGUF）+ ~150 MB（errordb.sqlite 全量含 1024-dim float×17k）= **+550 MB → 当前预估 ISO 从 3.32 → ~3.9 GB**

**前置准备（下次会话开工前）**：
1. **下载嵌入模型**：到 `C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf`
   - hf-mirror：`https://hf-mirror.com/Qwen/Qwen3-Embedding-0.6B-GGUF`（推荐）
   - 验证：`(Get-Item ...gguf).Length / 1MB` 应该 ~400
2. **Python venv**（如果开始 crawl 分支）：
   ```powershell
   cd C:\NeuroBoot\tools-dev
   python -m venv .venv
   .venv\Scripts\Activate.ps1
   pip install -i https://pypi.tuna.tsinghua.edu.cn/simple requests beautifulsoup4
   ```
3. **先跑当前 Phase 1 build**确认基线：
   ```powershell
   python C:\NeuroBoot\tools-dev\build-error-db.py --fixtures-only --no-embed -o C:\NeuroBoot\tools-dev\fixtures\errordb.sqlite
   ```

**Cargo deps to add (Phase 2)**：
- `sqlite-vec = "0.1.9"`（crates.io；FFI wrapper 自动 link 静态 lib）

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

### W8 — Eval 框架 + 30 golden prompts（~5-7 天，中型块）

**为什么 P0**：模型 / prompt / skill 改完不黑盒。Anthropic 明确 "evals give baselines and
regression tests for free; track latency / token usage / cost / error rates"。当 Qwen3.5 / Qwen3.6
出来能快速验证是否能升级。

**工作**：
- 30 个 golden prompt 覆盖：
  - 8 个 skill 各触发一次（8）
  - free-form 问题诊断（8）：「我的硬盘有几块」/「电脑慢」/「0x0000007B 是什么」等
  - 多步问题（5）：要求 2+ 工具调用
  - dangerous 触发（5）：必须进 Plan Mode 才能执行
  - 边缘场景（4）：歧义问题、空输入、超长输入
- 每个 prompt 期望：
  - 必须调用的 tool 集合（不要求顺序，但子集匹配）
  - 必须出现的关键词（中文 + 英文，正则）
  - 不应出现的（hallucination 检测：编造的工具名 / 不存在的盘符）
  - response 时间 / token 数（性能基线）
- Rust ~300 行 runner（**不**上 promptfoo / DeepEval / Anthropic Evaluations 重型框架）
- 集成到 `cargo test --features eval`，CI 跑过基线（不强制阻塞，只报告偏差）

**关键文件**：
- `app/src/eval/mod.rs`（新建）
- `tests/eval/*.yaml` 30 个 golden prompt
- `app/Cargo.toml` 新增 `eval` feature

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

| 指标 | v1.0.1（起点）| 当前（W5-6 P1 完成）| v3.0 末预期 |
|---|---|---|---|
| 单测 | 64 | **172**（+108）| ≥ 190 |
| 工具数 | 22 | **30**（+5 W7 + 3 元工具 load_skill / propose_plan / memory = +8 实质，原 22 不变）| ~31 |
| Skill 数 | 3 | **8**（+5 W2-3）| 8（W2-3 后稳定）|
| neuroboot.exe | 12.46 MB | **14.26 MB**（+1.8 MB 累计；W5-6 P1 rusqlite bundled +1.6 MB 是大头）| ~14.5 MB（W5-6 P2 + sqlite-vec ~200 KB） |
| ISO 体积 | 2.93 GB | 3.32 GB（Sprint 1.2 已 build，**未含本会话 17 commits**）| ~3.9 GB（含 RAG GGUF + db）|
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

**推荐顺序**：W5-6 Phase 2 → W8 → W8.5

理由：
- **W5-6 Phase 2 (~10-14 天)** 直接续 Phase 1：嵌入模型 + 全量 17k 爬虫 + sqlite-vec
  向量分支。下次会话先 `Get-Item C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf`
  确认嵌入模型已下载（见上文 W5-6 Phase 2 节的「前置准备」）
- **W8 Eval** 在 W5-6 P2 完成后跑最有价值（golden prompts 能测 RAG + memory + hooks 全链路）
- **W8.5 真测** 必须在前 2 个都完成后

### 2. push 决策

17 commits 都在本地。如果你信任本机不丢，可以等 v3.0 完全 release 再 push。
如果担心硬件 / 磁盘问题，先 `git push` 备份到远端：

```powershell
git -C C:\NeuroBoot push origin main
```

### 3. ISO 重 build 决策

当前 ISO `C:\NeuroBoot\pe-build\output\NeuroBoot.iso` 3.32 GB 是 **Sprint 1.2 的产物**，
**不含**本会话 17 commits 的改动（W1 / W1.5 / W2-3 / W3-4 / W7 / W6-7 / W5-6 P1）。

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

**Last updated**: 2026-05-25（W5-6 Phase 1 完成）
**Resume hint**: 下次开工先 `git log --oneline -20` 看 commit 历史 + 读本文档 + plan 文件；
W5-6 Phase 2 开工前先确认 `Get-Item C:\NeuroBoot\models\Qwen3-Embedding-0.6B-Q8_0.gguf`
已下载（hf-mirror.com 优先）

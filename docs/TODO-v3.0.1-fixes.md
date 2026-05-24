# NeuroBoot v3.0.1 — U 盘真测反馈紧急修复清单（模板）

> **本文件在 W8.5 真测开始时为空**。按 [W8.5-real-test-checklist.md](W8.5-real-test-checklist.md)
> 跑测试时，把发现的每个 bug 填进下面的「待修 bugs」表。
>
> 参考 [TODO-v1.0.1-fixes.md](TODO-v1.0.1-fixes.md) 的格式 —— v1.0 真测发现 4 个 bug，全部 P0，
> 一个 patch session 修完。
>
> **关联**：
> - [W8.5-real-test-checklist.md](W8.5-real-test-checklist.md) —— 真测清单
> - [TODO-v3.0.md](TODO-v3.0.md) —— v3.0 开发进度
> - [v3.0-release-notes.md](v3.0-release-notes.md) —— 发布说明
> - [KNOWN-ISSUES.md](KNOWN-ISSUES.md) —— 已知坑库

---

## 严重度定义

| 标签 | 含义 | 处理时机 |
|---|---|---|
| **P0** | 致命：用户无法启动 / 核心功能完全坏 / 数据丢失风险 / 安全护栏失效 | **本次 v3.0.1 patch 必修** |
| **P1** | 重要：某模块不可用但有 workaround / 影响诊断准确度 / UX 严重 | **本次 v3.0.1 patch 尽量修** |
| **P2** | 一般：UX 小问题 / 边缘场景 / 非阻塞 | **推 v3.1 或不修** |

---

## 待修 bugs

| # | 严重度 | 模块 | 用户现象 | 复现步骤 | 真根因 | 修复方案 | 状态 |
|---|---|---|---|---|---|---|---|
| _example_ | P0 | llama-server | 启动 60 秒后 GUI 报「端点不可达」 | 1. 进 PE  2. 等 startnet healthcheck 通过  3. NeuroBoot 启动 60s 后报错 | llama-server CRT 依赖缺 | 拷 17 个 CRT DLL 进 \NeuroBoot\llama-cpp\ | 已修 (v1.0.1) |
| 1 | | | | | | | 未开始 |
| 2 | | | | | | | 未开始 |
| 3 | | | | | | | 未开始 |
| 4 | | | | | | | 未开始 |
| 5 | | | | | | | 未开始 |

---

## 真测会话记录

填这一节让下次 session 能快速恢复上下文。

### 第 1 次真测（YYYY-MM-DD）

- **ISO 文件**：`pe-build/output/NeuroBoot.iso`（build 日期 / 大小 GB）
- **测试硬件**：（机型 / CPU / RAM）
- **测试机已下载嵌入模型**：是 / 否（Phase 3 决定 RAG 是 FTS5 only 还是 hybrid）
- **完成 checklist 项**：第 N / 100 项
- **耗时**：分钟
- **新发现 bug**：上方表格 #N1 到 #Nm
- **happy path 验证**：（写「全过」/ 哪些卡住了）

---

## 工程教训（边修边记）

每修一个 bug，回头看是不是有更深的工程教训没写进 KNOWN-ISSUES。
v1.0.1 真测发现的 5 个工程教训（#15-#19）现在已是 NeuroBoot 团队的「肌肉记忆」。
本次 v3.0.1 真测期望发现的新教训类别：

- **W6-7 hooks**: 用户自定义 PS 脚本失败时是否给清晰的诊断信息？
- **W5-6 RAG**: db 文件 corrupt / schema 版本不对时，是否 graceful 降级到 hardcoded？
- **W8 eval**: 真测发现 eval YAML 写错了的情况（must_match 太严 / 太松）？

---

## 修复完后

1. 全 cargo test 不退步（基线 210）
2. dumpbin 验证 crt-static
3. 99-build-all.ps1 一次过
4. 新 ISO 走完 [W8.5-real-test-checklist.md](W8.5-real-test-checklist.md) 复测
5. release v3.0.1

如果 v3.0.1 又发现新 bug，按本模板继续开 v3.0.2-fixes.md。

---
name: /diagnose-bsod
description: 用户报告蓝屏后，按此剧本走
---

当用户报告蓝屏（BSOD / 自动重启 / 突然黑屏），按以下步骤诊断：

## 1. 时间窗口确定
先问用户：
- 是**频繁**蓝屏（每天 / 每周）还是**一次性**？
- 最近一次大约是什么时候？

## 2. 证据收集（并行调）
- `read_event_log_errors` (hours=72, max_events=30) —— 看 Source=disk / Kernel-Power / WHEA-Logger / nvlddmkm 等关键源
- `list_minidumps` —— minidump 文件清单 + 时间戳。**多个 dump 时间密集** = 严重系统问题
- `analyze_minidump` —— **关键工具**：解析 dump 给出 BugCheckString / CausedByDriver（罪魁驱动）
- `list_recent_shutdowns` (max_events=20) —— Event ID 41 (Kernel-Power) / 6008 (异常关机) 配对就是蓝屏自动重启

## 3. 关键关联点
- analyze_minidump 给出的 `CausedByDriver` 是**首要嫌疑**（如 `nvlddmkm.sys` = NVIDIA 显卡驱动）
- `BugCheckString` 模式：
  - `DRIVER_IRQL_NOT_LESS_OR_EQUAL` → 驱动 bug（看 CausedByDriver）
  - `MEMORY_MANAGEMENT` → 内存问题（建议 MemTest）
  - `PAGE_FAULT_IN_NONPAGED_AREA` → 内存 / 驱动
  - `WHEA_UNCORRECTABLE_ERROR` → 硬件级问题（CPU / 内存 / PCIe）
  - `Ntfs.sys` → 文件系统损坏（下一步 chkdsk）
- Event 41 + 6008 同时间戳 ≈ 真蓝屏
- Kernel-Power 41 但无 6008 → 突然断电（电源 / 电池 / 插座）

## 4. 给用户的报告格式
```
最近 72 小时蓝屏次数：N
最近一次：YYYY-MM-DD HH:MM
最常见 BugCheck：[BugCheckString]
最可能罪魁：[CausedByDriver] (来自厂商 [Company])
可能原因：
1. ...
2. ...
建议下一步：
- ⚠ 先备份重要文件到 U 盘（蓝屏机随时可能挂）
- 推荐工具：sfc / chkdsk / 更新驱动 / ...
```

## 5. 不要做
- 不要直接调 `chkdsk` / `sfc` 工具 —— 让用户决定
- 不要删 minidump（它们是证据）
- 不要建议「重装系统」作为首选 —— 那是最后手段

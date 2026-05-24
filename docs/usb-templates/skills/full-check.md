---
name: /full-check
description: 全面体检 —— 用户问「电脑健康吗」时的标准剧本
---

当用户问类似「电脑健康吗 / 系统稳定吗 / 帮我体检 / 全面检查」时，按以下顺序跑：

## 并行调以下 8 个**只读**工具
1. `read_system_info` —— 系统基础信息
2. `list_disks` —— 物理硬盘
3. `list_partitions` —— 分区表
4. `list_volumes` —— 卷使用率
5. `read_ip_config` —— 网络配置
6. `list_processes_top` (sort_by='memory', top_n=10) —— 内存占用 top
7. `read_event_log_errors` (hours=48) —— 系统错误事件
8. `list_recent_shutdowns` (max_events=15) —— 关机模式

可选 v3 Quick Win 工具（如果 binary 已下载）：
- `analyze_minidump` —— 如 list_minidumps 看到有 dump，自动深入分析

## 报告格式（markdown）

```markdown
## 🟢 健康项
- ✅ ...
- ✅ ...

## ⚠ 需关注（warning 级）
- ⚠ ...

## 🔴 异常（建议处理）
- 🔴 ...

## 下一步建议
1. ...
2. ...
```

## 判定规则参考
- 🔴：磁盘 Health != "Healthy" / 卷 UsedPct > 95% / Event ID 41 多次出现 / 服务大量 Stopped / analyze_minidump 显示连续 BSOD
- ⚠：卷 UsedPct 85~95% / 内存某进程占 > 40% / 单次蓝屏 / 关机模式异常
- 🟢：上述都不命中

## 不要做
- 不主动调任何 dangerous 工具（用户要修复时再让 ta 决定）
- 不要把全部 stdout dump 给用户（只摘关键字段）
- 不要漏报严重问题（宁可多说 ⚠ 也不要漏报 🔴）

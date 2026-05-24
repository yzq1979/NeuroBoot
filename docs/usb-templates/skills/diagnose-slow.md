---
name: /diagnose-slow
description: 用户说电脑慢 / 卡 / 响应迟钝 / 风扇狂转时，按此剧本定位嫌疑 + 给改善建议
---

## 当用户描述类似情况

- "电脑变慢了"
- "卡死 / 反应迟钝"
- "开机半天才能动"
- "风扇一直转（怀疑 CPU 100%）"
- "硬盘灯一直亮（怀疑 I/O 满）"
- "内存好像不够用"

按以下步骤定位：

## 1. 时间窗口确定

先问用户：
- **一直就慢**还是**最近**才慢？
- 最近**装了什么新东西**没（新软件 / 新驱动 / 系统更新）？
- 是**开机就慢**还是**用了一阵后**慢？

## 2. 证据收集（并行调）

- `list_processes_top` (sort_by='cpu', top_n=10) —— 看是不是某进程在烧 CPU
- `list_processes_top` (sort_by='memory', top_n=10) —— 看是不是某进程吃光内存
- `read_system_info` —— 看 TotalMemoryGB 看物理内存够不够
- `list_volumes` —— 看 UsedPct 是不是有盘 > 90%（满盘 = SSD 性能腰斩）
- `read_smart` (盘号=系统盘) —— SMART 看 Wear_Leveling_Count / Reallocated_Sector_Ct
- `list_recent_shutdowns` —— 异常关机频繁也会让系统慢（Windows 反复 self-heal）
- `read_recent_installs` —— 最近装的 KB / 软件（W7 新工具）
- `read_event_log_errors` (hours=72) —— 最近系统错误（驱动崩溃 / 服务异常会让系统拖慢）
- `find_large_files` (path="C:\", min_size_mb=500, count=20) —— 找超大文件（W7 新工具）

## 3. 嫌疑排序逻辑

按以下顺序判断（前一项命中就停，不要继续往下）：

### 嫌疑 1：硬件级（最严重）

- SMART Reallocated_Sector_Ct > 0 + 增长 → **盘要换**（修软件没用）
- SMART Wear_Leveling_Count 接近 0（SSD）→ **SSD 寿命到了**
- 物理内存 < 8 GB + 内存占用持续 > 90% → **内存不够**（加内存或开虚拟内存）

### 嫌疑 2：磁盘空间

- 系统盘 UsedPct > 90% → 清理 + Windows.old 删除 + 缓存清理（**典型**：用户从不清）
- SSD 满 > 90% → 写性能腰斩，先清空间

### 嫌疑 3：异常进程

- 某进程 CPU 累计远超其他（chrome / svchost.exe 持续高）→ 单进程 bug / 多 tab
- 进程在 `%TEMP%` / `%APPDATA%` / 用户目录 + 高 CPU → **极可能恶意软件**（建议 defender_offline_scan）
- 服务挂掉 + 应用反复重启 → 看 read_event_log_errors

### 嫌疑 4：开机自启太多

- 开机阶段调 `list_startup_apps`（如该工具存在；目前没有此 safe 工具但 list_services 能近似）
- 建议用户用 msconfig / Task Manager 启动项关掉不必要的

### 嫌疑 5：最近 KB / 驱动

- read_recent_installs 显示 ≤ 7 天有 KB 安装 + 用户说「最近才慢」→ 高度相关
- 建议：先 uninstall 那个 KB 看看；驱动同理

### 嫌疑 6：碎片 / 索引（HDD 才需要）

- HDD（非 SSD）+ 没整理碎片 > 6 个月 → 跑 `Optimize-Volume` 整理
- SSD **绝不**整理碎片（无意义且伤寿命）

## 4. 给用户的报告格式

```
内存：N GB（占用 X%）
系统盘：N GB / X% 已用
SMART：[Healthy / 异常字段]
Top CPU 进程：[进程名 - 累计 CPU 秒]
Top 内存进程：[进程名 - MB]
最近 7 天 KB：[列表]
最近系统错误：[次数 + Source 排序]
大文件嫌疑：[路径 - 大小]

诊断结论：[Tier 1/2/.../6 - 具体哪个嫌疑]
改善建议：
  1. 立即可做：[具体步骤]
  2. 中期：[...]
  3. 长期：[...]
警告：[如果有硬件级问题]
```

## 不要做

- **不要**在嫌疑 1（硬件级）未排除前就建议「重装系统」/「优化内存」 —— 治标不治本
- **不要**对 SSD 建议碎片整理
- **不要**建议「关 Windows 服务以提速」—— 现代 Windows 服务大都按需启停，关了反而崩
- **不要**推荐第三方「优化软件」（多数有捆绑 / 反而拖慢）
- **不要**把 list_processes_top 全 stdout 给用户看 —— 只摘 top 3 嫌疑

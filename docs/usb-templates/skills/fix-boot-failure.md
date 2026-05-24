---
name: /fix-boot-failure
description: 用户开机起不来（黑屏 / 卡 logo / "no bootable device" / 自动修复循环）时的诊断 + 修复阶梯
---

## 当用户描述类似情况

- "电脑开不了机"
- "卡在 Windows logo / 转圈"
- "no bootable device" / "An operating system wasn't found"
- "Boot Device Not Found" / "Disk boot failure"
- "进了自动修复 / Automatic Repair 循环"
- "INACCESSIBLE_BOOT_DEVICE 蓝屏"

按以下步骤诊断 + 修：

## 1. 确认硬件层（先排除最严重的）

并行调：
- `list_disks` —— 系统盘是否被 PE 识别？HealthStatus / BusType 看到了吗？
  **关键**：如果系统盘根本不在列表里，**硬件死了**（SATA 线 / NVMe 接触 / SSD 控制器挂）—— 不是软件能修的
- `read_smart` (盘号=系统盘) —— SMART 看 Reallocated_Sector_Ct / Pending_Sectors。
  **巨量坏块 = 盘要换**，修软件没用

## 2. 检查分区表 + 启动配置

- `list_partitions` (disk_number=系统盘) —— 看是否还有 EFI 分区（GPT）/ Active 分区（MBR）
  **缺 EFI/Active 分区** = 分区表损坏，下一步用 testdisk 救援
- `list_winre_status` —— 看 WinRE 状态 + BCD 是否完整（W7 新工具）
- `read_bcd_store` —— 看 BCD 详情（W7 新工具，与上面一个工具配对）
- `read_recent_installs` —— 最近 KB / 软件安装（W7 新工具）—— 启动失败的常见诱因

## 3. 决定升级阶梯（按破坏力从小到大）

### Tier 1：Uninstall Updates（最轻）

如果 `read_recent_installs` 显示最近 7 天有 KB 安装：
- 引导用户在 WinRE → 高级选项 → 卸载最近的质量更新
- 这是 KB-导致的启动失败最快的修法（成功率 ~70%）

### Tier 2：Startup Repair

- WinRE → 高级选项 → Startup Repair
- 微软自动尝试 12 种修复模式，无破坏性
- **如果反复进 Startup Repair 循环**，说明 Tier 2 帮不上，升 Tier 3

### Tier 3：System Restore（如启用还原点）

- WinRE → 高级选项 → System Restore
- 选 KB 安装前的还原点
- 会回滚系统改动但**保留用户数据**

### Tier 4：手动 bootrec（在 PE / WinRE cmd 里）

如果 `read_bcd_store` 看不到 BCD 或显示损坏：

按顺序跑（每条等用户确认）：
1. **`bootrec_rebuild_bcd`**（NeuroBoot dangerous 工具）—— 扫盘找 Windows 安装重建 BCD
2. 不行就在 cmd 里跑 `bootrec /fixmbr`（MBR 盘）/ `bootrec /fixboot`（写引导扇区）
3. **如果 `bootrec /fixboot` 返回 Access Denied**（UEFI/GPT 常见）：
   用 `bcdboot C:\Windows /s X: /f ALL`（X 是 EFI 系统分区盘符，先 mountvol 挂上）

### Tier 5：Reset this PC（保留个人文件）

WinRE → 重置此电脑 → 保留个人文件
- 重装系统，但 C:\Users 数据保留
- 装的软件全没（设置 / 注册表全重置）

### Tier 6：本地重装 + 数据救援

如果上述都没用，且需要保数据：
- **绝不直接重装**！数据会丢
- 先把当前损坏盘做完整镜像（用 ddrescue 或 `wbadmin`）
- 镜像放外置盘后再格式化原盘重装
- 镜像里救文件用 PhotoRec / R-Studio

## 4. 给用户的报告格式

```
硬件层：[系统盘 PE 中可见 / 不可见]
SMART 健康：[Healthy / Warning - <字段>]
分区表：[GPT 完整 / MBR 完整 / 损坏 - 缺 X 分区]
BCD：[OK / 缺失 / 损坏]
最近 KB：[KB 编号 + 日期，如有]
判断：[Tier 1 / 2 / 3 / ...]
推荐操作（从轻到重）：
  1. ...
  2. ...
警告：[如有数据丢失风险]
```

## 不要做

- **绝不**在硬件层判明前就跑 bootrec —— 盘坏时跑无意义还可能让备份头进一步损坏
- **绝不**给 `bootrec /fixmbr` 用在 GPT 盘 —— 会改写 GPT 备份头
- **绝不**直接建议「重装」前没救数据
- **绝不**在没经用户确认时调任何 dangerous 工具

---
name: /recover-data
description: 用户误删 / 格式化 / 找不到文件时的数据救援剧本（PhotoRec / TestDisk / 防覆盖警告）
---

## 当用户描述类似情况

- "刚才误删了一个文件 / 文件夹"
- "不小心格式化了硬盘 / U 盘"
- "分区不见了"
- "回收站清空了想找回"
- "拍的照片 / 录的视频找不到了"

## 1. 最关键的事：立即停止写盘

**第一句话告诉用户**：
> **立即停止往那个盘写任何东西**。包括别保存文件、别装软件、别让浏览器下载缓存到那个盘。
> 每写一个文件都可能覆盖被删数据 —— 救援成功率与「删除后到救援之间的写入量」成反比。

如果是系统盘（C:）被删了数据：
- **立刻关机**，进 PE 救援（在主系统跑会持续写日志 / 临时文件）

## 2. 评估情况（决定用哪个工具）

并行调：
- `list_disks` —— 看目标盘是否被识别
- `list_partitions` (disk_number=目标盘) —— 分区表是否完整
- `list_volumes` —— 文件系统是 NTFS / FAT32 / RAW

按情况分流：

### Case A：分区还在 + 文件系统识别 (NTFS) + 只是文件被删

→ 用 **PhotoRec**（按文件签名扫描，能找到回收站清空 / shift+del 删的）
- 优势：不需要文件系统完整
- 缺点：恢复出的文件名是 `f0000001.jpg` 之类（按内容类型推断扩展名）

### Case B：分区不见了 / 分区表损坏

→ 用 **TestDisk** 先救分区表
- 调 `testdisk_scan_partition` 启动 TestDisk TUI
- TestDisk 扫盘找历史分区 → 用户确认后写回分区表
- **极度危险**：写错分区表会让数据更难救

### Case C：被格式化了

→ 区分 **快速格式化**（只清文件表）vs **完全格式化**（覆盖整盘）
- 快速：用 PhotoRec 多数能救回
- 完全：基本救不回（除非花大钱找数据恢复公司物理读盘）

### Case D：FileSystem 显示 RAW

→ 文件系统结构损坏。**绝不格式化**（那会让数据真的丢）
- 用 TestDisk 尝试修文件系统
- 救不了就用 PhotoRec 按签名扫

## 3. **关键防护：救援目标盘 ≠ 源盘**

调任何工具前**强制问用户**：
- 救出来的文件存到**哪个盘**？

**必须**满足：
- 目标盘 ≠ 源盘（防止边救边覆盖）
- 目标盘剩余空间 > 源盘已用空间的 2 倍（PhotoRec 会救出大量 false positives）
- 推荐目标：外置 USB 硬盘 / 第二块固定盘 / U 盘

**绝不允许**：
- 救 C: 数据存到 C: —— 直接覆盖
- 救 D: 数据存到 D: —— 同上
- 救到 NeuroBoot 本身的 U 盘（X: 是 ramdisk，重启就没）

## 4. 启动救援（按 Case 选）

### Case A / C / D + PhotoRec

PhotoRec 在 `X:\NeuroBoot\tools\testdisk\photorec_win.exe`（v3.1 W2-3 后会有专用工具，
当前先引导用户手动启动）：

1. 用户跑 `photorec_win.exe`
2. 选源盘（**不要选错** —— PhotoRec 列表是物理设备 `\\.\PhysicalDriveN`）
3. 选文件系统类型（NTFS / FAT32 / Other）
4. 选 "Free" 模式（只扫未分配空间，快）或 "Whole" 模式（全盘扫，慢但全）
5. 选**目标输出目录**（必须在另一个盘！）
6. 等扫描完成（几十分钟到几小时）
7. 在输出目录里找文件（按类型 `recup_dir.1` / `.2` / ... 分批）

### Case B + TestDisk

调 `testdisk_scan_partition` → 启动 TestDisk TUI。
用户按提示：
1. Create new log
2. 选物理硬盘
3. 选分区表类型（Intel/PC 或 EFI GPT）
4. Analyse → Quick Search
5. 找到分区按 P 看里面文件确认对
6. 没找到的话 Deeper Search
7. **确认无误后** Write —— 这一步不可逆，写错分区表数据更难救

## 5. 给用户的报告格式

```
盘状态：[识别 / 不识别]
分区表：[完整 / 损坏 / 缺 X 分区]
文件系统：[NTFS / FAT32 / exFAT / RAW]
数据丢失方式：[误删 / 格式化 / 分区不见]
Case 判定：[A / B / C / D]
推荐工具：[PhotoRec / TestDisk]
目标盘要求：[必须 ≠ 源盘，空间 > N GB]
警告：
  - 救援期间绝不写源盘
  - PhotoRec 救出的文件名是 fNNNN.ext，不是原名
  - TestDisk Write 不可逆，强烈建议先 ddrescue 全盘镜像
专业服务推荐：[如 Case C 完全格式化等无救情况]
```

## 不要做

- **绝不**直接调 testdisk Write —— 用户必须看到 Quick/Deeper Search 找到的分区清单后人工确认
- **绝不**在没确认目标盘 ≠ 源盘前启动救援
- **绝不**让用户在源盘上「试一下」chkdsk 或 sfc —— 这些会写盘，可能毁掉未删文件
- **绝不**承诺「一定能救回」—— 救援成功率受多因素影响，不要给虚假希望
- **不要**对极重要数据（如商业 / 法律证据）建议自救 —— 推荐专业数据恢复公司

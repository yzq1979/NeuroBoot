---
name: /recover-bitlocker
description: 用户开机被 BitLocker 要恢复键时（常见于 KB 更新后），按此引导找钥匙 + 走恢复阶梯
---

## 背景

2025-2026 多次 Windows 11 月度 KB 更新（如 KB5083769 / KB5089549）触发 BitLocker
恢复键循环 —— PCR7 + Secure Boot 配置变化让 TPM 解封策略失效，开机要求输入 48 位
恢复键。**这是已知问题**，用户大多不是被攻击。

## 当用户描述类似情况

- "开机蓝紫色屏要 BitLocker 恢复键"
- "更新后进不去系统了，要 recovery key"
- "BitLocker recovery 输入框反复弹出"
- "48 digit key" / "BitLocker 恢复 ID"

按以下步骤：

## 1. 收集证据（如果能进 PE 看到主系统盘）

并行调：
- `bitlocker_status` —— 看 C: 加密状态 / TPM 协议 / Protector 类型（W7 新工具）
- `read_bcd_store` —— 看 BCD 是否被 KB 改了 boot config（W7 新工具，名 `list_winre_status` 涵盖）
- `read_recent_installs` —— 看最近 7 天装了哪些 KB（W7 新工具）

**找出**：最近 1-2 周是否装了 KB5083769 / KB5089549 / 其它累积更新。

## 2. 引导用户找恢复键（关键步骤）

恢复键在以下位置之一，按优先级问用户：

1. **Microsoft 账户**（消费版最常见）：
   - 用手机访问 `account.microsoft.com/devices/recoverykey`
   - 用 Microsoft 账户登入
   - 找到这台电脑 → 复制 48 位密钥
2. **企业 / 学校账户** → AD / Intune / Azure AD 管理员后台
3. **打印件** —— 当初启用 BitLocker 时如果打印过
4. **USB 备份** —— 启用时如果存到 U 盘了
5. **导出文件** —— `BitLockerRecoveryKey-*.txt` 可能在某个云盘里

**不要**让用户用「猜」或「试」—— BitLocker 失败 5 次会硬性锁定一段时间。

## 3. 恢复阶梯（成功输入恢复键进 WinRE 之后）

按从轻到重排序，前一步不行再走下一步：

1. **Uninstall Updates**（最轻）：WinRE → 高级选项 → 卸载最新质量更新。
   90% KB-触发的 BitLocker 循环靠这步解决，且最不破坏数据
2. **System Restore**：回滚到 KB 安装前的还原点（如启用了）
3. **Startup Repair**：自动诊断 + 修
4. **Command Prompt 修 BCD**：在 WinRE cmd 里跑 `bcdedit /enum` 看 boot config；
   `bootrec /fixboot` 失败的话用 `bcdboot C:\Windows /s X: /f ALL`（X 是 EFI 系统分区）
5. **Reset this PC**（保留个人文件）—— 最后手段，**会重装系统**但保留 C:\Users
6. **本地重装**（不保留 → 数据全没）—— 一定确认数据已救出

每升一级都要再次确认用户是否同意。

## 4. 如果用户找不到恢复键

**坏消息**：BitLocker 设计上**没有后门**。找不到恢复键 = **数据永久无法访问**。
此时只能：
- 备份后格式化重装（数据丢）
- 联系数据恢复公司（成功率极低）
- 检查云盘是否有重要文件副本

## 5. 给用户的报告格式

```
BitLocker 状态：[加密中 / 未加密 / 暂停]
最近触发 KB：[KB 编号 + 安装日期]
恢复键查找途径：
  1. Microsoft 账户（[手机/网页访问 account.microsoft.com/devices]）
  2. ...
推荐恢复阶梯：
  优先：卸载最近 KB（成功率 ~90%）
  次选：System Restore（如启用还原点）
  保底：Reset this PC（保留个人文件）
警告：如所有恢复键都找不到，BitLocker 无后门，数据无法访问
```

## 不要做

- **不要建议「关 BitLocker」作为预防** —— 很多场景（公司电脑 / 笔记本）BitLocker 是合规要求
- **不要在用户没找到恢复键前**就建议 Reset/重装 —— 优先帮 ta 找钥匙
- **不要谎称有「绕过 BitLocker」的方法** —— 没有，AI 不要编
- 不要建议关 Secure Boot（会让 BitLocker 再次要恢复键）

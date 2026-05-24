---
name: /reset-password
description: 用户忘记 Windows 登录密码 / 账户被锁时的诊断 + 用 NTPWEdit 清空密码
---

## 当用户描述类似情况

- "忘记 Windows 登录密码了"
- "登录界面输了密码说不对"
- "账户被锁了"
- "想重置管理员密码"
- "进不去自己电脑"

## 1. 先确认账户类型（关键步骤）

问用户：
- 登录界面**显示的是邮箱地址吗**（如 `xxx@outlook.com` / `xxx@gmail.com`）？
- 还是显示**用户名**（如 `Administrator` / `张三`）？
- 这台电脑是**公司 / 学校发的**吗？还是私人？

### 邮箱地址 → Microsoft 账户

- **NeuroBoot 不能重置 Microsoft 账户密码** —— 那是云端账户
- 引导用户去 `account.microsoft.com` 用手机 / 备用邮箱重置
- 重置成功后回这台电脑登入要联网（Wi-Fi 或有线）
- **不需要进 PE 跑任何工具**

### 公司 / 学校账户（域账户）

- **NeuroBoot 不能重置域账户密码**
- 引导用户找 IT 管理员
- 域账户的密码策略在 AD 里，本地工具改不了

### 本地账户（用户名是 Administrator / 中文名 / 自定义）

- ✅ **NeuroBoot 可以用 NTPWEdit 清空**
- 继续下面步骤

## 2. 检查 BitLocker 是否启用

并行调：
- `list_disks` —— 找系统盘盘号
- `bitlocker_status` —— C: 加密状态（W7 新工具）

**如果 BitLocker 开了** 但密码忘了：
- 系统盘是加密的，PE 看不到 SAM hive
- 必须先用恢复键解锁 → 见 `/recover-bitlocker` skill
- 解锁后再跑本剧本

## 3. 确认 SAM hive 路径

PE 启动后系统盘通常被识别为某个盘符（不一定是 C:）。
- 跑 `list_volumes` 找文件系统是 NTFS + DriveType=Fixed + 有 Windows 文件夹的卷
- 典型路径：`<盘符>:\Windows\System32\config\SAM`

## 4. 调 reset_local_admin_password（dangerous，需确认）

调用前必须明确告诉用户：
- 此操作**直接编辑 SAM hive**（不可撤销，但只清密码，不删账户）
- 完成后**重启**回主系统，**密码框留空**就能登入
- **EFS 加密的文件会失去访问权限**（用户 EFS 私钥与原密码绑定）
- **存在浏览器 / Outlook 里的密码**会失效（Windows DPAPI 与原密码绑定）—— 这些要用户重新输入

确认无误后调：
```
reset_local_admin_password(
  sam_path="<盘符>:\Windows\System32\config\SAM",
  username="<用户名>"
)
```

工具会启动 NTPWEdit GUI，在新窗口里按提示操作：
1. 选中目标账户
2. 点 "Change password" 清空（不输新密码 = 清空）
3. 点 "Save changes" + 确认
4. 关闭 NTPWEdit
5. 重启回主系统

## 5. 给用户的报告格式

```
账户类型：[本地 / Microsoft / 域]
[如本地] 检测到的本地账户：[列表]
BitLocker 状态：[启用 - 需先恢复键 / 未启用]
SAM hive 路径：[路径]
操作步骤：
  1. 调 reset_local_admin_password (sam_path=..., username=...)
  2. NTPWEdit 弹窗里选账户 → Change password 清空 → Save
  3. 关 NTPWEdit → 重启
  4. 登录界面密码留空 → 进入
警告：
  - EFS 加密文件将失去访问权
  - 浏览器/Outlook 保存的密码会失效
```

## 不要做

- **绝不**在「不是用户自己电脑」上跑 —— 本工具能清任何本地账户密码，**有授权才合法**。
  公司 / 朋友 / 陌生电脑 = 拒绝
- **不要**对 Microsoft / 域账户跑（无效 + 浪费时间）
- **不要**在 BitLocker 启用且没解锁前跑（SAM 看不到）
- **不要**清完密码后忘记提醒「EFS 文件 + 浏览器密码会失效」
- **不要**给用户「破解密码」的错觉 —— 我们是清空，不是恢复

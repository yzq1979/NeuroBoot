# 我的 NeuroBoot 持久化记忆

> 这个文件在 NeuroBoot 启动时**自动加载到 system prompt 顶部**，AI 在每次对话开头都能读到。
> 把你希望 AI 长期记住的**机器层面的事实**（硬件 / 软件 / 网络 / 已知坑）写在这。
>
> **路径**：U 盘根 `<U盘>\NeuroBoot\memories\MEMORY.md`
>
> **用法**：你也可以直接在 NeuroBoot 里说「记住 X」，AI 会自动调 memory 工具更新这个文件。
> 写完后**不需要**手动 push 给 AI —— 下次启动自动加载。

---

## 硬件

- **型号**：（在这里写，如 ThinkPad X1 Carbon Gen 11）
- **CPU**：（如 i7-1370P，14 核 20 线程）
- **内存**：（如 32 GB DDR5-5200）
- **硬盘布局**：
  - `C:` 系统盘 NVMe 1 TB（剩 ~300 GB）
  - `D:` 备份 SATA SSD 512 GB
  - `E:` 外接 USB-C 移动硬盘 2 TB（数据盘）
- **显卡**：（如 Iris Xe 集显 + RTX 4060 独显）

## 软件 / 系统状态

- **系统**：Windows 11 24H2 build XXXXX
- **关键软件路径**：
  - 项目代码：`D:\workspace\`
  - 浏览器 profile：`C:\Users\<我>\AppData\Local\Google\Chrome\User Data\`
- **BitLocker 状态**：（启用 / 未启用）；如启用，恢复键存在……

## 网络 / 账号

- **家里 Wi-Fi SSID**：`MyHome_5G`（密码记在我的密码管理器里，不写这里）
- **公司 VPN**：（如有）
- **域名 / 内网共享**：`\\nas.local\backup`（用户名 `myname`）

## 历史已知问题

- **2026-04-20 蓝屏**：DRIVER_IRQL_NOT_LESS_OR_EQUAL，是显卡驱动 535.xx 兼容问题，回退 530.xx 解决
- **2026-03 偶尔 Wi-Fi 断**：Intel AX211 驱动 22.xxx 有 bug，升 23.xxx 修复

## 操作偏好

- 命令行**优先 PowerShell**，不用 cmd
- 修复前**优先备份**到 `E:\auto-backup\`
- 不要建议「重装系统」作为修复方案 —— 这是最后选项
- 中文术语 + 中文回答

---

> **AI 注意**：本文件由用户维护。你可以用 `memory(command='view', path='MEMORY.md')` 重读最新版，
> 用 `memory(command='str_replace', ...)` 更新已有条目。**未经用户授权不要主动改本文件**。

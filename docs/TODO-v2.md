# NeuroBoot v2.0 路线图 / TODO

> **⚠️ 优先看：[TODO-v1.0.1-fixes.md](TODO-v1.0.1-fixes.md)** —— 2026-05-24 U 盘真测反馈的 4 个 P0 紧急修复（中文输入 / 蓝牙鼠标 / 端点未启用 / 配置 UI），明天先做这些，再回到 v2 路线图。
>
> 本文档基于对 14 路 WebSearch 调研结果（Windows 命令行系统维护工具，PE 兼容）+ v1.0 实际使用反馈，整理下个版本（v2.0）需要新增 / 修改 / 优化 / 完善的工作清单。
>
> **现状（v1.0）**：4 个工具（3 safe + 1 dangerous），单一 Qwen3-4B 本地推理，非流式输出。已生成可启动的 NeuroBoot.iso 2.89 GB。

---

## Part A：待集成的命令行工具

每个工具都标注：[ ] 未做 / [P0] 必做 / [P1] 重要 / [P2] nice-to-have；安全等级 Safe / Dangerous；底层命令。

### A.1 已实现（v1.0 baseline）

| 工具 | 安全 | 底层 |
|---|---|---|
| `list_disks` | Safe | `Get-Disk` |
| `read_system_info` | Safe | `Get-CimInstance Win32_OperatingSystem/ComputerSystem/Processor/BIOS` |
| `read_event_log_errors` | Safe | `Get-WinEvent -FilterHashtable @{LogName='System';Level=1,2}` |
| `delete_path` | Dangerous | `Remove-Item -LiteralPath -Recurse -Force`（黑名单防整盘） |

### A.2 新增 Safe 工具（只读诊断，Agent 自动调用）

#### A.2.1 硬件/系统信息深化（P0）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_cpu_detail` | `Get-CimInstance Win32_Processor` 全字段 | 频率 / 核心 / L2/L3 cache / 微码版本 |
| `read_memory_modules` | `Get-CimInstance Win32_PhysicalMemory` | 每条内存条：容量、频率、厂商、序列号、插槽 |
| `read_gpu_info` | `Get-CimInstance Win32_VideoController` | 显卡型号、显存、驱动版本、分辨率 |
| `read_motherboard_info` | `Get-CimInstance Win32_BaseBoard` + `Win32_BIOS` | 主板厂商/型号、SMBIOS UUID |
| `read_battery_info` | `Get-CimInstance Win32_Battery` + `powercfg /batteryreport` | 笔记本电池健康度、设计容量 vs 当前满电容量 |
| `read_thermal_info` | `Get-CimInstance MSAcpi_ThermalZoneTemperature -Namespace root\wmi` | CPU/SOC 温度 |

#### A.2.2 磁盘/存储扩展（P0）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `list_partitions` | `Get-Partition` | 每块盘的分区详情（GPT/MBR、类型、字母、大小） |
| `list_volumes` | `Get-Volume` | 卷信息 + 使用率（FreeSpace / Size）|
| `read_storage_reliability` | `Get-StorageReliabilityCounter` | 读写错误计数、温度、上电小时、累计写入 |
| `read_smart` | `smartctl.exe -a /dev/sdX`（**需要打包 smartmontools**） | 详细 SMART 属性、self-test 历史、磨损度 |
| `check_filesystem_readonly` | `chkdsk <drive> /scan`（**只扫描不修**） | 文件系统错误检测，**不带 /f 不写盘** |
| `list_open_files` | `openfiles /query /v` | 哪些文件被锁/占用（要求事先 `openfiles /local on`） |
| `list_shadow_copies` | `vssadmin list shadows` | 卷影副本 / 还原点 |

#### A.2.3 网络诊断（P0）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_ip_config` | `ipconfig /all` | IP / MAC / DHCP / DNS 全量 |
| `ping_host` | `ping -n 4 <host>` | 连通性测试，参数 host + count |
| `trace_route` | `tracert <host>` | 路由路径 |
| `path_ping` | `pathping <host>` | tracert + 各跳延迟统计（更慢更详细） |
| `nslookup_query` | `nslookup <domain> [server]` | DNS 解析 |
| `list_arp_cache` | `arp -a` | IP↔MAC 缓存 |
| `list_netstat` | `netstat -ano` | TCP/UDP 连接 + 监听端口 + PID |
| `list_network_adapters` | `Get-NetAdapter` | 网卡列表 + 状态 + 速率 |
| `read_routing_table` | `route print` 或 `Get-NetRoute` | 路由表 |
| `flush_dns` | `ipconfig /flushdns`（**Dangerous 边界**，无副作用但有时归 D） | 清 DNS 缓存 |
| `wlan_show` | `netsh wlan show profiles` / `... show interfaces` | Wi-Fi 信息（PE 里 Wi-Fi 受限） |

#### A.2.4 进程/服务（P0）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `list_processes_top` | `Get-Process \| Sort CPU,WS -Desc \| Select -First 20` | 按 CPU/内存排序前 N 进程 |
| `find_process_by_name` | `Get-Process -Name <name>` | 按名查进程 |
| `list_services` | `Get-Service \| Where Status -eq 'Running'` | 运行中服务（可加状态过滤）|
| `list_startup_apps` | `Get-CimInstance Win32_StartupCommand` | 开机自启程序 |
| `list_scheduled_tasks` | `Get-ScheduledTask` | 计划任务（可过滤 Enabled） |

#### A.2.5 事件日志增强（P1）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_event_log_application` | `Get-WinEvent -LogName Application -Level 1,2 -MaxEvents 20` | 应用层 errors |
| `read_event_log_by_provider` | `Get-WinEvent -ProviderName <Name>` | 按来源（如 disk / Wlansvc / Service Control Manager） |
| `read_event_log_by_id` | `Get-WinEvent -FilterHashtable @{LogName='System';Id=<id>}` | 按事件 ID（如 6008 异常关机） |

#### A.2.6 蓝屏/dump 分析（P0）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `list_minidumps` | `Get-ChildItem C:\Windows\Minidump\*.dmp` | 列 minidump 文件 + 大小 + 时间 |
| `analyze_minidump` | 调 **BlueScreenView.exe**（**需打包**）`/scomma <output.csv>` | 解析 dump → driver / fault address / bug check code |
| `read_memorydump_settings` | `wmic recoveros get DebugInfoType,DebugFilePath` | 系统蓝屏 dump 类型设置 |
| `list_recent_shutdowns` | `Get-WinEvent -FilterHashtable @{LogName='System';Id=6005,6006,6008,41,1074}` | 6008=异常关机、41=Kernel-Power 异常重启 |

#### A.2.7 驱动管理（P1）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `list_drivers` | `pnputil /enum-drivers` | 第三方驱动包列表 |
| `list_devices` | `Get-PnpDevice` | 全部 PnP 设备 + 状态 |
| `find_problem_devices` | `Get-PnpDevice -Status Error,Unknown` | 黄色感叹号 / 红叉设备 |
| `list_signed_drivers` | `Get-CimInstance Win32_PnPSignedDriver` | 驱动签名状态 |

#### A.2.8 启动/引导（只读）（P1）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_bcd_store` | `bcdedit /enum` | 启动配置详情 |
| `read_efi_partition` | `mountvol`（无参数）+ `diskpart` 只读查 | EFI 分区识别 |
| `read_secure_boot_state` | `Confirm-SecureBootUEFI` | Secure Boot 启用状态 |

#### A.2.9 注册表（只读）（P1）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_registry_value` | `reg query <path> /v <name>` | 单值查询 |
| `read_registry_key` | `reg query <path> /s` | 子键递归（限深度防过大） |
| `list_uninstall_entries` | `reg query HKLM\SOFTWARE\...\Uninstall` | 已装程序列表 |

#### A.2.10 性能/资源（P1）

| 工具 | 底层命令 | 说明 |
|---|---|---|
| `read_cpu_usage` | `Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 3` | 实时 CPU 占用 |
| `read_memory_usage` | `Get-CimInstance Win32_OperatingSystem` 算 Free / Total | RAM 使用率 |
| `read_disk_io` | `Get-Counter '\PhysicalDisk(*)\% Disk Time'` | 磁盘繁忙度 |
| `power_efficiency_report` | `powercfg /energy /duration 60` | 60 秒电源效率报告 |
| `list_handles` | Sysinternals **handle.exe**（**需打包**） | 文件句柄持有进程 |

### A.3 新增 Dangerous 工具（要确认弹窗）

#### A.3.1 文件操作（P1）

| 工具 | 底层 | 风险 |
|---|---|---|
| `move_path` | `Move-Item -LiteralPath` | 中（移动到错位置） |
| `rename_file` | `Rename-Item` | 低 |
| `set_file_attributes` | `attrib +/-r +/-h +/-s +/-a` | 低 |
| `copy_path` | `Copy-Item -Recurse` | 低（同名覆盖要确认） |

#### A.3.2 磁盘修复（P0）

| 工具 | 底层 | 风险 |
|---|---|---|
| `run_chkdsk_fix` | `chkdsk <drive> /f /r` | 中（修复 + 坏块重映射，过程不可中断） |
| `run_sfc_scannow` | `sfc /scannow` | 低（修复系统文件，从 WinSxS 还原） |
| `run_dism_restorehealth` | `DISM /Online /Cleanup-Image /RestoreHealth` | 低（在线修复系统镜像） |
| `format_partition` | `Format-Volume -DriveLetter <X> -FileSystem NTFS` | **极高**（销毁数据），三重确认 |
| `extend_partition` | `Resize-Partition -DriveLetter <X> -Size ...` | 中 |
| `shrink_partition` | 同上 | 中 |

#### A.3.3 启动修复（P0）

| 工具 | 底层 | 风险 |
|---|---|---|
| `rebuild_bcd` | `bootrec /rebuildbcd` | 中（重建启动配置） |
| `fix_mbr` | `bootrec /fixmbr` | 中（MBR 重写，UEFI 系统不适用） |
| `fix_boot_sector` | `bootrec /fixboot` | 中 |
| `bcdboot_repair` | `bcdboot C:\Windows /s S: /f UEFI` | 中（重建 EFI 引导文件） |

#### A.3.4 病毒/恶意软件（P0）

| 工具 | 底层 | 风险 |
|---|---|---|
| `defender_quick_scan` | `MpCmdRun.exe -Scan -ScanType 1` | 低（仅扫描） |
| `defender_full_scan` | `MpCmdRun.exe -Scan -ScanType 2` | 低（全盘扫，慢） |
| `defender_custom_scan` | `MpCmdRun.exe -Scan -ScanType 3 -File <path>` | 低 |
| `defender_offline_scan` | `MpCmdRun.exe -Scan -ScanType 2 -BootSectorScan` | 中（需要重启进 PE 扫，PE 内反而合适） |
| `defender_update_signatures` | `MpCmdRun.exe -SignatureUpdate` | 低 |

#### A.3.5 驱动安装/卸载（P1）

| 工具 | 底层 | 风险 |
|---|---|---|
| `install_driver` | `pnputil /add-driver <inf> /install` | 中（错误驱动可能蓝屏） |
| `remove_driver` | `pnputil /delete-driver <oem.inf> /uninstall /force` | 高（卸网卡/显卡驱动可能让系统不可用） |

#### A.3.6 进程/服务（P1）

| 工具 | 底层 | 风险 |
|---|---|---|
| `kill_process` | `Stop-Process -Id <pid> -Force` | 中（杀关键进程系统蓝屏） |
| `restart_service` | `Restart-Service <name>` | 中 |
| `set_service_startup` | `Set-Service -StartupType <Auto/Manual/Disabled>` | 中（禁关键服务系统起不来） |

#### A.3.7 注册表写入（P0，最危险类）

| 工具 | 底层 | 风险 |
|---|---|---|
| `write_registry_value` | `reg add <path> /v <name> /t <type> /d <data> /f` | 高 |
| `delete_registry_value` | `reg delete <path> /v <name> /f` | 高 |
| `delete_registry_key` | `reg delete <path> /f` | **极高**（删错键系统不可用） |
| `import_registry_file` | `reg import <file.reg>` | 高 |
| `backup_registry_key` | `reg export <path> <file.reg>`（**实际 Safe**） | 应放 safe |

#### A.3.8 文件恢复（P1）

| 工具 | 底层 | 风险 |
|---|---|---|
| `winfr_recover_regular` | `winfr <src>: <dst>: /regular /n <pattern>` | 低（只读源，写目标） |
| `winfr_recover_extensive` | `winfr <src>: <dst>: /extensive` | 低 |
| `photorec_carve` | **PhotoRec**（需打包）—— 文件签名恢复 | 低 |
| `testdisk_repair_partition` | **TestDisk**（需打包）—— 修复分区表 | **高** |

#### A.3.9 内存/电源（P2）

| 工具 | 底层 | 风险 |
|---|---|---|
| `schedule_memory_test` | `mdsched.exe`（计划重启后跑 Windows Memory Diagnostic） | 中（重启） |
| `boot_memtest86` | 引导 MemTest86 ISO（Ventoy 菜单加） | 低 |

### A.4 需打包到 PE 的外部工具

| 工具 | 大小 | 用途 | 许可 |
|---|---|---|---|
| **smartmontools (smartctl.exe)** | ~5 MB | 详细 SMART 数据（PE 默认无） | GPL，可商用 |
| **NirSoft BlueScreenView** | 83 KB portable | 解析 minidump 看 driver/bug code | Freeware，禁止商业重分发，自用 OK |
| **Sysinternals Suite** (Process Explorer / Autoruns / handle / psloglist) | ~50 MB | 进程详情、自启项审计、文件句柄查询 | Microsoft EULA |
| **TestDisk + PhotoRec** | ~5 MB | 分区表修复 + 文件签名恢复（开源 portable） | GPL v2+ |
| **MemTest86** | ISO ~50 MB | Ventoy 双启动选项之一，非进 NeuroBoot PE | Free for personal |
| **7-Zip portable** (7za.exe) | ~1.5 MB | PE 里解压 zip/7z/rar | LGPL |
| **Notepad++ portable** (可选) | ~10 MB | 编辑配置文件比 Notepad 强 | GPL v3 |

> 注：BlueScreenView / Sysinternals 不能直接打包重分发到公开发行的 PE，但**自用 / 内部使用 OK**。如果要做公开发行版 NeuroBoot，需要换 GPL 替代品或要求用户自己下载放入 Ventoy 数据分区。

---

## Part B：Agent & LLM 架构改进

### B.1 流式输出（P0）
- 当前 `stream: false`，模型生成完整 response 才一次性显示，长答复让用户等
- 改为 `stream: true`，用 SSE 解析增量 tokens，UI 边生成边显示
- 实现：reqwest blocking 改用 `text/event-stream` Reader；worker 边读边 send `AgentEvent::TokenChunk(s)`；UI 把 chunk append 到当前 assistant message
- 对 PE 内 CPU 推理体验提升明显（30~60 秒不再"卡屏"）

### B.2 多轮上下文管理（P1）
- 工具结果可选「摘要化」：长 JSON（如 `read_event_log_errors` 返回 20 条事件 ~ 2 KB）自动截断 + 关键字段保留
- 实现：每条 Role::Tool 消息加 `full_content` 字段（完整），UI 显示 truncated 版；agent 发回 LLM 时如已超 ctx 用摘要替代
- Truncation 策略可配置（保留最近 N 工具结果完整，老的摘要）

### B.3 Agent loop 增强（P1）
- 「停止生成」按钮 —— UI 触发 cancellation token → worker poll 检测 → 中断 reqwest 请求
- 「重试上一条」按钮 —— 删除最后 assistant + tool messages，重 spawn agent
- 「重新提问」编辑框（编辑用户最后一条 user message，删后续，重发）
- 工具调用失败自动重试（最多 1 次，带 backoff）

### B.4 端点 A+C 路由增强（P1）
- A 端点 UI 配置面板（不只 env var，可在 UI 里改 endpoint/api_key/model）
- 定时探测 A 重连（不只启动一次）；A 恢复时 toast 通知
- 多个 A 端点 fallback 列表（A1 → A2 → C）
- 顶栏显示「上次响应延迟 X ms」让用户判断切换值不值

---

## Part C：工具基础设施改进

### C.1 Tool trait 扩展（P0）
```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn safety(&self) -> SafetyClass;
    fn parameters_schema(&self) -> Value;
    fn execute(&self, args: &Value) -> ToolOutput;

    // v2 新增：
    fn requires_admin(&self) -> bool { false }      // 未 admin 时给清晰错误而不是让 PS 报权限
    fn category(&self) -> &str { "general" }        // "disk" / "network" / "security" / "boot" 等
    fn version(&self) -> u32 { 1 }                  // 升级工具时 model 能感知
    fn estimated_duration_secs(&self) -> u32 { 5 }  // 长耗时工具 UI 显示进度
}
```

### C.2 ToolError 分类（P1）
```rust
pub enum ToolErrorKind {
    PermissionDenied,
    NotFound,
    Timeout,
    ParseError,
    InvalidArgument,
    ExternalCommandFailed { exit_code: i32 },
    Other,
}
```
LLM 看到 kind 能决策（如 PermissionDenied → 告诉用户切 admin；NotFound → 不重试）。

### C.3 工具执行日志（P1）
- 每个 tool execution 写一行到 `X:\NeuroBoot\logs\tool-YYYYMMDD.log`
- 字段：timestamp / tool_name / args / safety / user_confirmed (Y/N) / exit_status / duration_ms
- 用户事后能审计 "Agent 在我的电脑上做过哪些操作"

### C.4 「只读模式」启动选项（P0）
- `neuroboot.exe --readonly` → 所有 Dangerous 工具直接拒绝（不弹弹窗）
- PE 启动菜单加「NeuroBoot (只读模式)」选项，避免误操作

---

## Part D：UI / UX

### D.1 视觉（P1）
- 长 JSON 工具结果折叠/展开（前 200 字默认展开，「查看完整」按钮）
- 「等待用户确认」时给更显眼的视觉（红框 + 屏幕轻微变暗背景）
- NeuroBoot 神启 ICO 图标（exe 资源、PE 桌面、Window title）
- 启动 splash screen（loading llama-server 时显示「神启正在唤醒...」）
- Dark / Light 主题切换
- 字号调节（适配 1080p / 1440p / 4K 不同屏）

### D.2 Markdown 渲染（P0）
- 当前 `ui.label` 渲染 plain text，模型用 `**bold**` / 列表 / 代码块时不渲染
- 改用 [egui_commonmark](https://crates.io/crates/egui_commonmark) 渲染
- 代码块加「复制」按钮 + 语法高亮（PowerShell / cmd / rust 等常见语法）

### D.3 历史对话（P1）
- 对话历史保存为 JSON 文件，PE 退出前提示 export 到 Ventoy 数据分区
- 启动时可选「加载上次对话」继续
- 多对话 session 切换（左侧栏 session 列表）

### D.4 错误处理（P1）
- LLM HTTP 错误 → 红字「重试」按钮（不只显示错误文字）
- Tool ToolError 按 kind 给不同建议提示
- Loading 中超过 60 秒显示「llama-server 似乎卡住，要不要重启？」+ 一键重启 server 按钮

---

## Part E：PE 镜像与分发

### E.1 镜像优化（P0）
- 加 **WinPE-FontSupport-ZH-CN.cab** OC（让 cmd / 系统对话框也显示中文，当前只 NeuroBoot 自带字体）
- 测试 **wgpu + WARP 软件 D3D12** 作为 Mesa 备选（Mesa 兼容性万一有问题时退路）
- 加 **WinPE-FMAPI.cab**（文件管理 API，winfr 可能需要）
- 加 **WinPE-EnhancedStorage.cab**（增强存储，部分 SED/eDrive 用）
- 启动画面定制：背景图 + 「NeuroBoot 神启」品牌 logo（替换默认 Windows 启动徽标）

### E.2 自动化（P1）
- `99-build-all.ps1` 加参数：`-Verbose` / `-DryRun` / `-SkipPhase N` / `-Clean`
- CI/CD：GitHub Actions 自动每周/月重 build ISO + 跑单测
- 多模型 GGUF 动态切换：models 放 Ventoy 数据分区（不打包进 ISO）→ ISO 体积降到 ~500 MB，模型可以用更大的 Qwen3-14B

### E.3 多硬件适配（P1）
- 收集常见笔记本网卡驱动（Intel Wi-Fi 6/7、Realtek、Killer、MediaTek、Qualcomm）打包到 `pe-build/drivers/`，DISM /Add-Driver 集成
- ARM64 build（Mesa-dist-win 含 ARM64 binaries，eframe 也支持 ARM64）—— 给 Surface Pro X / Lenovo ThinkPad X13s 用
- 不同 GPU 厂商（NVIDIA / AMD / Intel）的 Mesa 兼容性 matrix 测试

### E.4 USB 部署优化（P1）
- `setup-new-usb.ps1` 加 health check（写完后随机读 1 MB 验证 USB 写入完整性）
- 可选「写入多个 ISO」（NeuroBoot + Win11 安装 + MemTest86 + Hiren's BootCD），Ventoy 菜单多选

---

## Part F：安全与合规

### F.1 Agent 行为约束（P0）
- System prompt 强化拒绝明显违法/破坏性请求（如「帮我删除系统盘所有文件」「禁用 Windows Defender 服务」）
- 高危关键词检测（路径含 `system32` / `Windows` 的删除一律拦截，不走模型）
- 工具调用频率限制（防 Agent 滥调，1 分钟内同 tool 调用 > 10 次告警）

### F.2 签名（P2）
- 自签名 NeuroBoot.exe（避开 SAC 阻塞）—— 自签名 + 导入 Trusted Publishers，PE 里能跑
- 探索 SignTool + EV 证书（公开发行需要）
- WinPE 镜像签名（让 Secure Boot 也能启动，免去用户禁 Secure Boot 的步骤）

### F.3 隐私（P1）
- 工具调用结果不外传（即使用云端 A 端点，敏感字段如 Computer Name、MAC、Serial 在发请求前 redact）
- 「隐私模式」启动选项：所有 tool result 在发 LLM 前 mask 敏感字段

---

## Part G：性能优化

### G.1 模型（P1）
- 测试 **Qwen3-1.7B Q4_K_M**（~1.1 GB）—— RAM 紧张机器友好（4~8 GB RAM），ISO 体积更小
- 测试 **Qwen3-8B Q4_K_M** / **Qwen3-14B Q4_K_M** —— 云端 A 端点跑，对比体验
- 量化对比：Q4_K_M vs Q5_K_M vs Q6_K vs Q8_0，找质量/速度甜点
- 测试 **Phi-4 / Llama 3.2** 等竞品作为备选

### G.2 启动速度（P0）
- 当前 startnet.cmd 写死 `timeout 60` 等 llama-server 加载 —— 浪费时间
- 改用 **healthcheck loop**：循环 curl http://127.0.0.1:8080/health 直到 200 OK 或超时 120s
- 并行启动：NeuroBoot GUI 先开（显示 splash），server 加载完后 UI 自动激活输入框

### G.3 模型加载优化（P1）
- llama-server `--mlock`（锁 mmap 到内存防换页）—— RAM 充足时性能更稳
- 实验 `--no-mmap`（小内存机器禁 mmap 反而快？）
- 实验 `-fa` (flash attention) 提升长 context 性能

---

## Part H：文档与测试

### H.1 文档增强（P1）
- **用户手册**（终端用户视角）："我的电脑蓝屏了，怎么用 NeuroBoot 排查？"
- **工具参考手册**：每个 tool 的 schema + 示例 prompt + 示例输出
- **故障决策树**：症状 → 推荐工具组合 → 解读结果
- 关键文档**中文版**（当前 BUILD.md / KNOWN-ISSUES.md 是中英混排）

### H.2 测试覆盖（P1）
- Rust 单测覆盖率（当前只 `agent/truncate.rs` 3 个测试） → 目标 50%+
- 集成测试（mock llama-server 用 wiremock 类 crate 模拟回复 + tool_calls，测整轮 agent loop）
- Tool trait 实现的契约测试（每个 tool 必须返回合法 JSON / 不超时 / safety class 正确）
- 端到端：Ventoy ISO 在 QEMU 自动 boot + 截图对比

### H.3 错误信息本地化（P2）
- 当前所有错误（PowerShell stderr、Rust Err、Agent 「出错」）混合中英文
- 错误信息 / 日志统一中文（用户）+ 英文（开发者 log）双语

---

## Part I：实施优先级 / 路线图

### v1.0.1+ 已完成（2026-05-24 真测后追加，超出原 v2 路线图）
- [x] **healthcheck-based startup** —— PE startnet.cmd 内嵌 PS 探测 /health（原 P0 #3）
- [x] **WinPE-FontSupport-ZH-CN.cab** —— cmd / 系统对话框中文显示（原 P0 #7）
- [x] **端点 A+C UI 配置面板** —— ⚙ 设置面板 + config.json 持久化（原 P1）
- [x] CRT redist 同目录拷贝修复 llama-server PE 闪退（原非路线图，真测发现）
- [x] 中文输入兜底：6 快捷按钮 + U 盘 prompts.txt 下拉（原非路线图）
- [x] **vision 多模态**：「+ 图片」按钮 + OpenAI vision schema + VL 模型检测（原非路线图）
- [x] **状态栏**：时钟 / 内存 / 本地 IP（原非路线图）
- [x] **系统启动器**：cmd / 文件管理器按钮（原非路线图）
- [x] **电源控制**：重启 / 关机 / 退出按钮 + 确认弹窗（原非路线图）

### v2.0 已完成 Stage A（2026-05-24）
- [x] **Stage A1 Markdown 渲染** —— egui_commonmark 0.23，Assistant CommonMarkViewer，User 纯文本，Tool 等宽
- [x] **Stage A2 工具集扩充 (8 新工具)** —— list_partitions / list_volumes / read_ip_config / list_network_adapters / list_processes_top / list_services / list_minidumps / list_recent_shutdowns
- [x] **Stage A 副产物** —— tools/ps_helper.rs 抽 run_ps + run_ps_json_array，新工具一律走 helper

### v2 实施路线（2026-05-24 调研后确定，**已排除微软 Phi / QMR / Foundry Local**）

> 详细调研背景见 **[RESEARCH-2026-05.md](RESEARCH-2026-05.md)**
> 总工作量：5.5~9 天；推荐每会话做 1~2 个 stage + 1 次 ISO 重 build

#### Stage 1：Agent 基础健壮性 ⚡ 性价比之王 (~0.5 天)
**理由**：纯改 prompt / 配置 / 换模型文件，无代码风险；调研一致认定收益巨大；给后续所有 stage 立标准。

- [ ] 1.1 **重写 system prompt**：从当前 ~150 字扩到 800~1500 token；markdown 结构化（Role / Tools / Constraints / 输出格式 / 1-2 个 few-shot）；关键加 PE 环境约束段（"你跑在 Windows PE 救援环境，磁盘可能损坏，X: 是 ramdisk 只读，服务不一定可用，不要假设主系统行为"）
- [ ] 1.2 **12 个工具 description 按 [Anthropic spec](https://www.anthropic.com/engineering/writing-tools-for-agents) 重写**：每个加「When to use」段；list 类工具加可选 `format: "concise"|"detailed"`；显式参数名；error 给可操作指引
- [ ] 1.3 **system prompt 加高危关键词拦截规则**：path 含 `system32` / `Windows` / `Program Files` 的删除请求 → 模型层先拒，不调 delete_path
- [ ] 1.4 **量化升级 Qwen3-4B Q4_K_M → Q5_K_M** (~3 GB) —— 4B 小模型 Q4 是下限，Q5 在 tool-calling 上肉眼可见提升。仅换 GGUF 文件 + 改 startnet.cmd 引用
- [ ] 1.5 **startnet.cmd 加 `--no-mmap` + `-t <物理核数>`** —— U 盘 IO 友好 + 超线程不拖累矩阵运算
- [ ] 1.6 cargo test + dumpbin verify + commit + push (无 ISO build；Q5 模型换文件单独 build)

#### Stage 2：流式 SSE 输出 🔥 用户最痛 (~0.5~1 天)
**理由**：当前 `stream: false` → 长答复 30~60s 卡屏；改 agent loop 核心路径，单独成阶段防回归。

- [ ] 2.1 `llm/client.rs` 改 reqwest blocking 一次性返回 → SSE EventSource reader（`reqwest-eventsource` crate）
- [ ] 2.2 `agent/mod.rs` 改 worker：边读 chunk 边 send `AgentEvent::TokenChunk(s)`；tool_calls 跨 chunk 按 `index` 在 HashMap 累积；只有 `finish_reason: "tool_calls"` 时才 dispatch
- [ ] 2.3 **关键兼容性兜底**：llama.cpp build 8233+ 的 `tool_calls[].function.arguments` 输出 JSON object 而非 string（[issue #20198](https://github.com/ggml-org/llama.cpp/issues/20198)）。Rust 解析双形态都吃
- [ ] 2.4 UI 端：current assistant message append chunk + `ctx.request_repaint()` 触发增量重绘
- [ ] 2.5 Markdown 渲染：实测卡顿时换 `mdstream` crate（增量解析）
- [ ] 2.6 加「停止生成」按钮（Cancellation token → worker poll → 中断 reqwest）
- [ ] 2.7 cargo test + commit + push + ISO 重 build

#### Stage 3：tool_result clearing + 工具执行日志 (~0.5 天)
**理由**：替代当前整 turn truncation，对小模型最稳；audit trail 让用户事后能复盘。

- [ ] 3.1 `agent/truncate.rs` 改写：保 system + 保最近 N（默认 4）个 tool_use 完整 + 老 tool_result 替换成 `[cleared, can re-call]` 占位符
- [ ] 3.2 新加 `tools/audit_log.rs`：每次 tool execute 写一行 JSONL 到 `X:\NeuroBoot\logs\tool-YYYYMMDD.jsonl`，字段 `{ts, tool, args, safety, user_confirmed, exit_status, duration_ms, result_summary}`
- [ ] 3.3 UI 顶栏加「查看日志」按钮 → 打开 `X:\logs\` 在 cmd 里 `type` 文件
- [ ] 3.4 ToolError 分类 enum（PermissionDenied / NotFound / Timeout / ParseError / InvalidArgument / ExternalCommandFailed / Other）
- [ ] 3.5 cargo test + commit + push + ISO 重 build

#### Stage 4：危险工具 + 只读模式 + 数据保护 💎 救援核心 (~1 天)
**理由**：NeuroBoot 真正成为「PE 救援盘」的关键；当前只有 1 个危险工具。

- [ ] 4.1 **5 个新 dangerous 工具**（每个走确认弹窗）：
  - [ ] `run_chkdsk_fix` (`chkdsk <drive> /f /r`)
  - [ ] `run_sfc_scannow` (`sfc /scannow`)
  - [ ] `run_dism_restorehealth` (`DISM /Online /Cleanup-Image /RestoreHealth`)
  - [ ] `defender_offline_scan` (`MpCmdRun.exe -Scan -ScanType 2 -BootSectorScan`) —— **填补 ESET/Norton/Bitdefender 救援盘 EOL 后的真空地带**
  - [ ] `bootrec_rebuild_bcd` (`bootrec /rebuildbcd`)
- [ ] 4.2 **`delete_path` 改 Aider 风格**：move to `X:\trash\<timestamp>\<orig-name>`；UI 加「清空 trash」按钮；模型看不见这层包装
- [ ] 4.3 **加 `--readonly` 启动开关**：所有 dangerous 工具直接拒（不弹窗）；顶栏显示「只读模式」徽章
- [ ] 4.4 Ventoy 启动菜单加「NeuroBoot（只读模式）」选项
- [ ] 4.5 高危关键词 pre-check：tool registry 加层，args 里 path 含 `system32` 等直接拒
- [ ] 4.6 cargo test + commit + push + ISO 重 build

#### Stage 5：本地视觉模型 Qwen3-VL-2B 🎯 差异化 (~1 天 + PE 真测)
**理由**：把 vision 能力从「依赖云端 VL」拓到「本地 + 中文 + 离线」；NeuroBoot 真正差异化护城河。

- [ ] **5.0 预研**：在主开发机 CPU 上对比 **MiniCPM-V 4.6 (1.3B Q4 529 MB)** vs **Qwen3-VL-2B (Q4 1.11 GB + mmproj 700 MB)**：准备 5 张中文样本图（BIOS / BSOD / 设备管理器 / 错误对话框 / 截图），实测中文 OCR 准确率（人工评分）+ CPU 推理时间 + RAM 峰值。结果决定 5.1 用哪个
- [ ] 5.1 升级 ISO 内 llama-server 到 **b6907+**（Qwen3-VL PR #16780 + 性能修复）
- [ ] 5.2 下载放入 ISO 的视觉模型 GGUF + mmproj
- [ ] 5.3 Rust 端 `llm/endpoint.rs` 加 `local-vl` 端点，base_url `http://127.0.0.1:8081/v1`（避开 8080）
- [ ] 5.4 实现 **lazy spawn**：vision llama-server 不预启，用户首次上传图片才起 → 5 分钟空闲自动回收
- [ ] 5.5 settings_dialog.rs 默认 VL 端点改本地（保留云端 fallback）
- [ ] 5.6 `dumpbin /DEPENDENTS` 验证新 llama-server.exe 不依赖 PE 缺失 DLL
- [ ] 5.7 PE 真机 benchmark：5 张样本图实测端到端响应时间，记录 `docs/vl-benchmark.md`
- [ ] 5.8 cargo test + commit + push + ISO 重 build（ISO 增量 ~+1.6 GB）

#### Stage 6：救援旗舰工具集 🏆 功能补全 (~1 天)
**理由**：让 NeuroBoot 进入「传统 PE 救援盘」功能完整度；填补 Linux 救援盘的核心工具空白。

- [ ] 6.1 打包 **NTPWEdit**（~500 KB portable）到 `pe-build/payload/tools/` + 新加 `reset_local_admin_password` AI 工具（dangerous，确认弹窗）—— PE 救援盘旗舰功能
- [ ] 6.2 打包 **TestDisk + PhotoRec for Windows**（~5 MB portable，GPL v2+）+ 新加 `winfr_recover_regular` / `testdisk_scan_partition` AI 工具
- [ ] 6.3 打包 **smartmontools**（smartctl.exe，~5 MB，GPL）+ 新加 `read_smart` AI 工具
- [ ] 6.4 更新 NOTICE 文件追加这 3 个工具的 attribution
- [ ] 6.5 docs/BUILD.md 加这些 portable 工具的下载步骤
- [ ] 6.6 cargo test + commit + push + ISO 重 build（增量 ~+12 MB）

#### Stage 7：UX 升级 + skill 系统 (~0.5~1 天)
- [ ] 7.1 顶栏加「**一键全面检查**」按钮（参考 HP AI Companion Perform tab）→ 并行触发 8~10 个只读工具 → 结构化报告
- [ ] 7.2 **skill 系统**（轻量版 Claude Code skill）：启动时扫 U 盘 `X:\NeuroBoot\skills\*.md` + ISO 内置 6 个 skill（`/diagnose-boot.md` / `/diagnose-network.md` / `/diagnose-bsod.md` / `/recover-files.md` / `/scan-malware.md` / `/reset-password.md`）
- [ ] 7.3 **取证模式**（WinFE-style）：`--forensic` 启动开关 + Ventoy 菜单选项 → 禁所有写盘工具 + 强制 `--readonly`
- [ ] 7.4 UI 视觉：dangerous 工具确认弹窗加红框 + 屏幕背景轻微变暗
- [ ] 7.5 cargo test + commit + push + ISO 重 build

#### Stage 8：MCP 协议 server 模式 🌐 长期布局 (~1~2 天)
**理由**：MCP 是 Anthropic + Microsoft + OpenAI 共推的开放协议（**不是微软专属**）；NeuroBoot 暴露 12+ 工具成 MCP server，未来 Claude Desktop / Cline / Continue.dev 都能调

- [ ] 8.1 评估 `mcp-rust-sdk` crate 或 `rmcp` 官方 Rust SDK 成熟度
- [ ] 8.2 NeuroBoot 加 `--mcp-server` 启动模式：暴露 stdio + http transport
- [ ] 8.3 12+ 工具自动暴露为 MCP tools（复用现有 Tool trait + parameters_schema）
- [ ] 8.4 文档：如何在 Claude Desktop 配置 NeuroBoot MCP server / 如何用 Claude Code 通过 MCP 调 NeuroBoot 工具
- [ ] 8.5 cargo test + commit + push

### 已明确**不做**的方向（基于 2026-05 调研）

- [N] **Microsoft Phi / Phi Silica / Foundry Local** —— 锁 Copilot+ PC NPU + 中国不可用 + PE 不兼容
- [N] **Microsoft QMR 集成** —— QMR 不是 LLM，跑在 WinRE 不是 WinPE，本质规则引擎
- [N] **SmolVLM 系列** —— 完全不支持中文，PE 中文用户场景核心需求失败
- [N] **多 agent 编排（AutoGen / CrewAI / MetaGPT）** —— 单用户单任务不需要
- [N] **Background agent 并行** —— PE 内存紧 + 软件渲染 OOM
- [N] **NPU first 路线** —— NeuroBoot CPU first 是核心护城河
- [N] **完整 Claude Code skill watch + 跨 session memory** —— PE 重启即 wipe

### v2.1 重要（P1，待 v2.0 Stage 1~8 完成后再评估）
- [ ] Sysinternals / BlueScreenView 打包
- [ ] Tool trait 扩展（requires_admin / category / version）
- [ ] 历史对话保存（U 盘导出）
- [ ] 多硬件驱动适配
- [ ] 单测覆盖率提升（当前 34 测试 → 目标 50%+ 覆盖率）

### v2.2 后续（P2）
- [ ] ARM64 build
- [ ] 代码签名
- [ ] 隐私模式（敏感字段 redact 后发 LLM）
- [ ] CI/CD 自动化（GitHub Actions 周期 build ISO + 跑 cargo test）
- [ ] 用户手册 / 工具参考手册
- [ ] **语音输入**（远程录音上传 → 云端 STT；PE 本地录音受 ADK 无 audio stack 限制）
- [ ] **应用层拼音 IME**（rime-luna-pinyin 词典 ~2.4 MB，~600 行 Rust）

---

## Part J：调研来源（Sources）

WebSearch 14 路调研覆盖的关键信息源：

### Windows 内置工具
- [Use Bootrec.exe in the Windows RE — Microsoft Support](https://support.microsoft.com/en-us/topic/use-bootrec-exe-in-the-windows-re-to-troubleshoot-startup-issues-902ebb04-daa3-4f90-579f-0fbf51f7dd5d)
- [PnPUtil Command Syntax — Microsoft Learn](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax)
- [Manage Microsoft Defender Antivirus via Command Line — Microsoft Learn](https://learn.microsoft.com/en-us/defender-endpoint/command-line-arguments-microsoft-defender-antivirus)
- [Windows File Recovery — Microsoft Support](https://support.microsoft.com/en-us/windows/windows-file-recovery-61f5b28a-f5b8-3cc2-0f8e-a63cb4e1d4c4)
- [Get-PhysicalDisk (Storage) — Microsoft Learn](https://learn.microsoft.com/en-us/powershell/module/storage/get-physicaldisk)
- [taskkill — Microsoft Learn](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/taskkill)
- [Windows PE (WinPE) — Microsoft Learn](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/winpe-intro)

### 第三方诊断工具
- [Smartmontools (smartctl) — Official Site](https://smartmontools.com/)
- [BlueScreenView — NirSoft](https://www.nirsoft.net/utils/blue_screen_view.html)
- [Sysinternals Suite — Microsoft Learn](https://learn.microsoft.com/en-us/sysinternals/downloads/sysinternals-suite)
- [TestDisk & PhotoRec — CGSecurity](https://www.cgsecurity.org/wiki/TestDisk)
- [MemTest86 — Official](https://www.memtest86.com/)
- [WindowsPEBasicEnhanced — GitHub](https://github.com/thedoggybrad/WindowsPEBasicEnhanced)

### 教程与综合参考
- [How to Repair Windows 11 with Command Prompt — EaseUS](https://www.easeus.com/partition-master/repair-windows-11-cmd.html)
- [Windows-Maintenance-Tool — GitHub](https://github.com/ios12checker/Windows-Maintenance-Tool)
- [Networking Commands For Troubleshooting Windows — GeeksforGeeks](https://www.geeksforgeeks.org/computer-networks/networking-commands-for-troubleshooting-windows/)
- [Using smartctl for hard drive diagnostics — Liquid Web](https://www.liquidweb.com/help-docs/performance/server-optimization/using-smartctl/)
- [Windows OS Hub — Disks and Partitions Management with PowerShell](https://woshub.com/disks-partitions-management-powershell/)

---

## Part K：变更管理

本文档随实施进度更新：每个工具 / 改进项完成后在 checkbox 打勾 + 在 git commit message 引用对应章节号。

**Last updated**: 2026-05-24（v1.0.1+ 完成后状态同步：3 项原 P0/P1 已完成 + 6 项原非路线图功能已实施）

**Next review**: v2.0 剩余 P0（流式输出 / Markdown / 扩工具 / smartmontools / 只读模式 / system prompt 强化）完成 50% 时回顾优先级调整

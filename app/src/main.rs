//! NeuroBoot 神启 —— 主程序入口
//!
//! 阶段 v1.0.1：U 盘真测反馈紧急修复 ——
//! - 中文输入兜底：快捷问题按钮行 + U 盘 prompts.txt 下拉框
//! - 在线 AI 配置 UI：齿轮按钮弹设置面板，可保存到 U 盘 config.json
//! - endpoint 探测增强：5s 超时 + HEAD /v1/models 优先，env var > config.json > 默认
//!
//! v1.0 baseline：
//! - A+C 双备：探测云端 A，可用则 active = 云端，否则 active = 本地
//! - 已注册 4 个工具：3 个 safe + 1 个 dangerous（delete_path）
//! - dangerous 工具触发模态确认弹窗，用户必须点「确认执行」才会动手

mod agent;
mod llm;
mod mcp;
mod tools;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use agent::{
    spawn_agent_request, AgentEvent, AgentJob, ConfirmationRequest, ConfirmationResponse,
    PlanProposalRequest, PlanResponse,
};
use eframe::egui;
use llm::config_file::{is_vl_model, save_to_first_writable_drive, ConfigFile};
use llm::endpoint::{detect_endpoints, DetectedEndpoints, EndpointConfig};
use tools::ToolRegistry;
use ui::{
    draw_power_confirmation_dialog, draw_settings_dialog, install_chinese_fonts, launch_cmd,
    launch_file_manager, load_path_as_attached, load_skill_body, open_log_dir, pick_image_files,
    render_message, scan_skills, scan_user_prompts, AttachedImage, ChatMessage, CommonMarkCache,
    PowerAction, SettingsAction, SettingsBuffer, SkillSummary, StatusBarState, UserPrompt,
};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080";
const DEFAULT_MODEL: &str = "qwen3-4b-instruct";

/// 默认 system prompt（v2 Stage 1，~1200 token）。
///
/// 结构化为 markdown 五段：身份 / 运行环境约束 / 工具使用准则 / 危险操作纪律 / 回答格式。
/// 调研依据见 docs/RESEARCH-2026-05.md 第五节（Agent 架构关键发现）：
/// - Anthropic context engineering cookbook 建议 system prompt 800~1500 token + 结构化
/// - 小模型（4B）对 system prompt 结构敏感度高于参数量
/// - 高危关键词应在 prompt 层先拒，不让模型决策再调工具
const DEFAULT_SYSTEM_PROMPT: &str = r##"# 身份

你是 **NeuroBoot 神启**，一个运行在 **Windows PE 救援环境** 里的本地 AI 助手。
你的用户是中文 IT 维护人员或遇到故障想自救的普通用户，他们通过 U 盘启动到 PE 后跟你对话。

---

# 运行环境（重要约束）

你**不**跑在主系统上，而是跑在 PE（Preinstallation Environment）里：

- **PE 是 ramdisk**：`X:` 盘是临时的 RAM 盘，关机即丢；不要假设可以写持久数据
- **磁盘可能损坏**：用户来 PE 通常是因为主系统出了问题（蓝屏、起不来、文件丢失、密码忘了、感染病毒）。`C:` / `D:` 等盘符可能挂载失败、文件系统损坏、或被 BitLocker 加密
- **服务不一定可用**：很多 Windows 服务在 PE 里没启动（如 Print Spooler、Windows Update、Cortana）；不要建议「重启 X 服务」类的修复在 PE 里跑
- **网络可能没有**：PE 默认不连 Wi-Fi（除非用户手工 `wpeutil InitializeNetwork`），有线网卡也未必连了网
- **PE 不带常见软件**：没有浏览器、没有 Office、没有 .NET 完整版、**没有蓝牙 stack**、**没有 IME（中文输入法）**
- **`X:\NeuroBoot\` 路径有这些资源**：`neuroboot.exe` 本体、`llama-cpp\` 推理服务、`models\` GGUF 模型、`logs\` 工具执行日志

---

# 工具使用准则

你有一组诊断和修复工具可调用（每个工具的 description 写了具体用途）：

1. **优先调工具，不要凭训练数据回答**。用户问「我有多少硬盘」「最近有哪些蓝屏」时，**永远先调 `list_disks` / `read_event_log_errors` 等查实情**，绝不编造
2. **没合适工具时明确说**「NeuroBoot 当前没有查 X 的工具」，不要瞎猜
3. **工具结果可能很长（stdout / JSON）**：你看到的是完整数据，但回复用户时**只摘取关键字段**。例如硬盘列出 5 块但用户只关心 D 盘异常，就只讲 D 盘
4. **工具结果是空数组（`[]`）合法**：表示没数据（如「最近 24 小时无蓝屏」），不是错误
5. **可以多轮调多个工具**：先调 safe 的只读工具收集证据 → 推理可能原因 → 再决定要不要调 dangerous 工具修复
6. **诊断思路**：症状 → 列证据 → 推断可能原因 → 给修复建议（让用户选要不要执行）

---

# 危险操作纪律（强约束）

**危险工具**（description 含「dangerous / 不可撤销 / 修复 / 删除 / 格式化 / 修改」的）会触发 UI 确认弹窗：

- **拒绝就是拒绝**：用户点「取消」后，**不要重试相同操作**，问用户「是否换个方式」
- **路径双重审查**：调用任何含路径参数的工具前，**先在脑里检查**：路径是否含 `C:\Windows`、`C:\Windows\System32`、`C:\Program Files`、`C:\Program Files (x86)`、`C:\ProgramData`？如果是 —— **拒绝调用并告诉用户「这是系统目录，不能删」**，建议改去 `Users\<name>\` 下找
- **整盘格式化绝不调**：哪怕用户说「帮我格式化整个 C 盘」，也要先反问「你确定吗？所有数据会丢失，是否想保留某个分区？」
- **诊断阶段绝不调危险工具**：用户问「电脑慢」时，应该调只读工具（list_processes_top / list_services）找原因，**不要直接建议**调 chkdsk / sfc / dism 等修复工具
- **dangerous 工具的参数要保守**：能用 readonly mode（如 `chkdsk /scan`）就不用写盘 mode（如 `chkdsk /f /r`）

---

# Plan Mode（v3.0 W3-4）

复杂任务**先提计划再执行**，用 `propose_plan` 工具。触发条件（任一）：

- 预计要调 **超过 2 个工具** 才能完成（如蓝屏诊断要 4+ 工具：list_minidumps + analyze_minidump + read_event_log_errors + read_recent_installs）
- 计划里**任何一步用 dangerous 工具**（chkdsk / sfc / dism / bootrec / delete_path / 等）
- 用户**显式说**「先告诉我你的计划」「先看一下你打算怎么做」
- 已用 `load_skill` 拿到剧本 → 把剧本的 step 列表直接作为 plan 提交

**不要 plan**：单工具回答（「我有几块硬盘」→ 直接 list_disks）；纯文字闲聊。

**propose_plan 流程**：
1. 调 `propose_plan(summary, steps=[{tool, args_preview, why, safety?}])` —— UI 弹窗给用户审
2. 用户 **Approve** → tool 返回「已批准」→ 你按 steps 依次调实际工具，完成后给中文总结
3. 用户 **Reject** → tool 返回「拒绝」→ **不要原样重提同一 plan**，重新思考或问用户为什么拒绝
4. **批准后不要再次 propose_plan**，直接调实际工具

---

# 回答格式

- **用中文**，简明扼要，不啰嗦
- **支持 Markdown**：模型回复会经 CommonMark 渲染。可用 `**粗体**`、`*斜体*`、`代码块`、列表、表格、引用
- **结构化复杂回复**：3 步以上的修复方案用编号列表；多块硬盘对比用表格
- **代码块语言标签**：PowerShell 命令用 powershell 标签，cmd 命令用 cmd 标签，让 UI 可以高亮
- **不要假装** 自己能跑用户写的命令 —— 你只能调你的工具集；要让用户跑命令时，写出命令让用户复制粘贴
- **避免技术黑话**：用户不一定懂「event log id 41 是 kernel-power」这种行话，要解释「= 突然断电或长按电源键关机」

---

# 示例对话片段

**用户**：「我电脑昨天突然蓝屏，重启后好了，怕再蓝屏」

**你**（好的回答）：
1. 调 `read_event_log_errors` 查最近 24 小时严重错误
2. 调 `list_minidumps` 看是否生成了崩溃 dump 文件
3. 调 `list_recent_shutdowns` 看异常关机事件
4. 综合结果告诉用户：「**最近一次蓝屏发生在昨天 14:23**，原因是 *Kernel-Power 41*（一般是突然断电或硬件不稳）。dump 文件存在 `C:\Windows\Minidump\xxx.dmp`。建议你 ① 检查电源线/插座是否松动 ② 跑一次内存检测（`mdsched.exe`）...」

**你**（不好的回答）：
- ❌ 凭印象说「可能是显卡驱动问题」（没看证据）
- ❌ 直接调 `delete_path C:\Windows\Minidump\*` 想「清理」（dump 文件正是诊断证据，删掉就废了）
- ❌ 跳过证据收集直接建议跑 `chkdsk C: /f /r`（用户没问到这一步）
"##;

/// v2 Stage 7.1：「一键全面检查」预设 prompt。点按钮注入；agent 跑 8~10 个只读工具组合。
const FULL_CHECK_PROMPT: &str = "请帮我做一次系统全面体检，**并行**调用以下工具，最后给出结构化报告：\n\
\n\
1. `read_system_info` —— 系统配置和最后启动时间\n\
2. `list_disks` + `list_partitions` + `list_volumes` —— 硬盘+分区+卷状态\n\
3. `read_ip_config` + `list_network_adapters` —— 网络配置\n\
4. `list_processes_top` (sort_by='memory', top_n=10) —— 内存占用 top 10\n\
5. `list_services` (status='Running') —— 运行中的服务概览\n\
6. `read_event_log_errors` (hours=48, max_events=20) —— 最近 48 小时严重错误\n\
7. `list_minidumps` —— 蓝屏 dump 文件清单\n\
8. `list_recent_shutdowns` (max_events=15) —— 最近关机/重启事件\n\
\n\
报告格式（markdown）：\n\
\n\
## 🟢 健康项\n\
列出运行正常的子系统（CPU/RAM/磁盘/网络/服务等）。\n\
\n\
## ⚠ 需关注\n\
列出有 warning 但还可工作的（如磁盘 > 85%、内存占用高、个别错误事件等）。\n\
\n\
## 🔴 异常 / 建议立刻处理\n\
列出 Health=Unhealthy / 频繁蓝屏 / 系统目录错误等严重问题。\n\
\n\
## 下一步建议\n\
每个 🔴 / ⚠ 项给一个**具体可行**的下一步动作（如「跑 sfc /scannow」「检查电源」「打开 X:\\\\Windows\\\\Minidump 看 dump」），\
**但只是建议** —— 不主动调任何 dangerous 工具，让用户决定。";

/// 内置快捷问题（PE 无 IME 中文输入兜底）。点按钮把预设 prompt 填入输入框。
const QUICK_PROMPTS: &[(&str, &str)] = &[
    (
        "电脑蓝屏",
        "我的电脑最近频繁蓝屏。请帮我:\n1. 列出最近 24 小时的系统错误事件\n2. 列出 minidump 文件（如果有工具）\n3. 给出排查方向",
    ),
    (
        "硬盘问题",
        "我担心硬盘出问题。请帮我:\n1. 列出所有硬盘和分区\n2. 报告任何异常\n3. 给出下一步建议",
    ),
    (
        "网络故障",
        "我的电脑连不上网。请帮我:\n1. 查 ipconfig 等网络配置\n2. 检查关键服务状态\n3. 给出排查方向",
    ),
    (
        "启动慢",
        "我的电脑开机很慢。请帮我:\n1. 列出开机自启程序\n2. 列运行中的服务\n3. 给出优化建议",
    ),
    (
        "找回误删",
        "我误删了一些文件想找回。请告诉我:\n1. NeuroBoot 当前能用什么工具尝试恢复\n2. 我应该提供哪些信息（盘符、文件类型等）",
    ),
    (
        "系统修复",
        "我的 Windows 系统起不来了。请帮我:\n1. 检查启动配置 BCD\n2. 列出系统错误事件\n3. 给出修复步骤",
    ),
];

fn main() -> eframe::Result<()> {
    // v2 Stage 8: --mcp-server 模式 —— 不启 GUI，stdio JSON-RPC 暴露 safe 工具
    if std::env::args().any(|a| a == "--mcp-server") {
        run_as_mcp_server();
        // 这条路径不返回 eframe::Result（run_as_mcp_server 内 std::process::exit）
        // 但 Rust 类型系统不知道，所以下面无脑用 Ok(())
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("NeuroBoot 神启"),
        ..Default::default()
    };

    eframe::run_native(
        "NeuroBoot",
        options,
        Box::new(|cc| {
            install_chinese_fonts(&cc.egui_ctx);
            Ok(Box::<NeuroBootApp>::default())
        }),
    )
}

/// v2 Stage 8 入口：注册全部 safe 工具后跑 MCP stdio 服务器；阻塞到 stdin EOF。
fn run_as_mcp_server() {
    // 跟 GUI 模式一样的 safe 工具集（dangerous 由 mcp.rs 内部过滤）
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(tools::safe::list_disks::ListDisks));
    registry.register(Box::new(tools::safe::read_system_info::ReadSystemInfo));
    registry.register(Box::new(tools::safe::read_event_log_errors::ReadEventLogErrors));
    registry.register(Box::new(tools::safe::list_partitions::ListPartitions));
    registry.register(Box::new(tools::safe::list_volumes::ListVolumes));
    registry.register(Box::new(tools::safe::read_ip_config::ReadIpConfig));
    registry.register(Box::new(tools::safe::list_network_adapters::ListNetworkAdapters));
    registry.register(Box::new(tools::safe::list_processes_top::ListProcessesTop));
    registry.register(Box::new(tools::safe::list_services::ListServices));
    registry.register(Box::new(tools::safe::list_minidumps::ListMinidumps));
    registry.register(Box::new(tools::safe::list_recent_shutdowns::ListRecentShutdowns));
    registry.register(Box::new(tools::safe::read_smart::ReadSmart));
    registry.register(Box::new(tools::safe::extract_archive::ExtractArchive));
    registry.register(Box::new(tools::safe::analyze_minidump::AnalyzeMinidump));
    // v3.0 W1.5: Progressive disclosure tier 2 工具
    registry.register(Box::new(tools::safe::load_skill::LoadSkill));
    // v3.0 W3-4: Plan Mode 入口（MCP 模式下 execute() 返 placeholder，不阻塞）
    registry.register(Box::new(tools::safe::propose_plan::ProposePlan));
    // v3.0 W7: 4 个 PS 包装的新 safe 工具
    registry.register(Box::new(tools::safe::list_winre_status::ListWinreStatus));
    registry.register(Box::new(tools::safe::bitlocker_status::BitlockerStatus));
    registry.register(Box::new(tools::safe::find_large_files::FindLargeFiles));
    registry.register(Box::new(tools::safe::read_recent_installs::ReadRecentInstalls));
    registry.register(Box::new(tools::safe::lookup_error_code::LookupErrorCode));

    mcp::run_mcp_server(Arc::new(registry));
    std::process::exit(0);
}

struct NeuroBootApp {
    messages: Vec<ChatMessage>,
    input_buffer: String,
    system_prompt: String,
    tool_registry: Arc<ToolRegistry>,
    pending_response: Option<mpsc::Receiver<AgentEvent>>,
    /// 当前生效的端点配置（A 或 C）
    active: EndpointConfig,
    /// 备选端点配置（None = UI 不显示切换按钮）
    inactive: Option<EndpointConfig>,
    /// 当 Agent 想调 dangerous 工具时，UI 把请求存这里 + 渲染弹窗等用户决定
    pending_confirmation: Option<ConfirmationRequest>,
    /// v3.0 W3-4 Plan Mode：Agent 调 propose_plan 时存这里 + 渲染 plan modal
    pending_plan: Option<PlanProposalRequest>,
    /// 当前内存里的 config（合并了 env var 和 config.json）—— 设置面板初始值来源
    effective_config: ConfigFile,
    /// U 盘 prompts.txt 解析出的候选问题（启动时一次扫描）
    user_prompts: Vec<UserPrompt>,
    /// 设置面板是否打开
    show_settings: bool,
    /// 设置面板的可编辑表单状态
    settings_buffer: SettingsBuffer,
    /// 待确认的电源动作（重启/关机/退出）；Some 时显示对应确认弹窗
    pending_power_action: Option<PowerAction>,
    /// 状态栏（时钟/内存/IP）的缓存
    status_bar: StatusBarState,
    /// 当前正在输入的消息附带的图片（点「+ 图片」加，点 X 删，submit 后清空）
    attached_images: Vec<AttachedImage>,
    /// Markdown 渲染缓存（避免每帧重 parse Assistant 消息）
    md_cache: CommonMarkCache,
    /// v2 Stage 2 取消标志：UI 点「停止生成」会 set 为 true，worker 检测后中断流式读
    cancel_flag: Arc<AtomicBool>,
    /// v2 Stage 4.3 只读模式：true 时 dangerous 工具完全没注册；顶栏显示徽章警示
    readonly_mode: bool,
    /// v2 Stage 7.3 取证模式：蕴含 readonly + 额外限制（disk read-only mount 等由 PE 启动配置控制）
    forensic_mode: bool,
    /// v2 Stage 7.2 加载的 skill **summaries**（启动时扫一次；只有 frontmatter）。
    /// v3.0 W1.5：从 `Vec<Skill>` 改为 `Vec<SkillSummary>`，body 按需 lazy load
    /// （AI 调 load_skill 工具 或 用户手动激活时通过 load_skill_body 读）。
    skills: Vec<SkillSummary>,
    /// 当前激活的 skill 索引（None = 不注入 skill；Some(i) = 用 skills[i] 增量到 system prompt）
    active_skill_idx: Option<usize>,
}

impl Default for NeuroBootApp {
    fn default() -> Self {
        let DetectedEndpoints {
            active,
            inactive,
            effective,
            source_hint,
            probe_hint,
        } = detect_endpoints(DEFAULT_ENDPOINT, DEFAULT_MODEL);

        // v2 Stage 4.3 + 7.3: 检测 --readonly / --forensic CLI flag
        // forensic 蕴含 readonly（取证场景更严格）
        let forensic_mode = std::env::args().any(|a| a == "--forensic")
            || std::env::var("NEUROBOOT_FORENSIC")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        let readonly_mode = forensic_mode
            || std::env::args().any(|a| a == "--readonly")
            || std::env::var("NEUROBOOT_READONLY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        // 注册工具：v1 baseline + v2 Stage A safe + v2 Stage 4 dangerous
        // 只读模式：跳过所有 dangerous 工具的注册 —— 模型层就没法看到它们
        let mut registry = ToolRegistry::new();
        // v1 baseline (safe)
        registry.register(Box::new(tools::safe::list_disks::ListDisks));
        registry.register(Box::new(tools::safe::read_system_info::ReadSystemInfo));
        registry.register(Box::new(tools::safe::read_event_log_errors::ReadEventLogErrors));
        // v2 Stage A 新增 safe 工具
        registry.register(Box::new(tools::safe::list_partitions::ListPartitions));
        registry.register(Box::new(tools::safe::list_volumes::ListVolumes));
        registry.register(Box::new(tools::safe::read_ip_config::ReadIpConfig));
        registry.register(Box::new(tools::safe::list_network_adapters::ListNetworkAdapters));
        registry.register(Box::new(tools::safe::list_processes_top::ListProcessesTop));
        registry.register(Box::new(tools::safe::list_services::ListServices));
        registry.register(Box::new(tools::safe::list_minidumps::ListMinidumps));
        registry.register(Box::new(tools::safe::list_recent_shutdowns::ListRecentShutdowns));
        // v2 Stage 6 新增 safe 工具（外部 binary 缺失时返回 NotFound）
        registry.register(Box::new(tools::safe::read_smart::ReadSmart));
        // v3 Quick Win 2 + 3 新增 safe 工具
        registry.register(Box::new(tools::safe::extract_archive::ExtractArchive));
        registry.register(Box::new(tools::safe::analyze_minidump::AnalyzeMinidump));
        // v3.0 W1.5: Progressive disclosure tier 2 工具
        registry.register(Box::new(tools::safe::load_skill::LoadSkill));
        // v3.0 W3-4: Plan Mode 入口工具（Cline 风格）
        registry.register(Box::new(tools::safe::propose_plan::ProposePlan));
        // v3.0 W7: 4 个 PS 包装的新 safe 工具（配套 W2-3 skills）
        registry.register(Box::new(tools::safe::list_winre_status::ListWinreStatus));
        registry.register(Box::new(tools::safe::bitlocker_status::BitlockerStatus));
        registry.register(Box::new(tools::safe::find_large_files::FindLargeFiles));
        registry.register(Box::new(tools::safe::read_recent_installs::ReadRecentInstalls));
        // v3.0 W7: error code 查询（MVP 版用 hardcoded 表；W5-6 升级 RAG）
        registry.register(Box::new(tools::safe::lookup_error_code::LookupErrorCode));
        // dangerous 工具：只读模式下完全不注册
        if !readonly_mode {
            // v1 dangerous
            registry.register(Box::new(tools::dangerous::delete_path::DeletePath));
            // v2 Stage 4.1 新增 dangerous 工具
            registry.register(Box::new(tools::dangerous::run_chkdsk::RunChkdsk));
            registry.register(Box::new(tools::dangerous::run_sfc::RunSfcScannow));
            registry.register(Box::new(
                tools::dangerous::run_dism_restorehealth::RunDismRestoreHealth,
            ));
            registry.register(Box::new(
                tools::dangerous::defender_offline_scan::DefenderOfflineScan,
            ));
            registry.register(Box::new(
                tools::dangerous::bootrec_rebuild_bcd::BootrecRebuildBcd,
            ));
            // v2 Stage 6 新增 dangerous 工具（外部 binary 缺失时返回 NotFound）
            registry.register(Box::new(
                tools::dangerous::reset_local_admin_password::ResetLocalAdminPassword,
            ));
            registry.register(Box::new(
                tools::dangerous::testdisk_scan_partition::TestdiskScanPartition,
            ));
        }

        let user_prompts = scan_user_prompts();
        let prompts_hint = if user_prompts.is_empty() {
            String::new()
        } else {
            format!("\n从 U 盘加载了 {} 条候选问题，在输入框上方下拉框里选用。", user_prompts.len())
        };

        let endpoint_hint = if inactive.is_some() {
            format!("当前用「{}」，可在顶栏切换。", active.label)
        } else {
            "当前用本地端点。".to_owned()
        };

        let system_prompt = effective
            .system_prompt_override
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_owned());

        let welcome = format!(
            "你好，我是 NeuroBoot 神启。{endpoint_hint}\n\
             已注册 4 个工具：3 个只读诊断（硬盘 / 系统配置 / 系统日志错误）+ 1 个危险工具 delete_path。\n\
             - {source_hint}\n\
             - {probe_hint}\n\
             如要修改在线 AI 端点配置，请点顶栏齿轮按钮 ⚙。{prompts_hint}"
        );

        let settings_buffer = SettingsBuffer::from_config(&effective);

        Self {
            messages: vec![ChatMessage::assistant(welcome)],
            input_buffer: String::new(),
            system_prompt,
            tool_registry: Arc::new(registry),
            pending_response: None,
            active,
            inactive,
            pending_confirmation: None,
            pending_plan: None,
            effective_config: effective,
            user_prompts,
            show_settings: false,
            settings_buffer,
            pending_power_action: None,
            status_bar: {
                let mut s = StatusBarState::default();
                s.refresh_now(); // 首帧就有数据，免得显示「?」
                s
            },
            attached_images: Vec::new(),
            md_cache: CommonMarkCache::default(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            readonly_mode,
            forensic_mode,
            skills: scan_skills(),
            active_skill_idx: None,
        }
    }
}

impl NeuroBootApp {
    /// 交换 active 与 inactive endpoint。
    fn toggle_endpoint(&mut self) {
        if let Some(inactive) = self.inactive.take() {
            let prev_active = std::mem::replace(&mut self.active, inactive);
            self.inactive = Some(prev_active);
            self.messages.push(ChatMessage::assistant(format!(
                "（端点切换）现在使用：{} ({})",
                self.active.label, self.active.endpoint
            )));
        }
    }

    /// 把 settings_buffer 的修改写回 effective_config + 立刻应用到 active endpoint。
    ///
    /// 「仅本次会话」分支用：不改 active 当前在用的那个（避免对话进行中突然换端点），
    /// 而是更新 inactive；下次切换时生效。如果当前 active 是 local 而 buffer 改了 remote，
    /// 则把新 remote 配上去当 inactive。
    fn apply_settings_in_memory(&mut self) {
        self.settings_buffer.apply_to_config(&mut self.effective_config);

        // 重新生成 remote / local endpoint
        let new_remote = if self.effective_config.has_remote() {
            Some(EndpointConfig {
                endpoint: self.effective_config.remote_endpoint.clone(),
                model: if self.effective_config.remote_model.is_empty() {
                    "default".to_owned()
                } else {
                    self.effective_config.remote_model.clone()
                },
                api_key: if self.effective_config.remote_api_key.is_empty() {
                    None
                } else {
                    Some(self.effective_config.remote_api_key.clone())
                },
                label: self.effective_config.remote_label.clone(),
            })
        } else {
            None
        };

        let new_local = EndpointConfig {
            endpoint: self.effective_config.local_endpoint.clone(),
            model: self.effective_config.local_model.clone(),
            api_key: None,
            label: "本地".to_owned(),
        };

        // 决策：active 保持类型不变，把新配置套上去；inactive 同理
        let active_is_remote = self.active.label != "本地";
        let (new_active, new_inactive) = if active_is_remote {
            match new_remote {
                Some(r) => (r, Some(new_local)),
                None => (new_local, None), // 用户清空了 remote
            }
        } else {
            // active 是本地
            (new_local, new_remote)
        };

        self.active = new_active;
        self.inactive = new_inactive;

        self.messages.push(ChatMessage::assistant(format!(
            "（设置已应用，仅本次会话）当前 active = {} ({})",
            self.active.label, self.active.endpoint
        )));
    }

    /// 保存配置到 U 盘 + 重新探测（重新探测会更新 active/inactive，可能切换 active）。
    fn save_settings_and_reprobe(&mut self) {
        self.settings_buffer.apply_to_config(&mut self.effective_config);

        match save_to_first_writable_drive(&self.effective_config) {
            Ok(path) => {
                self.messages.push(ChatMessage::assistant(format!(
                    "（设置已保存）写入 {}\n下次启动也会自动加载此配置。正在重新探测端点...",
                    path.display()
                )));
            }
            Err(e) => {
                self.messages.push(ChatMessage::assistant(format!(
                    "（保存失败）{e}\n但本次会话已应用新配置。"
                )));
            }
        }

        // 重新探测 —— 注意 detect_endpoints 会再读 config.json
        // 但我们刚写了 config.json，所以读到的就是最新的
        let DetectedEndpoints {
            active,
            inactive,
            effective,
            source_hint: _,
            probe_hint,
        } = detect_endpoints(DEFAULT_ENDPOINT, DEFAULT_MODEL);
        self.active = active;
        self.inactive = inactive;
        self.effective_config = effective;
        self.messages
            .push(ChatMessage::assistant(format!("（重新探测）{probe_hint}")));
    }
}

impl eframe::App for NeuroBootApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_pending_response();

        let busy = self.pending_response.is_some();
        let waiting_confirm = self.pending_confirmation.is_some();

        // ----- 顶部：品牌 + endpoint 状态 + 切换按钮 + 设置按钮 -----
        egui::Panel::top("header").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("NeuroBoot 神启");
                ui.weak("·");
                ui.label(format!("{} ({})", self.active.label, self.active.endpoint));
                ui.weak(format!("· {} 个工具", self.tool_registry.len()));
                if self.forensic_mode {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 200, 120),
                        "· 🔬 取证模式",
                    );
                } else if self.readonly_mode {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 200, 120),
                        "· 🔒 只读模式",
                    );
                }
                // v2 Stage 7.2: 当前激活 skill 显示
                if let Some(i) = self.active_skill_idx {
                    if let Some(s) = self.skills.get(i) {
                        ui.colored_label(
                            egui::Color32::from_rgb(170, 170, 220),
                            format!("· {} ", s.name),
                        );
                    }
                }
                if let Some(alt) = &self.inactive {
                    if !busy {
                        if ui.small_button(format!("切到{}", alt.label)).clicked() {
                            self.toggle_endpoint();
                        }
                    }
                }
                if waiting_confirm {
                    ui.colored_label(egui::Color32::from_rgb(255, 180, 100), "· 等待你确认...");
                } else if busy {
                    ui.weak("· 正在思考...");
                }

                // 右对齐按钮组（右往左：退出 / 关机 / 重启 / 分隔 / 设置 / 文件 / cmd）
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("退出")
                        .on_hover_text("退出 NeuroBoot 程序，回到 PE 命令行")
                        .clicked()
                    {
                        self.pending_power_action = Some(PowerAction::ExitToCmd);
                    }
                    if ui
                        .small_button("关机")
                        .on_hover_text("wpeutil shutdown —— 关闭电脑")
                        .clicked()
                    {
                        self.pending_power_action = Some(PowerAction::Shutdown);
                    }
                    if ui
                        .small_button("重启")
                        .on_hover_text("wpeutil reboot —— 重启电脑")
                        .clicked()
                    {
                        self.pending_power_action = Some(PowerAction::Reboot);
                    }
                    ui.separator();
                    if ui
                        .small_button("⚙ 设置")
                        .on_hover_text("配置在线 AI 端点（OpenAI / DeepSeek 等）")
                        .clicked()
                    {
                        // 打开前用最新 effective_config 重新刷新 buffer
                        self.settings_buffer = SettingsBuffer::from_config(&self.effective_config);
                        self.show_settings = true;
                    }
                    if ui
                        .small_button("文件")
                        .on_hover_text("打开文件管理器（PE 没 explorer 时回落 cmd dir 列表）")
                        .clicked()
                    {
                        match launch_file_manager() {
                            Ok(r) => self.messages.push(ChatMessage::assistant(format!(
                                "（已启动 {}）{}",
                                r.program, r.note
                            ))),
                            Err(e) => self
                                .messages
                                .push(ChatMessage::assistant(format!("（启动失败）{e}"))),
                        }
                    }
                    if ui
                        .small_button("cmd")
                        .on_hover_text("打开新的 cmd 窗口（不退出 NeuroBoot）")
                        .clicked()
                    {
                        match launch_cmd() {
                            Ok(r) => self.messages.push(ChatMessage::assistant(format!(
                                "（已启动 {}）{}",
                                r.program, r.note
                            ))),
                            Err(e) => self
                                .messages
                                .push(ChatMessage::assistant(format!("（启动失败）{e}"))),
                        }
                    }
                    if ui
                        .small_button("日志")
                        .on_hover_text("查看工具执行审计日志（X:\\NeuroBoot\\logs\\）")
                        .clicked()
                    {
                        match open_log_dir() {
                            Ok(r) => self
                                .messages
                                .push(ChatMessage::assistant(format!("（日志）{}", r.note))),
                            Err(e) => self
                                .messages
                                .push(ChatMessage::assistant(format!("（日志打不开）{e}"))),
                        }
                    }
                });
            });
            // 状态栏：时钟 · 内存 · IP
            ui.add_space(2.0);
            self.status_bar.draw(ui);
            ui.add_space(4.0);
        });

        // ----- 底部：快捷按钮 + 用户 prompts 下拉 + 输入区 -----
        egui::Panel::bottom("input")
            .resizable(false)
            .show_inside(ui, |ui| {
                self.draw_quick_prompt_bar(ui, busy);
                self.draw_input_panel(ui, busy);
            });

        // ----- 中央：消息列表 -----
        let messages = &self.messages;
        let md_cache = &mut self.md_cache;
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for msg in messages {
                        render_message(ui, msg, md_cache);
                    }
                });
        });

        // ----- 危险工具确认弹窗（floating modal） -----
        self.draw_confirmation_dialog(ui.ctx());
        // ----- v3.0 W3-4: Plan Mode 审批弹窗 -----
        self.draw_plan_dialog(ui.ctx());

        // ----- 设置面板（modal） -----
        if self.show_settings {
            if let Some(action) = draw_settings_dialog(ui.ctx(), &mut self.settings_buffer) {
                self.show_settings = false;
                match action {
                    SettingsAction::SaveAndReprobe => self.save_settings_and_reprobe(),
                    SettingsAction::ApplyOnce => self.apply_settings_in_memory(),
                    SettingsAction::Cancel => {}
                }
            }
        }

        // ----- 电源动作确认弹窗 -----
        if let Some(action) = self.pending_power_action {
            if let Some(confirmed) = draw_power_confirmation_dialog(ui.ctx(), action) {
                self.pending_power_action = None;
                if confirmed {
                    // execute() 成功时不返回（进程消失）；返回 Err 仅在开发机调试时出现
                    if let Err(e) = action.execute() {
                        self.messages
                            .push(ChatMessage::assistant(format!("（电源动作失败）{e}")));
                    }
                }
            }
        }

        if busy {
            ui.ctx().request_repaint();
        }
    }
}

impl NeuroBootApp {
    /// 快捷问题按钮 + U 盘 prompts.txt 下拉 —— PE 无 IME 中文输入的兜底。
    fn draw_quick_prompt_bar(&mut self, ui: &mut egui::Ui, busy: bool) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.weak("快捷问题:");
            for (label, prompt) in QUICK_PROMPTS {
                ui.add_enabled_ui(!busy, |ui| {
                    if ui
                        .small_button(*label)
                        .on_hover_text(*prompt)
                        .clicked()
                    {
                        self.input_buffer = (*prompt).to_string();
                    }
                });
            }
            // v2 Stage 7.1: 一键全面检查（高频救援场景的入口）
            ui.separator();
            ui.add_enabled_ui(!busy, |ui| {
                if ui
                    .small_button("🔍 全面检查")
                    .on_hover_text("注入一份「请并行调多个只读诊断工具收集系统状态」的 prompt，让 agent 跑一轮完整诊断")
                    .clicked()
                {
                    self.input_buffer = FULL_CHECK_PROMPT.to_string();
                }
            });
        });

        if !self.user_prompts.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.weak("U 盘问题:");
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("user_prompts_combo")
                        .selected_text(format!("我准备的 {} 条问题...", self.user_prompts.len()))
                        .width(360.0)
                        .show_ui(ui, |ui| {
                            for p in &self.user_prompts {
                                let preview: String =
                                    p.text.chars().take(24).collect::<String>();
                                let label = format!("[{}] {}", p.label, preview);
                                if ui.selectable_label(false, label).clicked() {
                                    self.input_buffer = p.text.clone();
                                }
                            }
                        });
                });
            });
        }
        // v2 Stage 7.2: skill 下拉框（如果扫到了 skills）
        if !self.skills.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.weak("Skill:");
                ui.add_enabled_ui(!busy, |ui| {
                    let current_text = match self.active_skill_idx.and_then(|i| self.skills.get(i)) {
                        Some(s) => s.name.clone(),
                        None => "(无 skill / 用默认 system prompt)".to_owned(),
                    };
                    egui::ComboBox::from_id_salt("skills_combo")
                        .selected_text(current_text)
                        .width(360.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(self.active_skill_idx.is_none(), "(无 skill)")
                                .clicked()
                            {
                                self.active_skill_idx = None;
                            }
                            let skills_snapshot: Vec<(String, String)> = self
                                .skills
                                .iter()
                                .map(|s| (s.name.clone(), s.description.clone()))
                                .collect();
                            for (i, (name, desc)) in skills_snapshot.iter().enumerate() {
                                let label = if desc.is_empty() {
                                    name.clone()
                                } else {
                                    format!("{name} — {desc}")
                                };
                                if ui
                                    .selectable_label(self.active_skill_idx == Some(i), label)
                                    .clicked()
                                {
                                    self.active_skill_idx = Some(i);
                                }
                            }
                        });
                });
            });
        }
        ui.add_space(2.0);
    }

    /// 附件 chip 行 + 「+ 图片」按钮 —— 显示当前已选附图，让用户点 X 删除单张。
    /// VL 检测：当前端点不是 vision 模型时禁用按钮，hover 提示原因。
    fn draw_attachment_bar(&mut self, ui: &mut egui::Ui, busy: bool) {
        let vl_capable = is_vl_model(&self.active.model);
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!busy && vl_capable, |ui| {
                let hover = if !vl_capable {
                    format!(
                        "当前模型「{}」似乎不支持图片输入。点 ⚙ 设置切到 vision 模型（如 gpt-4o、claude-3、qwen-vl、deepseek-vl）后再上传。",
                        self.active.model
                    )
                } else {
                    "选择 png/jpg/webp/gif/bmp 图片附到下一条消息（多选）".to_owned()
                };
                if ui.button("+ 图片").on_hover_text(hover).clicked() {
                    // rfd 是模态阻塞调用 —— UI 这一帧会卡住直到用户关对话框，可接受
                    let picked = pick_image_files();
                    for path in picked {
                        match load_path_as_attached(&path) {
                            Ok(img) => {
                                if img.size_bytes > 10 * 1024 * 1024 {
                                    self.messages.push(ChatMessage::assistant(format!(
                                        "（警告）{} 大小 {} —— 超过 10 MB，部分 vision API 可能拒收或慢。",
                                        img.display_name,
                                        img.human_size()
                                    )));
                                }
                                self.attached_images.push(img);
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::assistant(format!(
                                    "（无法加载 {}）{}",
                                    e.path.display(),
                                    e.message
                                )));
                            }
                        }
                    }
                }
            });

            if !vl_capable && self.attached_images.is_empty() {
                ui.weak(format!("（模型 {} 不支持图片）", self.active.model));
            }

            // 已选附件 chips —— 「📷 name (size) [X]」
            let mut to_remove: Option<usize> = None;
            for (i, img) in self.attached_images.iter().enumerate() {
                ui.separator();
                ui.weak(format!("📷 {} · {}", img.display_name, img.human_size()));
                if ui.small_button("✕").on_hover_text("移除此图片").clicked() {
                    to_remove = Some(i);
                }
            }
            if let Some(i) = to_remove {
                self.attached_images.remove(i);
            }
        });
    }

    fn draw_input_panel(&mut self, ui: &mut egui::Ui, busy: bool) {
        // 附件 chip 行：附图列表 + 「+ 图片」按钮
        self.draw_attachment_bar(ui, busy);
        ui.add_space(4.0);
        let mut should_send = false;

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 88.0, 64.0],
                    egui::TextEdit::multiline(&mut self.input_buffer)
                        .hint_text("输入消息，Ctrl+Enter 或点「发送」提交")
                        .desired_rows(3),
                );

                if busy {
                    // v2 Stage 2: busy 时按钮变「停止生成」，点击 set cancel flag
                    if ui
                        .add_sized([80.0, 64.0], egui::Button::new("停止生成"))
                        .on_hover_text("中断本次流式生成（worker 会清理后退出）")
                        .clicked()
                    {
                        self.cancel_flag.store(true, Ordering::Relaxed);
                    }
                } else if ui
                    .add_sized([80.0, 64.0], egui::Button::new("发送"))
                    .clicked()
                {
                    should_send = true;
                }

                if response.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
                {
                    should_send = true;
                }
            });
        });
        ui.add_space(4.0);

        if should_send && !busy {
            self.submit_current_input();
        }
    }

    fn submit_current_input(&mut self) {
        let text = self.input_buffer.trim().to_owned();
        // 允许「只有图片没有文字」发送 —— 用户拍了张蓝屏只想说「这是什么」
        // 也可以一字不写直接发图让模型描述
        if text.is_empty() && self.attached_images.is_empty() {
            return;
        }
        // 把当前附图打包进消息；submit 后清空附图列表
        let images = std::mem::take(&mut self.attached_images);
        self.messages.push(ChatMessage::user_with_images(text, images));
        self.input_buffer.clear();

        // 新一轮 agent 任务前重置 cancel 标志
        self.cancel_flag.store(false, Ordering::Relaxed);

        // v2 Stage 7.2 + v3.0 W1.5: skill 注入 system prompt
        let mut system_prompt = self.system_prompt.clone();

        // v3.0 W1.5 Tier 1：始终列出**所有** skill summary（progressive disclosure）。
        // AI 据此判断何时调 load_skill(name) 拿完整 body。每条 ~80 tokens。
        if !self.skills.is_empty() {
            system_prompt.push_str(
                "\n\n---\n\n\
                 # 可用 skill（诊断剧本目录）\n\
                 \n\
                 system 列了下面所有 skill 的 name + 一句话描述。当用户请求匹配某 skill 的描述时，\
                 **先调 `load_skill(name=\"/...\")` 工具拿完整剧本**，再按剧本步骤执行（调对应工具 + 总结）。\
                 不匹配的请求直接回答 / 调常规工具，不要无谓 load_skill。\n\
                 \n",
            );
            for s in &self.skills {
                if s.description.is_empty() {
                    system_prompt.push_str(&format!("- `{}`\n", s.name));
                } else {
                    system_prompt.push_str(&format!("- `{}`: {}\n", s.name, s.description));
                }
            }
        }

        // v2 Stage 7.2 兼容路径：用户从 UI 下拉框手动激活某 skill，
        // 把它的完整 body 直接灌进 system prompt（不依赖 AI 自己 load_skill）。
        // v3.0 W1.5：从 self.skills[i].body 改为 lazy load 当前 active 那条。
        if let Some(summary) = self.active_skill_idx.and_then(|i| self.skills.get(i)) {
            if let Some(body) = load_skill_body(&summary.name) {
                system_prompt.push_str("\n\n---\n\n# 用户手动激活 skill: ");
                system_prompt.push_str(&body.name);
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&body.body);
            }
        }
        // v2 Stage 7.3: 取证模式额外约束
        if self.forensic_mode {
            system_prompt.push_str("\n\n---\n\n# 🔬 取证模式（强约束）\n\n\
                你正在取证模式下运行。**所有磁盘视作证据**：\n\
                - 绝对不写盘（已通过 readonly 屏蔽所有 dangerous 工具）\n\
                - 不建议任何修复操作\n\
                - 重点：发现异常 + 记录 + 截图保留证据，**不动它**\n\
                - 用户的目的是「保全现场后取证分析」，不是修复\n\
                如果用户要求修复，提醒「你在取证模式，要修复请重启进非取证模式」");
        }

        let job = AgentJob {
            endpoint: self.active.endpoint.clone(),
            model: self.active.model.clone(),
            api_key: self.active.api_key.clone(),
            system_prompt,
            messages: self.messages.clone(),
            tool_registry: Arc::clone(&self.tool_registry),
            cancel: Arc::clone(&self.cancel_flag),
        };
        self.pending_response = Some(spawn_agent_request(job));
    }

    fn poll_pending_response(&mut self) {
        let Some(rx) = &self.pending_response else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(AgentEvent::AssistantStart) => {
                    // 流式 assistant 消息开始：推一个空 assistant 容器
                    self.messages.push(ChatMessage::assistant(String::new()));
                }
                Ok(AgentEvent::TokenChunk(chunk)) => {
                    // 追加到最后一条 assistant message（应该是 AssistantStart 推的那个）
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == ui::chat::Role::Assistant {
                            last.content.push_str(&chunk);
                        } else {
                            // 防御：没拿到 AssistantStart 就来了 chunk —— 临时新建一个
                            self.messages.push(ChatMessage::assistant(chunk));
                        }
                    } else {
                        self.messages.push(ChatMessage::assistant(chunk));
                    }
                }
                Ok(AgentEvent::AssistantToolCalls(summaries)) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == ui::chat::Role::Assistant {
                            last.tool_calls = summaries;
                        }
                    }
                }
                Ok(AgentEvent::Message(msg)) => {
                    self.messages.push(msg);
                }
                Ok(AgentEvent::Done) => {
                    self.pending_response = None;
                    self.cancel_flag.store(false, Ordering::Relaxed);
                    return;
                }
                Ok(AgentEvent::Error(message)) => {
                    self.messages
                        .push(ChatMessage::assistant(format!("（出错）{message}")));
                    self.pending_response = None;
                    self.cancel_flag.store(false, Ordering::Relaxed);
                    return;
                }
                Ok(AgentEvent::Confirmation(req)) => {
                    // 存起来 + 渲染时画弹窗；pending_response 仍 Some（worker 还在 block）
                    self.pending_confirmation = Some(req);
                    return;
                }
                Ok(AgentEvent::PlanProposal(req)) => {
                    // v3.0 W3-4: 存起来 + 渲染时画 plan 审批弹窗；worker 阻塞等用户决定
                    self.pending_plan = Some(req);
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.messages
                        .push(ChatMessage::assistant("（出错）后台 Agent 线程意外断开"));
                    self.pending_response = None;
                    self.cancel_flag.store(false, Ordering::Relaxed);
                    return;
                }
            }
        }
    }

    /// 危险工具确认弹窗。
    ///
    /// 当 pending_confirmation Some 时显示一个居中的 Window：工具名、参数 JSON、
    /// 安全提示文字 + 「确认执行」/「取消」两个按钮。用户点击后通过 responder 把
    /// 决定送回 worker 线程，worker unblock 继续 agent loop。
    fn draw_confirmation_dialog(&mut self, ctx: &egui::Context) {
        if self.pending_confirmation.is_none() {
            return;
        }
        // 把要展示的数据先 clone 出来，避免 closure 借 self
        let (tool_name, arguments) = {
            let p = self.pending_confirmation.as_ref().unwrap();
            (p.tool_name.clone(), p.arguments.clone())
        };

        let mut chosen: Option<ConfirmationResponse> = None;

        // v2 Stage 7.4: 红色边框 + 更显眼背景，强化「这是危险操作」视觉信号
        let red_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 60, 60));
        let bg = egui::Color32::from_rgb(50, 30, 30);
        egui::Window::new("⚠ 确认执行危险工具")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .stroke(red_stroke)
                    .fill(bg),
            )
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::from_rgb(255, 220, 100),
                    format!("Agent 想调用危险工具：{tool_name}"),
                );
                ui.add_space(6.0);
                ui.label("参数（JSON）：");
                ui.code(&arguments);
                ui.add_space(8.0);
                ui.colored_label(
                    egui::Color32::from_rgb(255, 150, 100),
                    "此操作可能不可撤销。请仔细确认参数（特别是路径）后再继续。",
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([120.0, 28.0], egui::Button::new("确认执行"))
                        .clicked()
                    {
                        chosen = Some(ConfirmationResponse::Confirm);
                    }
                    ui.add_space(8.0);
                    if ui
                        .add_sized([120.0, 28.0], egui::Button::new("取消"))
                        .clicked()
                    {
                        chosen = Some(ConfirmationResponse::Reject);
                    }
                });
                ui.add_space(4.0);
            });

        if let Some(response) = chosen {
            if let Some(pending) = self.pending_confirmation.take() {
                let _ = pending.responder.send(response);
            }
        }
    }

    /// v3.0 W3-4: Plan Mode 审批弹窗。
    ///
    /// pending_plan Some 时显示居中 Window：summary + steps 列表（dangerous 步骤红色）+
    /// Approve / Reject 两按钮。用户点击通过 responder 把决定送回 worker。
    fn draw_plan_dialog(&mut self, ctx: &egui::Context) {
        if self.pending_plan.is_none() {
            return;
        }
        // Clone 出展示数据，避免 closure 借 self
        let (summary, steps) = {
            let p = self.pending_plan.as_ref().unwrap();
            (p.summary.clone(), p.steps.clone())
        };

        let mut chosen: Option<PlanResponse> = None;
        let has_dangerous = steps.iter().any(|s| s.safety == "dangerous");

        // 含 dangerous 时用红边框 + 警告色；纯 safe 时用普通蓝色
        let (stroke_color, bg, header_color, header_icon) = if has_dangerous {
            (
                egui::Color32::from_rgb(220, 60, 60),
                egui::Color32::from_rgb(50, 30, 30),
                egui::Color32::from_rgb(255, 220, 100),
                "⚠ 审批 Plan（含 dangerous 步骤）",
            )
        } else {
            (
                egui::Color32::from_rgb(80, 150, 220),
                egui::Color32::from_rgb(30, 40, 50),
                egui::Color32::from_rgb(180, 220, 255),
                "📋 审批 Plan",
            )
        };

        egui::Window::new(header_icon)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .max_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .stroke(egui::Stroke::new(2.0, stroke_color))
                    .fill(bg),
            )
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add_space(4.0);
                ui.colored_label(header_color, format!("📝 Summary：{summary}"));
                ui.add_space(8.0);
                ui.label(format!("共 {} 步：", steps.len()));
                ui.add_space(4.0);

                // 步骤列表（可滚动）
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (i, step) in steps.iter().enumerate() {
                            ui.add_space(2.0);
                            let is_dangerous = step.safety == "dangerous";
                            let marker = if is_dangerous { "⚠ DANGEROUS" } else { "✓ safe" };
                            let marker_color = if is_dangerous {
                                egui::Color32::from_rgb(255, 120, 100)
                            } else {
                                egui::Color32::from_rgb(160, 220, 160)
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("[{}]", i + 1));
                                ui.colored_label(marker_color, marker);
                                ui.strong(&step.tool);
                            });
                            if !step.args_preview.is_empty() {
                                ui.label(format!("  args: {}", step.args_preview));
                            }
                            ui.label(format!("  → {}", step.why));
                            ui.separator();
                        }
                    });

                ui.add_space(8.0);
                if has_dangerous {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 150, 100),
                        "本 plan 含 dangerous 步骤 —— 批准后**每个 dangerous 工具**仍会再单独弹窗确认。",
                    );
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([130.0, 30.0], egui::Button::new("✅ Approve"))
                        .clicked()
                    {
                        chosen = Some(PlanResponse::Approve);
                    }
                    ui.add_space(8.0);
                    if ui
                        .add_sized([130.0, 30.0], egui::Button::new("❌ Reject"))
                        .clicked()
                    {
                        chosen = Some(PlanResponse::Reject);
                    }
                });
                ui.add_space(4.0);
            });

        if let Some(response) = chosen {
            if let Some(pending) = self.pending_plan.take() {
                let _ = pending.responder.send(response);
            }
        }
    }
}

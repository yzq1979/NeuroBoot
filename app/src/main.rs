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
mod tools;
mod ui;

use std::sync::mpsc;
use std::sync::Arc;

use agent::{
    spawn_agent_request, AgentEvent, AgentJob, ConfirmationRequest, ConfirmationResponse,
};
use eframe::egui;
use llm::config_file::{is_vl_model, save_to_first_writable_drive, ConfigFile};
use llm::endpoint::{detect_endpoints, DetectedEndpoints, EndpointConfig};
use tools::ToolRegistry;
use ui::{
    draw_power_confirmation_dialog, draw_settings_dialog, install_chinese_fonts, launch_cmd,
    launch_file_manager, load_path_as_attached, pick_image_files, render_message,
    scan_user_prompts, AttachedImage, ChatMessage, CommonMarkCache, PowerAction, SettingsAction,
    SettingsBuffer, StatusBarState, UserPrompt,
};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080";
const DEFAULT_MODEL: &str = "qwen3-4b-instruct";

const DEFAULT_SYSTEM_PROMPT: &str = "\
你是 NeuroBoot 神启，运行在 Windows 维护环境里的 AI 助手，帮用户诊断和修复 Windows 系统问题。

行为准则：
- 你可以调用提供的工具读取系统信息（硬盘、事件日志、硬件配置等）。诊断完成后用中文简洁回答。
- 不知道的硬件/系统信息**不要瞎编** —— 应该调工具查询；若没有合适工具，明确告诉用户「无法查到」。
- 工具结果可能是命令的 stdout，可能较长 —— 只摘取相关部分回答用户。
- **危险工具**（标注「不可撤销」的）会触发用户确认弹窗。如果用户拒绝执行，不要重试同样操作，问用户是否换个方式。
- 用中文，简明扼要。
";

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

        // 注册工具：v1 baseline (3 safe + 1 dangerous) + v2 P0 (8 safe)
        let mut registry = ToolRegistry::new();
        // v1 baseline
        registry.register(Box::new(tools::safe::list_disks::ListDisks));
        registry.register(Box::new(tools::safe::read_system_info::ReadSystemInfo));
        registry.register(Box::new(tools::safe::read_event_log_errors::ReadEventLogErrors));
        registry.register(Box::new(tools::dangerous::delete_path::DeletePath));
        // v2 P0 新增 safe 工具
        registry.register(Box::new(tools::safe::list_partitions::ListPartitions));
        registry.register(Box::new(tools::safe::list_volumes::ListVolumes));
        registry.register(Box::new(tools::safe::read_ip_config::ReadIpConfig));
        registry.register(Box::new(tools::safe::list_network_adapters::ListNetworkAdapters));
        registry.register(Box::new(tools::safe::list_processes_top::ListProcessesTop));
        registry.register(Box::new(tools::safe::list_services::ListServices));
        registry.register(Box::new(tools::safe::list_minidumps::ListMinidumps));
        registry.register(Box::new(tools::safe::list_recent_shutdowns::ListRecentShutdowns));

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

                let btn_label = if busy { "发送中" } else { "发送" };
                if ui
                    .add_sized([80.0, 64.0], egui::Button::new(btn_label))
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

        let job = AgentJob {
            endpoint: self.active.endpoint.clone(),
            model: self.active.model.clone(),
            api_key: self.active.api_key.clone(),
            system_prompt: self.system_prompt.clone(),
            messages: self.messages.clone(),
            tool_registry: Arc::clone(&self.tool_registry),
        };
        self.pending_response = Some(spawn_agent_request(job));
    }

    fn poll_pending_response(&mut self) {
        let Some(rx) = &self.pending_response else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(AgentEvent::Message(msg)) => {
                    self.messages.push(msg);
                }
                Ok(AgentEvent::Done) => {
                    self.pending_response = None;
                    return;
                }
                Ok(AgentEvent::Error(message)) => {
                    self.messages
                        .push(ChatMessage::assistant(format!("（出错）{message}")));
                    self.pending_response = None;
                    return;
                }
                Ok(AgentEvent::Confirmation(req)) => {
                    // 存起来 + 渲染时画弹窗；pending_response 仍 Some（worker 还在 block）
                    self.pending_confirmation = Some(req);
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.messages
                        .push(ChatMessage::assistant("（出错）后台 Agent 线程意外断开"));
                    self.pending_response = None;
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

        egui::Window::new("确认执行危险工具")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add_space(4.0);
                ui.label(format!("Agent 想调用危险工具：{tool_name}"));
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
}

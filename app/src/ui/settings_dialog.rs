//! 在线 AI 端点设置面板 —— modal Window，编辑 ConfigFile 字段 + 保存到 U 盘。
//!
//! 阶段 v1.0.1 新增：U 盘真测发现 PE 内无法注入环境变量，必须提供 GUI 配置 + 持久化方案。
//!
//! 设计：
//! - `SettingsBuffer` 是可编辑的中间态（egui TextEdit 需要 `&mut String`），
//!   主程序从 ConfigFile 初始化它、对话框结束后再把它写回 ConfigFile
//! - `draw_settings_dialog` 是纯函数式 —— 返回用户的动作选择，由主程序决定怎么处理
//! - 这样避免 dialog 内部直接动 App 状态，UI 边界清晰、可单测

use eframe::egui;

use crate::llm::config_file::ConfigFile;

/// 设置面板的可编辑表单状态。
#[derive(Debug, Clone, Default)]
pub struct SettingsBuffer {
    pub remote_endpoint: String,
    pub remote_model: String,
    pub remote_api_key: String,
    pub remote_label: String,
    pub prefer_remote: bool,
    pub local_endpoint: String,
    pub local_model: String,
}

impl SettingsBuffer {
    /// 从 ConfigFile 初始化 buffer（用于打开对话框时）。
    pub fn from_config(cfg: &ConfigFile) -> Self {
        Self {
            remote_endpoint: cfg.remote_endpoint.clone(),
            remote_model: cfg.remote_model.clone(),
            remote_api_key: cfg.remote_api_key.clone(),
            remote_label: cfg.remote_label.clone(),
            prefer_remote: cfg.prefer_remote,
            local_endpoint: cfg.local_endpoint.clone(),
            local_model: cfg.local_model.clone(),
        }
    }

    /// 把 buffer 写回 ConfigFile（保留原 cfg 里 buffer 不管的字段，如 system_prompt_override）。
    pub fn apply_to_config(&self, cfg: &mut ConfigFile) {
        cfg.remote_endpoint = self.remote_endpoint.trim().to_owned();
        cfg.remote_model = self.remote_model.trim().to_owned();
        cfg.remote_api_key = self.remote_api_key.trim().to_owned();
        cfg.remote_label = if self.remote_label.trim().is_empty() {
            "云端".to_owned()
        } else {
            self.remote_label.trim().to_owned()
        };
        cfg.prefer_remote = self.prefer_remote;
        cfg.local_endpoint = self.local_endpoint.trim().to_owned();
        cfg.local_model = self.local_model.trim().to_owned();
    }
}

/// 用户在设置面板的动作选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// 保存到 U 盘 + 重新探测端点
    SaveAndReprobe,
    /// 仅本次会话使用（不写文件，主程序应直接套用到 active endpoint）
    ApplyOnce,
    /// 关闭，不应用任何改动
    Cancel,
}

/// 渲染设置面板 modal Window。返回 Some(action) 表示用户做了选择，None 表示窗口仍在编辑中。
///
/// 主程序应当在 `show_settings = true` 时每帧调用，并在收到 Some 时清空 show_settings。
pub fn draw_settings_dialog(
    ctx: &egui::Context,
    buf: &mut SettingsBuffer,
) -> Option<SettingsAction> {
    let mut chosen: Option<SettingsAction> = None;

    egui::Window::new("在线 AI 端点设置")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(520.0);
            ui.add_space(4.0);

            ui.label("在线端点（OpenAI 兼容，如 DeepSeek / 通义千问 / 智谱 / OpenAI 本身）：");
            ui.add_space(4.0);

            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Endpoint URL:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.remote_endpoint)
                            .hint_text("https://api.deepseek.com")
                            .desired_width(360.0),
                    );
                    ui.end_row();

                    ui.label("Model:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.remote_model)
                            .hint_text("deepseek-chat / gpt-4o-mini / qwen-plus")
                            .desired_width(360.0),
                    );
                    ui.end_row();

                    ui.label("API Key:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.remote_api_key)
                            .password(true)
                            .hint_text("sk-xxxxxxxxxxxxxxxxxxxx")
                            .desired_width(360.0),
                    );
                    ui.end_row();

                    ui.label("显示名:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.remote_label)
                            .hint_text("云端 / DeepSeek / 通义")
                            .desired_width(360.0),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.checkbox(
                &mut buf.prefer_remote,
                "启动时优先用在线端点（若可达）",
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.weak("本地端点（一般不用改，除非你跑了多个 llama-server）：");
            ui.add_space(4.0);

            egui::Grid::new("settings_local_grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Local Endpoint:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.local_endpoint)
                            .desired_width(360.0),
                    );
                    ui.end_row();

                    ui.label("Local Model:");
                    ui.add(
                        egui::TextEdit::singleline(&mut buf.local_model)
                            .desired_width(360.0),
                    );
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized([170.0, 30.0], egui::Button::new("保存到 U 盘并重新探测"))
                    .on_hover_text(
                        "把当前配置写入第一个可写非 X: 盘的 NeuroBoot.config.json，\
                         下次启动也生效；同时立刻重新探测端点。",
                    )
                    .clicked()
                {
                    chosen = Some(SettingsAction::SaveAndReprobe);
                }
                ui.add_space(6.0);
                if ui
                    .add_sized([170.0, 30.0], egui::Button::new("仅本次会话使用"))
                    .on_hover_text("不写文件，立刻把当前配置应用到 active endpoint。")
                    .clicked()
                {
                    chosen = Some(SettingsAction::ApplyOnce);
                }
                ui.add_space(6.0);
                if ui
                    .add_sized([100.0, 30.0], egui::Button::new("关闭"))
                    .clicked()
                {
                    chosen = Some(SettingsAction::Cancel);
                }
            });
            ui.add_space(4.0);
        });

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_round_trip() {
        let cfg = ConfigFile {
            remote_endpoint: "https://api.deepseek.com".to_owned(),
            remote_model: "deepseek-chat".to_owned(),
            remote_api_key: "sk-abc".to_owned(),
            remote_label: "DeepSeek".to_owned(),
            prefer_remote: false,
            local_endpoint: "http://127.0.0.1:9090".to_owned(),
            local_model: "qwen-test".to_owned(),
            system_prompt_override: Some("custom".to_owned()),
        };
        let buf = SettingsBuffer::from_config(&cfg);
        assert_eq!(buf.remote_endpoint, cfg.remote_endpoint);
        assert_eq!(buf.prefer_remote, cfg.prefer_remote);

        let mut cfg2 = ConfigFile::default();
        // 给 system_prompt_override 设个值，验证 apply 不会破坏它
        cfg2.system_prompt_override = Some("preserve_me".to_owned());
        buf.apply_to_config(&mut cfg2);
        assert_eq!(cfg2.remote_endpoint, "https://api.deepseek.com");
        assert_eq!(cfg2.local_model, "qwen-test");
        assert_eq!(
            cfg2.system_prompt_override,
            Some("preserve_me".to_owned()),
            "apply_to_config 不应该破坏 buffer 不覆盖的字段"
        );
    }

    #[test]
    fn empty_label_falls_back_to_default() {
        let buf = SettingsBuffer {
            remote_label: "   ".to_owned(),
            ..Default::default()
        };
        let mut cfg = ConfigFile::default();
        buf.apply_to_config(&mut cfg);
        assert_eq!(cfg.remote_label, "云端");
    }
}

//! UI 子模块入口：聊天数据结构与渲染、中文字体加载。
//!
//! 后续阶段会在这里继续扩展（如工具确认弹窗、设置面板、A/C 路由状态栏等）。

pub mod chat;
mod fonts;
pub mod image_picker;
pub mod log_viewer;
pub mod power_actions;
pub mod prompts_file;
pub mod settings_dialog;
pub mod skills;
pub mod status_bar;
pub mod system_launchers;

pub use chat::{render_message, AttachedImage, ChatMessage};
// Re-export egui_commonmark cache for main.rs to hold a shared instance
pub use egui_commonmark::CommonMarkCache;
pub use fonts::install_chinese_fonts;
pub use image_picker::{load_path_as_attached, pick_image_files};
pub use log_viewer::open_log_dir;
pub use power_actions::{draw_power_confirmation_dialog, PowerAction};
pub use prompts_file::{scan_user_prompts, UserPrompt};
pub use settings_dialog::{draw_settings_dialog, SettingsAction, SettingsBuffer};
pub use skills::{load_skill_body, scan_skills, SkillSummary};
pub use status_bar::StatusBarState;
pub use system_launchers::{launch_cmd, launch_file_manager};

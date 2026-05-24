//! 聊天消息数据结构与渲染。
//!
//! `Role` 与 OpenAI 兼容协议（llama.cpp chat API 同样兼容）的 `role` 字段对应；
//! 阶段 3 加 `Role::Tool` 与 `tool_calls` 字段，支持 Agent function calling 流程。
//! 阶段 v1.0.1+ 加 `images: Vec<AttachedImage>` 字段，支持 vision 多模态。

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// 聊天消息的角色 —— OpenAI 兼容协议里 `role` 字段的对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 用户输入
    User,
    /// 模型回复（可能含 tool_calls）
    Assistant,
    /// 系统提示（指令、工具清单、行为约束）—— 阶段 3 起 Agent 用
    System,
    /// 工具执行结果回传给模型 —— OpenAI 协议 "tool" 角色，需带 tool_call_id
    Tool,
}

impl Role {
    /// 渲染时显示的中文前缀。
    pub fn display_prefix(self) -> &'static str {
        match self {
            Role::User => "你",
            Role::Assistant => "神启",
            Role::System => "系统",
            Role::Tool => "结果",
        }
    }

    /// 渲染时前缀的颜色（让对话视觉上分得清）。
    pub fn display_color(self) -> egui::Color32 {
        match self {
            Role::User => egui::Color32::from_rgb(120, 170, 255),      // 蓝
            Role::Assistant => egui::Color32::from_rgb(170, 220, 170), // 绿
            Role::System => egui::Color32::DARK_GRAY,
            Role::Tool => egui::Color32::from_rgb(255, 180, 100), // 橙
        }
    }
}

/// Assistant 决定调用一次工具的摘要 —— OpenAI 协议 `tool_calls[]` 元素的 UI 层映射。
///
/// UI 上单独渲染成「工具：name(arguments)」一行（橙色），让用户看见 Agent 思考链。
#[derive(Debug, Clone)]
pub struct ToolCallSummary {
    pub id: String,
    pub name: String,
    /// JSON 字符串（模型生成的实参）
    pub arguments: String,
}

/// 用户附加到消息上的图片（v1.0.1+ vision 支持）。
///
/// `data_base64` 是原始字节的 base64 字符串；`mime` 决定 `data:image/<x>;base64,...` 前缀。
/// 通常 jpeg/png/webp。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedImage {
    pub mime: String,
    pub data_base64: String,
    pub display_name: String,
    pub size_bytes: u64,
}

impl AttachedImage {
    /// 转成 OpenAI vision API 用的 data URL：`data:image/jpeg;base64,<base64>`。
    pub fn to_data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data_base64)
    }

    /// 给 UI 显示用的尺寸文本：`23.4 KB` / `2.1 MB`。
    pub fn human_size(&self) -> String {
        let kb = self.size_bytes as f64 / 1024.0;
        if kb < 1024.0 {
            format!("{:.1} KB", kb)
        } else {
            format!("{:.2} MB", kb / 1024.0)
        }
    }
}

/// 一条聊天消息：角色 + 文本内容 + 可选的工具调用 / 工具结果元数据 / 附图。
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    /// 文本内容：
    /// - User / System：原始输入
    /// - Assistant：模型回答；纯调工具时可能为空字符串
    /// - Tool：工具的执行结果（stdout 等）
    pub content: String,
    /// 仅 Role::Assistant 用：模型本轮想调用的工具列表（可能多个）
    pub tool_calls: Vec<ToolCallSummary>,
    /// 仅 Role::Tool 用：响应的是哪一个 tool_call.id
    pub tool_call_id: Option<String>,
    /// 仅 Role::User 用：附加的图片（vision API）
    pub images: Vec<AttachedImage>,
}

impl ChatMessage {
    /// 构造一条 user 消息（无图片）—— 简化版，等价于 `user_with_images(content, vec![])`。
    #[allow(dead_code)] // 保留作为 v1 API 兼容；现在所有 caller 走 user_with_images
    pub fn user(content: impl Into<String>) -> Self {
        Self::user_with_images(content, Vec::new())
    }

    /// 构造一条 user 消息含图片附件。
    pub fn user_with_images(content: impl Into<String>, images: Vec<AttachedImage>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images,
        }
    }

    /// 构造一条 assistant 消息（无工具调用）。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// 构造一条 system 消息（阶段 3 起 Agent 的 system prompt）。
    #[allow(dead_code)] // 阶段 3.2 起 agent loop 注入 system prompt 时用
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// 构造一条 tool 消息（工具执行结果回传给模型）。
    #[allow(dead_code)] // 阶段 3.2 起 agent loop 构造
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            images: Vec::new(),
        }
    }
}

/// 在 ui 上渲染一条聊天消息：角色前缀（带颜色）+ 内容 + 可能的 tool_calls 摘要 + 附图 chips。
///
/// 渲染策略：
/// - User：前缀 + 纯文本 label（用户输入不假设 Markdown）
/// - Assistant：前缀 + **Markdown 渲染** (v2 P0，CommonMarkViewer 处理 **bold** / 列表 / 代码块 /
///   表格 / 引用 等)；模型常用 Markdown 输出，能渲染极大提升观感
/// - Tool：前缀 + 等宽 code-style label（JSON / stdout 用等宽显示更整齐，且不让 Markdown 误解析）
/// - System：默认不显示（system prompt 通常不暴露给用户）
/// - Assistant 含 tool_calls：内容之后追加每个 tool_call 的「工具：name(args)」橙色摘要
/// - User 含 images：内容之后追加每张图的「📷 filename (size)」chip
pub fn render_message(ui: &mut egui::Ui, msg: &ChatMessage, md_cache: &mut CommonMarkCache) {
    // System 消息不显示给用户（避免把 system prompt 暴露在聊天框里）
    if msg.role == Role::System {
        return;
    }

    if !msg.content.is_empty() {
        match msg.role {
            Role::Assistant => {
                // 角色前缀 + Markdown 渲染：前缀单起一行让模型的 Markdown 行宽充分
                ui.colored_label(
                    msg.role.display_color(),
                    format!("{}：", msg.role.display_prefix()),
                );
                CommonMarkViewer::new().show(ui, md_cache, &msg.content);
            }
            Role::Tool => {
                // 工具结果通常是 JSON / stdout：用等宽字体让数字 / 缩进对齐
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        msg.role.display_color(),
                        format!("{}：", msg.role.display_prefix()),
                    );
                });
                ui.monospace(&msg.content);
            }
            _ => {
                // User: 前缀 + 纯文本
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        msg.role.display_color(),
                        format!("{}：", msg.role.display_prefix()),
                    );
                    ui.label(&msg.content);
                });
            }
        }
    }

    // User 附图：每张图渲染成蓝色 chip「📷 filename (size)」
    for img in &msg.images {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(Role::User.display_color(), "📷");
            ui.weak(format!("{} · {}", img.display_name, img.human_size()));
        });
    }

    // Assistant 的 tool_calls：每个调用渲染成独立的橙色"工具："行
    for tc in &msg.tool_calls {
        render_tool_call_summary(ui, tc);
    }

    ui.add_space(6.0);
}

/// 渲染一次工具调用的摘要：「工具：name(arguments)」。
fn render_tool_call_summary(ui: &mut egui::Ui, tc: &ToolCallSummary) {
    let tool_color = Role::Tool.display_color();
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(tool_color, "工具：");
        // 用等宽字体显示 name(args) —— 让 JSON 参数视觉上区分
        ui.code(format!("{}({})", tc.name, tc.arguments));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_image_to_data_url() {
        let img = AttachedImage {
            mime: "image/png".to_owned(),
            data_base64: "iVBORw0KGgo=".to_owned(),
            display_name: "test.png".to_owned(),
            size_bytes: 1024,
        };
        assert_eq!(img.to_data_url(), "data:image/png;base64,iVBORw0KGgo=");
    }

    #[test]
    fn human_size_formats() {
        let mut img = AttachedImage {
            mime: "image/jpeg".to_owned(),
            data_base64: String::new(),
            display_name: "x".to_owned(),
            size_bytes: 500,
        };
        assert_eq!(img.human_size(), "0.5 KB");

        img.size_bytes = 1024 * 100; // 100 KB
        assert_eq!(img.human_size(), "100.0 KB");

        img.size_bytes = 2 * 1024 * 1024 + 512 * 1024; // 2.5 MB
        assert_eq!(img.human_size(), "2.50 MB");
    }

    #[test]
    fn user_with_images_constructor() {
        let imgs = vec![AttachedImage {
            mime: "image/png".to_owned(),
            data_base64: "Zm9v".to_owned(),
            display_name: "a.png".to_owned(),
            size_bytes: 3,
        }];
        let msg = ChatMessage::user_with_images("看这个", imgs.clone());
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "看这个");
        assert_eq!(msg.images, imgs);
    }
}

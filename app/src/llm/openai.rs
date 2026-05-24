//! OpenAI chat completion API 的数据结构（serde 序列化/反序列化）。
//!
//! 与 llama.cpp `llama-server` 暴露的 `/v1/chat/completions` 端点兼容。
//! 阶段 3 加 function calling 协议：`tools`、`tool_calls`、`tool_call_id`。
//! 阶段 v1.0.1+ 加 vision 多模态协议：`content` 既可以是单纯 String，
//! 也可以是 ContentPart 数组（text + image_url 混合），匹配 OpenAI Vision schema。

use serde::{Deserialize, Serialize};

use crate::ui::chat::{ChatMessage, Role, ToolCallSummary};

/// OpenAI 兼容消息内容 —— 既支持纯文本（OpenAI 老协议）也支持 vision 多模态数组。
///
/// Serde untagged：
/// - `Content::Text("...")` 序列化为 `"content": "..."`
/// - `Content::Parts([...])` 序列化为 `"content": [{...}, {...}]`
///
/// 反序列化时按 JSON 实际形状自动匹配（字符串 / 数组）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// vision 多模态消息中的一个内容片段。
///
/// OpenAI 协议 `{type: "text", text: "..."} | {type: "image_url", image_url: {url}}`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrl,
    },
}

/// vision API 的 image_url 子对象。
///
/// `url` 可以是：
/// - HTTPS 真 URL：`https://example.com/image.jpg`
/// - data URL：`data:image/jpeg;base64,<base64-encoded-bytes>`（v1.0.1 走这条，把本地图片塞进 body）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageUrl {
    pub url: String,
}

/// OpenAI 兼容消息：`role` 字符串（"user"/"assistant"/"system"/"tool"），content 文本或多模态。
///
/// 阶段 3 加：
/// - `content` 改成 Option（assistant 纯调工具时 OpenAI 协议允许 null）
/// - `tool_calls`：assistant 响应里模型决定调的工具列表
/// - `tool_call_id`：role="tool" 时必填，标识响应哪一个 tool_call
///
/// 阶段 v1.0.1+ 改 `content` 从 `Option<String>` 变 `Option<Content>` —— 兼容旧 String 形态
/// 也支持 vision Parts 数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl OpenAiMessage {
    /// 把内部 `ChatMessage` 转成 OpenAI 协议消息。
    ///
    /// 决策：
    /// - assistant content 为空 + 无 tool_calls + 无 images → None
    /// - assistant content 为空 + 有 tool_calls → None（OpenAI 协议「纯工具调用」）
    /// - images 非空 → Content::Parts([text, image1, image2, ...])
    /// - 否则 → Content::Text(content)
    pub fn from_chat(msg: &ChatMessage) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };

        let content = if !msg.images.is_empty() {
            // 多模态：文本 + 所有图片
            let mut parts: Vec<ContentPart> = Vec::with_capacity(1 + msg.images.len());
            if !msg.content.is_empty() {
                parts.push(ContentPart::Text {
                    text: msg.content.clone(),
                });
            }
            for img in &msg.images {
                parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: img.to_data_url(),
                    },
                });
            }
            Some(Content::Parts(parts))
        } else if msg.content.is_empty() {
            None
        } else {
            Some(Content::Text(msg.content.clone()))
        };

        let tool_calls = if msg.tool_calls.is_empty() {
            None
        } else {
            Some(
                msg.tool_calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        kind: "function".to_owned(),
                        function: ToolCallFunction {
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                    })
                    .collect(),
            )
        };

        Self {
            role: role.to_owned(),
            content,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    /// 取出 content 的文本部分（多模态消息合并所有 Text part），用于 UI 显示 / token 估算。
    pub fn content_text(&self) -> String {
        match &self.content {
            None => String::new(),
            Some(Content::Text(s)) => s.clone(),
            Some(Content::Parts(parts)) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.clone()),
                    ContentPart::ImageUrl { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// chat completion 请求体（OpenAI 兼容）。
///
/// 阶段 3 加 `tools` 字段（function calling 工具清单，None 表示不提供工具）。
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 阶段 2 固定 false；阶段 4 改流式时设 true 并改用 SSE 解析
    pub stream: bool,
    /// 工具清单（OpenAI function calling）—— 阶段 3.2 起 agent loop 注入
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// chat completion 响应体（最小化，只解析我们用得到的字段）。
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    pub message: OpenAiMessage,
    #[serde(default)]
    #[allow(dead_code)] // 阶段 4 起可能用到（length / stop / tool_calls 等）
    pub finish_reason: Option<String>,
}

/// 工具定义 —— OpenAI function calling 协议的 `tools[]` 元素。
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    /// 始终 "function"（OpenAI 协议目前只有这个类型）
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// 用 Tool trait 的 name/description/parameters_schema 构造一个 ToolDefinition。
    #[allow(dead_code)] // 阶段 3.2 起 agent loop 把 ToolRegistry 转成 tools[] 时用
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            kind: "function".to_owned(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// function 定义 —— 工具的名字、描述、参数 JSON Schema。
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    /// 参数的 JSON Schema（OpenAI function calling 规定的格式）
    pub parameters: serde_json::Value,
}

/// 模型决定调用一次工具 —— OpenAI 协议响应里 `tool_calls[]` 元素。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// 始终 "function"
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

impl ToolCall {
    /// 把 API 协议层的 ToolCall 转成 UI 层 ToolCallSummary。
    #[allow(dead_code)] // 阶段 3.2 起 agent loop 用
    pub fn to_summary(&self) -> ToolCallSummary {
        ToolCallSummary {
            id: self.id.clone(),
            name: self.function.name.clone(),
            arguments: self.function.arguments.clone(),
        }
    }
}

/// `tool_calls[].function` 子对象 —— 模型生成的函数名与参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON 字符串（模型生成的实参 JSON 序列化）
    pub arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::chat::AttachedImage;

    #[test]
    fn content_text_serializes_as_string() {
        let c = Content::Text("hello".to_owned());
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, r#""hello""#);
    }

    #[test]
    fn content_parts_serializes_as_array() {
        let c = Content::Parts(vec![
            ContentPart::Text {
                text: "what is this".to_owned(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: "data:image/png;base64,iVBORw0KGgo=".to_owned(),
                },
            },
        ]);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""type":"image_url""#));
        assert!(json.contains(r#""url":"data:image/png;base64,iVBORw0KGgo=""#));
    }

    #[test]
    fn content_deserializes_either_form() {
        let s: Content = serde_json::from_str(r#""plain text""#).unwrap();
        assert_eq!(s, Content::Text("plain text".to_owned()));

        let p: Content = serde_json::from_str(
            r#"[{"type":"text","text":"hi"},{"type":"image_url","image_url":{"url":"data:..."}}]"#,
        )
        .unwrap();
        match p {
            Content::Parts(parts) => assert_eq!(parts.len(), 2),
            _ => panic!("expected Parts"),
        }
    }

    #[test]
    fn from_chat_with_images_makes_parts() {
        let msg = ChatMessage {
            role: Role::User,
            content: "看这个错误".to_owned(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: vec![AttachedImage {
                mime: "image/png".to_owned(),
                data_base64: "iVBORw0KGgo=".to_owned(),
                display_name: "bsod.png".to_owned(),
                size_bytes: 1024,
            }],
        };
        let api = OpenAiMessage::from_chat(&msg);
        match api.content {
            Some(Content::Parts(parts)) => {
                assert_eq!(parts.len(), 2, "1 text + 1 image");
            }
            _ => panic!("expected Parts"),
        }
    }

    #[test]
    fn from_chat_without_images_keeps_string() {
        let msg = ChatMessage {
            role: Role::User,
            content: "纯文本".to_owned(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        };
        let api = OpenAiMessage::from_chat(&msg);
        match api.content {
            Some(Content::Text(s)) => assert_eq!(s, "纯文本"),
            _ => panic!("expected Text"),
        }
    }
}

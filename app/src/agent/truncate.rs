//! Token 估算与对话历史 truncation —— 防止单次请求超过 server ctx limit。
//!
//! 设计思路：
//! - **估算粗略**：不引入完整 tokenizer（Qwen tokenizer 在 Rust 生态里没成熟 crate）。
//!   按 char count / 2 + 协议 overhead 估算 —— over-estimate 偏多，让 truncation 触发偏早 = 安全。
//! - **整 turn 删除**：一个 turn = user message + 后续 assistant/tool messages 直到下一个 user。
//!   按整 turn 删能保证 assistant tool_calls ↔ tool message tool_call_id 配对完整，
//!   否则 OpenAI server 会因孤立的 tool message 报 400。

use crate::llm::openai::{Content, ContentPart, OpenAiMessage};

/// 粗略估算 OpenAI 消息列表的 token 数。
///
/// 公式：每条 message 6 token overhead + 文本 chars/2 + tool_calls 各 10 + (name+args)/2。
/// 多模态消息：文本 part 按字符算；image_url 按粗估 1024 tokens/张计入（OpenAI vision
/// 实际依分辨率不同，512×512 ~85 tokens，1024×1024 ~765 tokens；over-estimate 安全）。
/// 这是 over-estimate（实际 tokenizer 通常更紧凑），让 truncation 偏早触发更安全。
pub fn estimate_tokens(messages: &[OpenAiMessage]) -> usize {
    let mut total: usize = 0;
    for m in messages {
        total += 6; // 协议 overhead per message
        match &m.content {
            None => {}
            Some(Content::Text(s)) => {
                total += s.chars().count() / 2 + 1;
            }
            Some(Content::Parts(parts)) => {
                for p in parts {
                    match p {
                        ContentPart::Text { text } => total += text.chars().count() / 2 + 1,
                        ContentPart::ImageUrl { .. } => total += 1024, // 粗估
                    }
                }
            }
        }
        if let Some(tcs) = &m.tool_calls {
            for tc in tcs {
                total += (tc.function.name.len() + tc.function.arguments.len()) / 2 + 10;
            }
        }
    }
    total
}

/// 如果总 token 数超过 max_input_tokens，按整 turn 删最早对话（保留 system messages）。
///
/// 一个 turn = user message + 后续 assistant/tool messages 直到下一个 user message。
/// 按整 turn 删 = OpenAI 协议正确性保证（每个 tool message 必须前有对应 assistant tool_calls）。
///
/// 极端情况：删到只剩 system 还超阈值 —— 不再删（让 server 自己报错，UI 提示用户）。
pub fn truncate_history(messages: &mut Vec<OpenAiMessage>, max_input_tokens: usize) {
    loop {
        if estimate_tokens(messages) <= max_input_tokens {
            return;
        }
        // 找第一个 user 的位置（system 在最前面，留着不动）
        let first_user = match messages.iter().position(|m| m.role == "user") {
            Some(p) => p,
            None => return, // 已无 user 可删 —— 只剩 system，没法再缩
        };
        // 找下一个 user 的位置（或末尾）
        let next_user = messages[(first_user + 1)..]
            .iter()
            .position(|m| m.role == "user")
            .map(|p| first_user + 1 + p)
            .unwrap_or(messages.len());
        // 删除 [first_user, next_user) 这一个完整 turn
        messages.drain(first_user..next_user);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> OpenAiMessage {
        OpenAiMessage {
            role: role.to_owned(),
            content: if content.is_empty() {
                None
            } else {
                Some(Content::Text(content.to_owned()))
            },
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[test]
    fn keeps_short_history_untouched() {
        let mut msgs = vec![
            msg("system", "you are an assistant"),
            msg("user", "hi"),
            msg("assistant", "hello"),
        ];
        let before_len = msgs.len();
        truncate_history(&mut msgs, 10_000);
        assert_eq!(msgs.len(), before_len);
    }

    #[test]
    fn drops_oldest_user_turn_first() {
        let long_content = "x".repeat(2000); // ~1000 token (over-estimate)
        let mut msgs = vec![
            msg("system", "sys"),
            msg("user", &long_content),
            msg("assistant", &long_content),
            msg("user", "second turn"),
            msg("assistant", "second answer"),
        ];
        truncate_history(&mut msgs, 100);
        // system 必留；最老的 turn 应被删
        assert_eq!(msgs[0].role, "system");
        let long_still_there = msgs.iter().any(|m| {
            m.role == "user"
                && matches!(&m.content, Some(Content::Text(t)) if t == long_content.as_str())
        });
        assert!(!long_still_there, "old long user turn 应被删");
    }

    #[test]
    fn keeps_only_system_if_all_too_long() {
        let mut msgs = vec![
            msg("system", "sys"),
            msg("user", "u"),
            msg("assistant", "a"),
        ];
        truncate_history(&mut msgs, 5); // 极低阈值
        // 至少 system 还在
        assert!(msgs.iter().any(|m| m.role == "system"));
    }
}

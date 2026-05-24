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

/// v2 Stage 3.1：tool_result clearing —— 替代整 turn truncation，对小模型更友好。
///
/// 思路（来自 [Anthropic context engineering cookbook](https://platform.claude.com/cookbook/tool-use-context-engineering-context-engineering-tools)）：
/// - 保留所有 system / user / assistant 消息原样
/// - 保留**最近 N** 个 tool result 消息原样
/// - 把更早的 tool result 内容**替换成占位符**：`[cleared, can re-call <tool_id>]`
///   - tool_call_id 仍保留，让 assistant 消息的 tool_calls 对应关系完整（OpenAI 协议要求）
///   - 模型看到占位符就知道：要再用这个数据的话，重新调一次工具
///
/// 比按 turn 删除更温和：保留对话脉络（"我之前调过哪些工具，问过什么"），只丢弃 stdout 字节。
/// 对 Qwen3-4B 这种小模型尤其重要 —— 不会"忘掉自己刚做过什么"。
///
/// 参数：
/// - `messages`: 待处理的 OpenAI 消息列表（原地改）
/// - `keep_recent_tool_results`: 保留最近 N 个 tool result 内容原样（默认 4）
///
/// 返回：被 cleared 的 tool result 数量。
pub fn clear_old_tool_results(
    messages: &mut [OpenAiMessage],
    keep_recent_tool_results: usize,
) -> usize {
    // 倒序找 tool result，保留最近 N 个；前面的 cleared
    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(i, m)| if m.role == "tool" { Some(i) } else { None })
        .collect();

    if tool_indices.len() <= keep_recent_tool_results {
        return 0; // 数量不足以触发 clearing
    }

    let cutoff = tool_indices.len() - keep_recent_tool_results;
    let to_clear: Vec<usize> = tool_indices[..cutoff].to_vec();
    let cleared = to_clear.len();

    for idx in to_clear {
        let tool_call_id = messages[idx].tool_call_id.clone().unwrap_or_default();
        // 替换 content；保留 tool_call_id 让 OpenAI 协议配对完整
        messages[idx].content = Some(Content::Text(format!(
            "[cleared, can re-call tool with id={tool_call_id}]"
        )));
    }
    cleared
}

/// 组合策略：先 clear 老 tool results，再如有必要按 turn truncate。
///
/// 优先 clear 而非 truncate —— clear 是「丢字节但保结构」，truncate 是「整段抛弃」。
/// agent loop 默认调这个。
pub fn smart_truncate(messages: &mut Vec<OpenAiMessage>, max_input_tokens: usize) {
    // Step 1: 先 clear 老 tool results（最廉价的减 token 手段）
    clear_old_tool_results(messages, 4);

    // Step 2: 如果还超，再走 turn-level truncation
    truncate_history(messages, max_input_tokens);
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

    fn tool_msg(call_id: &str, content: &str) -> OpenAiMessage {
        OpenAiMessage {
            role: "tool".to_owned(),
            content: Some(Content::Text(content.to_owned())),
            tool_calls: None,
            tool_call_id: Some(call_id.to_owned()),
        }
    }

    #[test]
    fn clear_old_tool_results_keeps_recent_n() {
        let big = "result-with-1000-bytes-stdout".repeat(40); // simulate large stdout
        let mut msgs = vec![
            msg("system", "sys"),
            msg("user", "q1"),
            tool_msg("call_1", &big),
            tool_msg("call_2", &big),
            tool_msg("call_3", &big),
            tool_msg("call_4", &big),
            tool_msg("call_5", &big),
            tool_msg("call_6", &big),
        ];
        let cleared = clear_old_tool_results(&mut msgs, 4);
        assert_eq!(cleared, 2, "older 2 should be cleared, recent 4 kept");

        // call_1 + call_2 应被替换
        let m1 = &msgs[2];
        assert!(matches!(&m1.content, Some(Content::Text(t)) if t.contains("cleared")));
        assert_eq!(m1.tool_call_id.as_deref(), Some("call_1"));

        let m2 = &msgs[3];
        assert!(matches!(&m2.content, Some(Content::Text(t)) if t.contains("cleared")));

        // call_3 ~ call_6 应原样保留
        for i in 4..8 {
            assert!(
                matches!(&msgs[i].content, Some(Content::Text(t)) if t == big.as_str()),
                "msg index {i} should be intact",
            );
        }
    }

    #[test]
    fn clear_does_nothing_when_under_threshold() {
        let mut msgs = vec![
            msg("system", "sys"),
            msg("user", "q1"),
            tool_msg("call_1", "ok"),
            tool_msg("call_2", "ok"),
        ];
        let cleared = clear_old_tool_results(&mut msgs, 4);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn smart_truncate_clears_first_then_truncates() {
        let big = "x".repeat(800);
        let mut msgs = vec![
            msg("system", "sys"),
            msg("user", "q1"),
            tool_msg("call_1", &big),
            tool_msg("call_2", &big),
            tool_msg("call_3", &big),
            tool_msg("call_4", &big),
            tool_msg("call_5", &big),
            tool_msg("call_6", &big),
            msg("assistant", "done"),
        ];
        smart_truncate(&mut msgs, 2000);
        // system 在；至少 4 个 tool 完整或被清；不应该把整 user turn 删了
        assert_eq!(msgs[0].role, "system");
    }
}

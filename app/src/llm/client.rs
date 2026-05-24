//! OpenAI 兼容 chat completion 的同步 HTTP 客户端 helper。
//!
//! 阶段 3.2 起：`spawn_chat_request` / `ChatJob` / `ChatResult` 被
//! `agent::spawn_agent_request` 取代；这里只保留 `blocking_chat_completion`
//! 作为 agent loop 每一步的底层 HTTP helper。
//!
//! 阶段 4.2 起：加 `api_key` 参数 —— 远端 endpoint（DeepSeek、OpenAI 等）需要 Bearer 认证。
//!
//! 阶段 v2 Stage 2 起：加 `blocking_chat_completion_stream` —— SSE 流式输出，
//! 通过 reqwest blocking + 手动 BufReader 解析「data: {json}\n\n」格式，不引入 tokio。
//! 兼容 OpenAI 规范 + llama.cpp 行为（含 build 8233+ 的 [issue #20198](https://github.com/ggml-org/llama.cpp/issues/20198) 双形态 arguments）。
//!
//! 设计：reqwest blocking + 调用方自行 spawn 线程
//! - egui 是 immediate mode UI，每帧 update 刚好适合 poll channel 取结果
//! - 避免引入 tokio runtime 增加复杂度与二进制体积（PE 越小越好）

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use super::openai::{
    ChatCompletionRequest, ChatCompletionResponse, ChatCompletionStreamChunk, OpenAiMessage,
};

/// 发起一次同步 chat completion 请求（非流式），返回响应中第一个 message。
///
/// 参数：
/// - `endpoint`：端点根地址（不含 `/v1/chat/completions`），如 `http://127.0.0.1:8080`
/// - `api_key`：可选 API key；Some 时加 `Authorization: Bearer ...` 头
/// - `req`：完整的请求体（messages、tools 等）
///
/// 错误描述为中文，可直接呈现给用户。
#[allow(dead_code)] // v2 Stage 2 起 agent loop 默认走 stream 版本；这个 API 仍保留给将来 fallback
pub fn blocking_chat_completion(
    endpoint: &str,
    api_key: Option<&str>,
    req: &ChatCompletionRequest,
) -> Result<OpenAiMessage, String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("构造 HTTP 客户端失败：{e}"))?;

    let mut request = client.post(&url).json(req);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .map_err(|e| format!("请求 {url} 失败：{e}\n请确认端点已启动。"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!("server 返回 HTTP {status}: {body}"));
    }

    let parsed: ChatCompletionResponse = response
        .json()
        .map_err(|e| format!("解析 JSON 响应失败：{e}"))?;

    parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message)
        .ok_or_else(|| "模型返回 choices 为空".to_owned())
}

/// 流式 chat completion 的一次回调事件。
///
/// agent loop 消费 mpsc::Receiver<StreamEvent>，每收一个 Chunk 解析增量再决定怎么处理。
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 一个解析好的 SSE chunk（OpenAI 兼容 schema）
    Chunk(ChatCompletionStreamChunk),
    /// 流正常结束（收到 `data: [DONE]`）
    Done,
    /// 流出错（HTTP / parse / 网络）—— 字符串描述给用户看
    Error(String),
}

/// 发起一次流式 chat completion 请求，返回 mpsc::Receiver 实时拿 SSE chunk。
///
/// 设计：
/// - 内部 spawn 一个线程读 reqwest response body line by line
/// - 每收到一行 `data: {json}\n\n` 就 parse 成 ChatCompletionStreamChunk 推到 channel
/// - 收到 `data: [DONE]` 推 StreamEvent::Done 后退出
/// - 出错推 StreamEvent::Error 后退出
/// - `cancel` 是共享标志：UI 点「停止生成」会 set 它，worker 每次读 line 前检查
///
/// 该函数本身阻塞到 HTTP 响应 headers 到达，然后立刻返回 receiver；body 读取在新线程。
/// 调用方拿到 receiver 后**用 try_recv 在 UI 主循环 poll**，跟现有 agent / UI 模式一致。
pub fn blocking_chat_completion_stream(
    endpoint: &str,
    api_key: Option<&str>,
    req: &ChatCompletionRequest,
    cancel: Arc<AtomicBool>,
) -> Result<mpsc::Receiver<StreamEvent>, String> {
    // 强制 stream: true（调用方有可能忘了设；我们这里兜底）
    let mut req = req.clone();
    req.stream = true;

    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        // 流式可能很长（模型边推理边返回），整体超时给宽 —— 单 chunk 没有超时
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("构造 HTTP 客户端失败：{e}"))?;

    let mut request = client.post(&url).json(&req);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .map_err(|e| format!("请求 {url} 失败：{e}\n请确认端点已启动。"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!("server 返回 HTTP {status}: {body}"));
    }

    let (tx, rx) = mpsc::channel();

    // 转 body → BufReader，行式读取 SSE event
    std::thread::spawn(move || {
        let reader = BufReader::new(response);
        for line in reader.lines() {
            // 取消检查放在每行读完之后，最长可能延迟 = 一行的网络传输时间
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(StreamEvent::Done);
                return;
            }

            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(format!("读流失败：{e}")));
                    return;
                }
            };

            // SSE event 之间是空行分隔；comment 行以 `:` 开头；都跳过
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            // 数据行格式 `data: <payload>`
            let payload = match line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")) {
                Some(p) => p.trim(),
                None => continue, // 非 data: 行（如 event:）忽略
            };

            // 终止信号
            if payload == "[DONE]" {
                let _ = tx.send(StreamEvent::Done);
                return;
            }

            // 解析为 OpenAI stream chunk JSON
            match serde_json::from_str::<ChatCompletionStreamChunk>(payload) {
                Ok(chunk) => {
                    if tx.send(StreamEvent::Chunk(chunk)).is_err() {
                        return; // receiver 已 drop，UI 关了
                    }
                }
                Err(e) => {
                    // 个别 chunk 解析失败不致命；记录并继续（防御 server 杂质）
                    let _ = tx.send(StreamEvent::Error(format!(
                        "解析 SSE chunk 失败：{e}\nchunk: {payload}"
                    )));
                    // 继续读下一个 chunk —— 不 return
                }
            }
        }

        // EOF 但没有 [DONE]：服务器可能挂了。当作 Done 走收尾。
        let _ = tx.send(StreamEvent::Done);
    });

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::openai::ChatCompletionStreamChunk;

    /// SSE chunk JSON 解析烟雾测试（涵盖 content 增量场景）
    #[test]
    fn parse_content_delta_chunk() {
        let payload = r#"{"id":"chatcmpl-x","object":"chat.completion.chunk","created":1700000000,"model":"qwen3-4b-instruct","choices":[{"index":0,"delta":{"role":"assistant","content":"你"},"finish_reason":null}]}"#;
        let parsed: ChatCompletionStreamChunk = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        let choice = &parsed.choices[0];
        assert_eq!(choice.index, 0);
        assert_eq!(choice.delta.content.as_deref(), Some("你"));
        assert!(choice.finish_reason.is_none());
    }

    /// 最后一个 chunk 通常 delta 空 + finish_reason="stop"
    #[test]
    fn parse_final_chunk() {
        let payload = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let parsed: ChatCompletionStreamChunk = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
        assert!(parsed.choices[0].delta.content.is_none());
    }

    /// tool_calls 流式 chunk（第一个 chunk 含 id + name）
    #[test]
    fn parse_tool_call_first_chunk() {
        let payload = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"list_disks","arguments":""}}]},"finish_reason":null}]}"#;
        let parsed: ChatCompletionStreamChunk = serde_json::from_str(payload).unwrap();
        let tcs = parsed.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].index, 0);
        assert_eq!(tcs[0].id.as_deref(), Some("call_abc"));
        let func = tcs[0].function.as_ref().unwrap();
        assert_eq!(func.name.as_deref(), Some("list_disks"));
    }

    /// tool_calls 流式 chunk（后续 chunk 只含 index + arguments 增量）
    #[test]
    fn parse_tool_call_subsequent_chunk() {
        let payload = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]},"finish_reason":null}]}"#;
        let parsed: ChatCompletionStreamChunk = serde_json::from_str(payload).unwrap();
        let tcs = parsed.choices[0].delta.tool_calls.as_ref().unwrap();
        assert!(tcs[0].id.is_none(), "subsequent chunk should NOT carry id");
        let arg_val = tcs[0].function.as_ref().unwrap().arguments.as_ref().unwrap();
        assert!(arg_val.is_string());
    }

    /// finish_reason=tool_calls 表示模型决定调工具，agent 应停止累积并 dispatch
    #[test]
    fn parse_finish_reason_tool_calls() {
        let payload = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
        let parsed: ChatCompletionStreamChunk = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("tool_calls"));
    }
}

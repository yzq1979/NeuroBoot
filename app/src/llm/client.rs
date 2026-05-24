//! OpenAI 兼容 chat completion 的同步 HTTP 客户端 helper。
//!
//! 阶段 3.2 起：`spawn_chat_request` / `ChatJob` / `ChatResult` 被
//! `agent::spawn_agent_request` 取代；这里只保留 `blocking_chat_completion`
//! 作为 agent loop 每一步的底层 HTTP helper。
//!
//! 阶段 4.2 起：加 `api_key` 参数 —— 远端 endpoint（DeepSeek、OpenAI 等）需要 Bearer 认证。
//!
//! 设计：reqwest blocking + 调用方自行 spawn 线程
//! - egui 是 immediate mode UI，每帧 update 刚好适合 poll channel 取结果
//! - 避免引入 tokio runtime 增加复杂度与二进制体积（PE 越小越好）

use std::time::Duration;

use super::openai::{ChatCompletionRequest, ChatCompletionResponse, OpenAiMessage};

/// 发起一次同步 chat completion 请求，返回响应中第一个 message。
///
/// 参数：
/// - `endpoint`：端点根地址（不含 `/v1/chat/completions`），如 `http://127.0.0.1:8080`
/// - `api_key`：可选 API key；Some 时加 `Authorization: Bearer ...` 头
/// - `req`：完整的请求体（messages、tools 等）
///
/// 错误描述为中文，可直接呈现给用户。
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

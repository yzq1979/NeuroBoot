//! MCP (Model Context Protocol) stdio server —— v2 Stage 8。
//!
//! 让外部 MCP 客户端（Claude Desktop / Cline / Continue.dev / 等）通过 stdio JSON-RPC
//! 调 NeuroBoot 的诊断工具。**只暴露 safe 工具**（11 个），dangerous 类不通过 MCP 暴露
//! —— 远端 agent 没有 NeuroBoot 的 UI 弹窗确认能力，dangerous 工具会变成无监督的远程修改。
//!
//! 协议参考：https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/
//! 自实现而非用 rmcp crate 的理由：
//! - rmcp 用 tokio，NeuroBoot 全栈 sync（避 tokio 增大二进制 ~3-10 MB）
//! - MCP 协议本身简单 —— JSON-RPC 2.0 + 3 个核心方法（initialize / tools/list / tools/call）
//! - 自实现 ~250 行，无新依赖，完全控制行为
//!
//! 启动方式：`neuroboot.exe --mcp-server` —— 切到 stdio 模式，不启 GUI。
//! 客户端配置示例（Claude Desktop `claude_desktop_config.json`）：
//! ```json
//! {
//!   "mcpServers": {
//!     "neuroboot": {
//!       "command": "C:\\NeuroBoot\\app\\target\\release\\neuroboot.exe",
//!       "args": ["--mcp-server"]
//!     }
//!   }
//! }
//! ```

use std::io::{self, BufRead, BufReader, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::{SafetyClass, ToolRegistry};

/// JSON-RPC 2.0 请求结构。
#[derive(Debug, Clone, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    #[serde(default = "default_jsonrpc")]
    jsonrpc: String,
    /// 缺 id = notification（无响应）；有 id = request
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn default_jsonrpc() -> String {
    "2.0".to_owned()
}

/// JSON-RPC 2.0 响应结构。
#[derive(Debug, Clone, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// 跑 MCP 服务器，阻塞当前线程；从 stdin 读 JSON-RPC 请求，往 stdout 写响应。
///
/// 每个请求一行 JSON。stderr 用于日志（避免污染 stdout 的协议数据）。
/// 遇 EOF（client 关闭）退出。
pub fn run_mcp_server(tool_registry: Arc<ToolRegistry>) {
    eprintln!("[NeuroBoot MCP] server start, awaiting JSON-RPC on stdin");

    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[NeuroBoot MCP] stdin read err: {e}");
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[NeuroBoot MCP] parse err: {e}");
                // 无 id 时无法回错误响应；忽略并继续
                continue;
            }
        };

        // notifications 无 id —— 不响应
        let Some(id) = req.id.clone() else {
            eprintln!("[NeuroBoot MCP] got notification: {}", req.method);
            continue;
        };

        let response = handle_request(&req, &tool_registry, id);
        let response_json = match serde_json::to_string(&response) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[NeuroBoot MCP] serialize err: {e}");
                continue;
            }
        };

        // stdout 写一行；按 NDJSON / JSON-RPC over stdio 惯例
        {
            let mut out = stdout.lock();
            if let Err(e) = writeln!(out, "{response_json}") {
                eprintln!("[NeuroBoot MCP] stdout write err: {e}");
                break;
            }
            let _ = out.flush();
        }
    }

    eprintln!("[NeuroBoot MCP] server exit");
}

/// 派发一个 MCP 请求。
fn handle_request(req: &JsonRpcRequest, registry: &ToolRegistry, id: Value) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            // 协议握手。返回服务器能力 + 协议版本
            JsonRpcResponse::ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false }
                    },
                    "serverInfo": {
                        "name": "neuroboot",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
        }
        "tools/list" => {
            // 列所有 safe 工具（dangerous 不暴露）
            let tools: Vec<Value> = registry
                .all()
                .filter(|t| matches!(t.safety(), SafetyClass::Safe))
                .map(|t| {
                    json!({
                        "name": t.name(),
                        "description": t.description(),
                        "inputSchema": t.parameters_schema()
                    })
                })
                .collect();
            JsonRpcResponse::ok(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let tool_name = match req.params.get("name").and_then(Value::as_str) {
                Some(n) => n,
                None => return JsonRpcResponse::err(id, -32602, "missing 'name' parameter"),
            };
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let tool = match registry.get(tool_name) {
                Some(t) => t,
                None => {
                    return JsonRpcResponse::err(id, -32602, format!("unknown tool: {tool_name}"))
                }
            };

            // **安全护栏**：MCP 客户端不能调 dangerous 工具
            if matches!(tool.safety(), SafetyClass::Dangerous) {
                return JsonRpcResponse::err(
                    id,
                    -32603,
                    format!(
                        "tool '{tool_name}' is dangerous; MCP server only exposes safe tools. \
                         Run it via NeuroBoot GUI (with user confirmation) instead."
                    ),
                );
            }

            match tool.execute(&args) {
                Ok(output) => JsonRpcResponse::ok(
                    id,
                    json!({
                        "content": [
                            { "type": "text", "text": output }
                        ],
                        "isError": false
                    }),
                ),
                Err(e) => JsonRpcResponse::ok(
                    id,
                    json!({
                        "content": [
                            { "type": "text", "text": e.display_for_model() }
                        ],
                        "isError": true
                    }),
                ),
            }
        }
        // ping 是常见心跳；MCP 规范里可选
        "ping" => JsonRpcResponse::ok(id, json!({})),
        // 未识别方法
        m => JsonRpcResponse::err(id, -32601, format!("method not found: {m}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_initialize_request() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(json!(1)));
    }

    #[test]
    fn parse_notification_no_id() {
        let raw = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert!(req.id.is_none());
    }

    #[test]
    fn response_serializes_correctly() {
        let r = JsonRpcResponse::ok(json!(1), json!({"ok": true}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"id\":1"));
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn error_response_no_result() {
        let r = JsonRpcResponse::err(json!(2), -32601, "method not found");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
    }
}

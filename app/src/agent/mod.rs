//! Agent 子模块：tool-use 多轮循环 + dangerous 工具人工确认关卡。
//!
//! 流程：
//!   用户消息 → LLM → 解析 tool_calls → 按 safety 分支：
//!     - Safe：直接执行
//!     - Dangerous：发 ConfirmationRequest 给 UI → 阻塞等用户决定 → 执行 / 拒绝
//!   → tool results 回传 LLM → 再决策 → … 上限 5 轮。
//!
//! 设计：
//! - worker 线程跑同步循环，每一步通过 mpsc channel 把 `AgentEvent` 送给 UI
//! - UI 每帧 try_recv 把 events 转成可视消息，让用户看到思考链
//! - 阶段 4.1 加 truncate：每轮发请求前估算 token，超阈值时按整 turn 删最早对话
//! - 阶段 4.2 加 api_key：调 blocking_chat_completion 时透传给远端
//! - 阶段 4.3 加 ConfirmationRequest：dangerous tool 走双向 channel，UI 弹窗确认

mod truncate;

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use serde_json::Value;

use crate::llm::client::blocking_chat_completion;
use crate::llm::openai::{ChatCompletionRequest, OpenAiMessage, ToolDefinition};
use crate::tools::{SafetyClass, ToolRegistry};
use crate::ui::chat::{ChatMessage, Role};

/// 单次 agent 请求所需的全部上下文。
pub struct AgentJob {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub system_prompt: String,
    pub messages: Vec<ChatMessage>,
    pub tool_registry: Arc<ToolRegistry>,
}

/// Agent 边跑边向 UI 发送的事件。
///
/// 没 derive Debug —— ConfirmationRequest 内含 mpsc::Sender 不实现 Debug。
pub enum AgentEvent {
    /// 新产生一条消息（assistant 文本/工具调用、tool 结果、或终态提示）
    Message(ChatMessage),
    /// 整轮结束 —— UI 应解锁输入框、清空 pending_response
    Done,
    /// 出错描述（中文，UI 包成一条 assistant 错误消息显示）
    Error(String),
    /// Agent 想调 dangerous 工具 —— UI 必须弹窗让用户决定，
    /// 通过 responder 把决定送回 worker（worker 阻塞等待）
    Confirmation(ConfirmationRequest),
}

/// 危险工具调用前的确认请求。
///
/// UI 收到此事件后：
/// 1. 把它存进 NeuroBootApp.pending_confirmation
/// 2. 在 ui() 里画 modal Window 显示工具名 + 参数
/// 3. 用户点击「确认」/「取消」→ 通过 `responder` send 决定
/// 4. Worker 的 `resp_rx.recv()` unblock → 继续 agent loop
pub struct ConfirmationRequest {
    pub tool_name: String,
    /// 模型生成的参数 JSON 字符串（原样，方便人审）
    pub arguments: String,
    /// UI 把用户决定送回 worker 的发送端
    pub responder: mpsc::Sender<ConfirmationResponse>,
}

/// 用户对确认请求的回应。
#[derive(Debug, Clone, Copy)]
pub enum ConfirmationResponse {
    /// 同意执行
    Confirm,
    /// 拒绝执行（worker 会把"用户拒绝"作为 tool 结果回传给 LLM）
    Reject,
}

/// 最大循环轮数（防死循环 + 防工具被滥用）。
const MAX_ROUNDS: usize = 5;

/// 每次请求前 truncate 的输入 token 阈值。
const MAX_INPUT_TOKENS: usize = 13000;

/// 起一个后台线程跑 agent 循环；返回 receiver 端供 UI 每帧 try_recv。
pub fn spawn_agent_request(job: AgentJob) -> mpsc::Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        run_agent_loop(job, &tx);
    });
    rx
}

/// 在 worker 线程里跑 agent 循环。
fn run_agent_loop(job: AgentJob, tx: &mpsc::Sender<AgentEvent>) {
    // 构造发给 LLM 的初始消息：system + UI 历史
    let mut api_messages: Vec<OpenAiMessage> = Vec::new();
    if !job.system_prompt.is_empty() {
        api_messages.push(OpenAiMessage::from_chat(&ChatMessage::system(
            job.system_prompt.clone(),
        )));
    }
    for msg in &job.messages {
        api_messages.push(OpenAiMessage::from_chat(msg));
    }

    // 一次性构造 tools 清单（多轮间不变）
    let tools = build_tools(&job.tool_registry);

    for _ in 0..MAX_ROUNDS {
        // 阶段 4.1：发送前 truncate 历史，防超 server ctx
        truncate::truncate_history(&mut api_messages, MAX_INPUT_TOKENS);

        // ----- 一次 LLM 调用 -----
        let req = ChatCompletionRequest {
            model: job.model.clone(),
            messages: api_messages.clone(),
            temperature: Some(0.7),
            max_tokens: Some(2048),
            stream: false,
            tools: tools.clone(),
        };

        let response_msg =
            match blocking_chat_completion(&job.endpoint, job.api_key.as_deref(), &req) {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(e));
                    return;
                }
            };

        // 推入 api_messages 供下一轮 LLM 看见
        api_messages.push(response_msg.clone());

        // 拆出 content 与 tool_calls
        // content_text() 在多模态响应里把所有 Text part 合并；当前 assistant
        // 一般只返回纯文本，这里加 untagged enum 兼容也无开销
        let content = response_msg.content_text();
        let tool_calls = response_msg.tool_calls.clone().unwrap_or_default();

        // 把这一轮的 assistant 消息发给 UI
        let ui_msg = ChatMessage {
            role: Role::Assistant,
            content,
            tool_calls: tool_calls.iter().map(|tc| tc.to_summary()).collect(),
            tool_call_id: None,
            images: Vec::new(),
        };
        if tx.send(AgentEvent::Message(ui_msg)).is_err() {
            return; // UI 已关
        }

        // 无 tool_calls = 已是最终答案
        if tool_calls.is_empty() {
            let _ = tx.send(AgentEvent::Done);
            return;
        }

        // ----- 执行每个 tool call -----
        for tc in &tool_calls {
            let tool_name = tc.function.name.as_str();
            let raw_args = tc.function.arguments.as_str();
            let args: Value = serde_json::from_str(raw_args).unwrap_or(Value::Null);

            let result_text = match job.tool_registry.get(tool_name) {
                None => format!("（错误）未找到工具 `{tool_name}`"),
                Some(tool) => match tool.safety() {
                    SafetyClass::Safe => match tool.execute(&args) {
                        Ok(s) => s,
                        Err(e) => format!("（工具错误）{}", e.message),
                    },
                    SafetyClass::Dangerous => {
                        // 阶段 4.3：dangerous 工具 → 发确认请求 → 阻塞等用户
                        let (resp_tx, resp_rx) = mpsc::channel::<ConfirmationResponse>();
                        let confirmation = ConfirmationRequest {
                            tool_name: tool_name.to_owned(),
                            arguments: raw_args.to_owned(),
                            responder: resp_tx,
                        };
                        if tx.send(AgentEvent::Confirmation(confirmation)).is_err() {
                            return; // UI 已关
                        }
                        // 阻塞等用户在 UI 上点按钮
                        match resp_rx.recv() {
                            Ok(ConfirmationResponse::Confirm) => match tool.execute(&args) {
                                Ok(s) => format!("（已执行）{s}"),
                                Err(e) => format!("（工具错误）{}", e.message),
                            },
                            Ok(ConfirmationResponse::Reject) => {
                                "（用户拒绝）用户拒绝执行此危险操作。请告诉用户你已停止该操作，并询问是否要尝试其它方式。".to_owned()
                            }
                            Err(_) => "（错误）确认通道意外关闭".to_owned(),
                        }
                    }
                },
            };

            let tool_msg = ChatMessage::tool_result(tc.id.clone(), result_text.clone());
            api_messages.push(OpenAiMessage::from_chat(&tool_msg));

            if tx.send(AgentEvent::Message(tool_msg)).is_err() {
                return;
            }
        }
    }

    let _ = tx.send(AgentEvent::Message(ChatMessage::assistant(
        "（提示）Agent 已达到最大轮数限制（5 轮），强制结束。\
         如需继续，请重新提问或拆分任务。",
    )));
    let _ = tx.send(AgentEvent::Done);
}

/// 把 ToolRegistry 转成 OpenAI tools 清单；空 registry 返回 None。
fn build_tools(registry: &ToolRegistry) -> Option<Vec<ToolDefinition>> {
    if registry.is_empty() {
        return None;
    }
    Some(
        registry
            .all()
            .map(|t| ToolDefinition::function(t.name(), t.description(), t.parameters_schema()))
            .collect(),
    )
}

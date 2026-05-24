//! Agent 子模块：tool-use 多轮循环 + dangerous 工具人工确认关卡 + 流式输出。
//!
//! 流程：
//!   用户消息 → LLM（流式 SSE）→ 流式 chunk append 到 UI → 解析 tool_calls →
//!   按 safety 分支：
//!     - Safe：直接执行
//!     - Dangerous：发 ConfirmationRequest 给 UI → 阻塞等用户决定 → 执行 / 拒绝
//!   → tool results 回传 LLM → 再决策 → … 上限 5 轮。
//!
//! 设计：
//! - worker 线程跑同步循环，每一步通过 mpsc channel 把 `AgentEvent` 送给 UI
//! - UI 每帧 try_recv 把 events 转成可视消息，让用户看到思考链 + 流式 token 增量
//! - 阶段 4.1 加 truncate：每轮发请求前估算 token，超阈值时按整 turn 删最早对话
//! - 阶段 4.2 加 api_key：调 chat completion 时透传给远端
//! - 阶段 4.3 加 ConfirmationRequest：dangerous tool 走双向 channel，UI 弹窗确认
//! - **阶段 v2 Stage 2** 加流式：blocking_chat_completion_stream 边读 SSE chunk 边
//!   send AgentEvent::TokenChunk(s) 让 UI 实时 append；tool_calls 跨 chunk 按 index 累积；
//!   `finish_reason: "tool_calls"` 才 dispatch 工具

mod truncate;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use serde_json::Value;

use crate::hooks::{self, HooksConfig};
use crate::llm::client::{blocking_chat_completion_stream, StreamEvent};
use crate::llm::openai::{
    ChatCompletionRequest, OpenAiMessage, ToolCall, ToolCallFunction, ToolDefinition,
};
use crate::tools::audit_log;
use crate::tools::{SafetyClass, ToolRegistry};
use crate::ui::chat::{ChatMessage, Role, ToolCallSummary};

/// 单次 agent 请求所需的全部上下文。
pub struct AgentJob {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub system_prompt: String,
    pub messages: Vec<ChatMessage>,
    pub tool_registry: Arc<ToolRegistry>,
    /// 共享取消标志 —— UI 点「停止生成」会 set 为 true，worker 检测后清理退出
    pub cancel: Arc<AtomicBool>,
    /// v3.0 W6-7 hooks 配置（PreToolUse / PostToolUse / Stop 用）
    pub hooks_config: Arc<HooksConfig>,
}

/// Agent 边跑边向 UI 发送的事件。
///
/// **流式扩展（v2 Stage 2）**：
/// - `AssistantStart`：模型开始生成一条新 assistant message。UI 推一个空 ChatMessage 占位
/// - `TokenChunk(s)`：assistant 文本增量。UI 追加到当前 active assistant message.content
/// - `AssistantToolCalls(...)`：本轮 assistant 生成的完整 tool_calls 列表（流末才发，
///   因为要等所有 chunk 累积完）。UI 把这个加到当前 active message.tool_calls
/// - `Message(...)` 仍保留 —— 用于 Tool result 等一次性消息
///
/// 没 derive Debug —— ConfirmationRequest 内含 mpsc::Sender 不实现 Debug。
pub enum AgentEvent {
    /// 流式 assistant message 开始：UI 应推一个空 assistant message 作为后续 chunk 的容器
    AssistantStart,
    /// 文本增量：UI 追加到当前 active assistant message
    TokenChunk(String),
    /// 流末，本轮 assistant 生成的完整 tool_calls：UI 把它附加到当前 active assistant message
    AssistantToolCalls(Vec<ToolCallSummary>),
    /// 一次性的完整消息（tool result / error 提示等）
    Message(ChatMessage),
    /// 整轮结束 —— UI 应解锁输入框、清空 pending_response
    Done,
    /// 出错描述（中文，UI 包成一条 assistant 错误消息显示）
    Error(String),
    /// Agent 想调 dangerous 工具 —— UI 必须弹窗让用户决定，
    /// 通过 responder 把决定送回 worker（worker 阻塞等待）
    Confirmation(ConfirmationRequest),
    /// v3.0 W3-4: Agent 调了 propose_plan 工具 —— UI 渲染 plan 让用户审批，
    /// 决定通过 responder 回传。批准 / 拒绝映射到合成 tool result 回灌 LLM
    PlanProposal(PlanProposalRequest),
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

/// v3.0 W3-4: Plan Mode 单个步骤的简化视图（从 propose_plan 的 args 解析）。
///
/// 与 propose_plan tool 的 parameters_schema 中 steps[] 元素对齐。
#[derive(Debug, Clone)]
pub struct PlanStep {
    pub tool: String,
    pub args_preview: String,
    pub why: String,
    /// "safe" / "dangerous"（可选；空 = 未标）
    pub safety: String,
}

/// v3.0 W3-4: propose_plan 工具触发的 plan 审批请求。
///
/// UI 收到后：
/// 1. 存进 NeuroBootApp.pending_plan
/// 2. 渲染 plan modal Window 显示 summary + steps（dangerous 步骤红字）
/// 3. 用户点 Approve / Reject → responder send
/// 4. Worker 的 resp_rx.recv() unblock → 合成 tool result 回灌 LLM
pub struct PlanProposalRequest {
    /// AI 给出的整体计划描述
    pub summary: String,
    /// 步骤清单（已从 args 的 steps[] 解析过；无效条目跳过）
    pub steps: Vec<PlanStep>,
    /// 原始 propose_plan 工具的 tool_call.id —— 合成 tool result 时回填
    pub tool_call_id: String,
    /// UI 把用户决定送回 worker 的发送端
    pub responder: mpsc::Sender<PlanResponse>,
}

/// v3.0 W3-4: 用户对 plan 的回应。
#[derive(Debug, Clone, Copy)]
pub enum PlanResponse {
    /// 批准：worker 合成 "（已批准）请按 steps 执行" 作为 tool result 回灌
    Approve,
    /// 拒绝：worker 合成 "（拒绝）请重新规划" 作为 tool result 回灌
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

/// 跨 chunk 累积一个 tool_call 的中间状态。
#[derive(Debug, Default, Clone)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String, // 跨 chunk append
}

/// v3.0 W6-7：发 Done 前先跑 Stop hook（exit code 不影响流程）。
/// 取代之前裸的 `tx.send(AgentEvent::Done)`。
fn finish_turn(tx: &mpsc::Sender<AgentEvent>, hooks_config: &HooksConfig) {
    let _ = hooks::run_stop(hooks_config);
    let _ = tx.send(AgentEvent::Done);
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
        // 取消提前检查
        if job.cancel.load(Ordering::Relaxed) {
            finish_turn(tx, &job.hooks_config);
            return;
        }

        // 阶段 v2 Stage 3.1：先 clear 老 tool results 再按 turn truncate（小模型更友好）
        truncate::smart_truncate(&mut api_messages, MAX_INPUT_TOKENS);

        // ----- 一次 LLM 流式调用 -----
        let req = ChatCompletionRequest {
            model: job.model.clone(),
            messages: api_messages.clone(),
            temperature: Some(0.7),
            max_tokens: Some(2048),
            stream: true, // v2 Stage 2: 流式
            tools: tools.clone(),
            cache_prompt: true, // v3 Quick Win 1: 启用 prompt KV cache 复用
        };

        let stream_rx = match blocking_chat_completion_stream(
            &job.endpoint,
            job.api_key.as_deref(),
            &req,
            Arc::clone(&job.cancel),
        ) {
            Ok(rx) => rx,
            Err(e) => {
                let _ = tx.send(AgentEvent::Error(e));
                return;
            }
        };

        // 通知 UI：新的 assistant message 开始累积
        if tx.send(AgentEvent::AssistantStart).is_err() {
            return; // UI 已关
        }

        // 累积本轮：完整文本 + tool_calls (按 index)
        let mut accumulated_content = String::new();
        let mut tool_call_accs: BTreeMap<u32, ToolCallAccumulator> = BTreeMap::new();
        let mut finish_reason: Option<String> = None;
        let mut stream_error: Option<String> = None;

        for event in stream_rx {
            if job.cancel.load(Ordering::Relaxed) {
                break;
            }
            match event {
                StreamEvent::Chunk(chunk) => {
                    for choice in chunk.choices {
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() {
                                accumulated_content.push_str(&content);
                                if tx.send(AgentEvent::TokenChunk(content)).is_err() {
                                    return;
                                }
                            }
                        }
                        if let Some(tc_deltas) = choice.delta.tool_calls {
                            for delta in tc_deltas {
                                let acc = tool_call_accs.entry(delta.index).or_default();
                                if let Some(id) = delta.id {
                                    acc.id = id;
                                }
                                if let Some(func) = delta.function {
                                    if let Some(name) = func.name {
                                        acc.name.push_str(&name);
                                    }
                                    if let Some(arg_val) = func.arguments {
                                        // 兼容 llama.cpp build 8233+ #20198：value 可能是 string OR object
                                        let arg_str = match arg_val {
                                            Value::String(s) => s,
                                            other => other.to_string(),
                                        };
                                        acc.arguments.push_str(&arg_str);
                                    }
                                }
                            }
                        }
                        if let Some(fr) = choice.finish_reason {
                            finish_reason = Some(fr);
                        }
                    }
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    stream_error = Some(e);
                    break;
                }
            }
        }

        if let Some(e) = stream_error {
            let _ = tx.send(AgentEvent::Error(e));
            return;
        }

        // 取消时 finish_reason 可能没收到 —— 干净退出
        if job.cancel.load(Ordering::Relaxed) {
            finish_turn(tx, &job.hooks_config);
            return;
        }

        // 把累积的 tool_calls 转 OpenAI 协议格式（送回 LLM 上下文 + UI 展示）
        let tool_calls: Vec<ToolCall> = tool_call_accs
            .into_values()
            .map(|acc| ToolCall {
                id: acc.id,
                kind: "function".to_owned(),
                function: ToolCallFunction {
                    name: acc.name,
                    arguments: acc.arguments,
                },
            })
            .collect();

        // 推入 api_messages 供下一轮 LLM 看见
        let assistant_msg = OpenAiMessage {
            role: "assistant".to_owned(),
            content: if accumulated_content.is_empty() {
                None
            } else {
                Some(crate::llm::openai::Content::Text(accumulated_content.clone()))
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.clone())
            },
            tool_call_id: None,
        };
        api_messages.push(assistant_msg);

        // 发 tool_calls 摘要给 UI（让 UI 把"工具：name(args)"行加到当前 assistant message）
        if !tool_calls.is_empty() {
            let summaries: Vec<ToolCallSummary> =
                tool_calls.iter().map(|tc| tc.to_summary()).collect();
            if tx
                .send(AgentEvent::AssistantToolCalls(summaries))
                .is_err()
            {
                return;
            }
        }

        // 无 tool_calls = 已是最终答案
        if tool_calls.is_empty() {
            finish_turn(tx, &job.hooks_config);
            return;
        }

        // ----- 执行每个 tool call -----
        for tc in &tool_calls {
            if job.cancel.load(Ordering::Relaxed) {
                finish_turn(tx, &job.hooks_config);
                return;
            }
            let tool_name = tc.function.name.as_str();
            let raw_args = tc.function.arguments.as_str();
            let args: Value = serde_json::from_str(raw_args).unwrap_or(Value::Null);

            // 时间窗口仅计算工具实际执行时间，不含用户在确认弹窗的等待时间
            let mut user_confirmed: Option<bool> = None;
            let mut safety_str = "safe";
            let exec_start = std::time::Instant::now();
            let exec_duration;
            let success;

            // v3.0 W3-4: 拦截 propose_plan 走 UI 双向 mpsc（参考 Dangerous 模式）
            // 不调 tool.execute()（那个路径仅 MCP / fallback 用）；纯 GUI 路径在此处理。
            let result_text = if tool_name == "propose_plan" {
                safety_str = "plan_proposal";
                // 解析 steps（兜底：解析失败回退到 fallback execute）
                let steps_parsed: Vec<PlanStep> = args
                    .get("steps")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                let tool = v.get("tool").and_then(Value::as_str)?.to_owned();
                                let why = v.get("why").and_then(Value::as_str)?.to_owned();
                                let args_preview = v
                                    .get("args_preview")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_owned();
                                let safety = v
                                    .get("safety")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_owned();
                                Some(PlanStep {
                                    tool,
                                    args_preview,
                                    why,
                                    safety,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let summary = args
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("(无 summary)")
                    .to_owned();

                if steps_parsed.is_empty() {
                    exec_duration = exec_start.elapsed();
                    success = false;
                    "（错误）propose_plan steps 解析为空 —— 请用 propose_plan 的正确 schema：\
                     {summary: string, steps: [{tool, why, args_preview?, safety?}]}"
                        .to_owned()
                } else {
                    let (resp_tx, resp_rx) = mpsc::channel::<PlanResponse>();
                    let req = PlanProposalRequest {
                        summary,
                        steps: steps_parsed,
                        tool_call_id: tc.id.clone(),
                        responder: resp_tx,
                    };
                    if tx.send(AgentEvent::PlanProposal(req)).is_err() {
                        return;
                    }
                    // 阻塞等用户决定（与 ConfirmationRequest 同模式）
                    match resp_rx.recv() {
                        Ok(PlanResponse::Approve) => {
                            user_confirmed = Some(true);
                            exec_duration = std::time::Duration::ZERO;
                            success = true;
                            "（用户已批准 plan）请按 steps 依次执行实际工具调用。\
                             完成所有步骤后给中文总结报告。不要再次调 propose_plan。"
                                .to_owned()
                        }
                        Ok(PlanResponse::Reject) => {
                            user_confirmed = Some(false);
                            exec_duration = std::time::Duration::ZERO;
                            success = false;
                            "（用户拒绝了 plan）请重新规划或问用户为什么拒绝。\
                             常见原因：某步多余 / 顺序不对 / 缺关键步骤 / 用户改主意。\
                             **不要原样重提同一 plan**。"
                                .to_owned()
                        }
                        Err(_) => {
                            exec_duration = std::time::Duration::ZERO;
                            success = false;
                            "（错误）plan 审批通道意外关闭".to_owned()
                        }
                    }
                }
            } else {
                // v3.0 W6-7: PreToolUse hook 在 tool.execute() 之前（也在 dangerous 确认弹窗之前）
                // 跑 —— hook 拒绝则不弹窗、不执行，直接把拒绝原因合成 tool result
                let (pre_outcome, _pre_execs) =
                    hooks::run_pre_tool_use(&job.hooks_config, tool_name, raw_args);
                if let hooks::HookOutcome::Block { reason } = pre_outcome {
                    exec_duration = exec_start.elapsed();
                    success = false;
                    safety_str = "hook_blocked";
                    format!("（hook 拒绝）{reason}")
                } else {
                    match job.tool_registry.get(tool_name) {
                None => {
                    exec_duration = exec_start.elapsed();
                    success = false;
                    format!("（错误）未找到工具 `{tool_name}`")
                }
                Some(tool) => match tool.safety() {
                    SafetyClass::Safe => {
                        let exec_inner_start = std::time::Instant::now();
                        let r = tool.execute(&args);
                        exec_duration = exec_inner_start.elapsed();
                        match r {
                            Ok(s) => {
                                success = true;
                                s
                            }
                            Err(e) => {
                                success = false;
                                format!("（工具错误）{}", e.display_for_model())
                            }
                        }
                    }
                    SafetyClass::Dangerous => {
                        safety_str = "dangerous";
                        let (resp_tx, resp_rx) = mpsc::channel::<ConfirmationResponse>();
                        let confirmation = ConfirmationRequest {
                            tool_name: tool_name.to_owned(),
                            arguments: raw_args.to_owned(),
                            responder: resp_tx,
                        };
                        if tx.send(AgentEvent::Confirmation(confirmation)).is_err() {
                            return;
                        }
                        match resp_rx.recv() {
                            Ok(ConfirmationResponse::Confirm) => {
                                user_confirmed = Some(true);
                                let exec_inner_start = std::time::Instant::now();
                                let r = tool.execute(&args);
                                exec_duration = exec_inner_start.elapsed();
                                match r {
                                    Ok(s) => {
                                        success = true;
                                        format!("（已执行）{s}")
                                    }
                                    Err(e) => {
                                        success = false;
                                        format!("（工具错误）{}", e.display_for_model())
                                    }
                                }
                            }
                            Ok(ConfirmationResponse::Reject) => {
                                user_confirmed = Some(false);
                                exec_duration = std::time::Duration::ZERO;
                                success = false;
                                "（用户拒绝）用户拒绝执行此危险操作。请告诉用户你已停止该操作，并询问是否要尝试其它方式。".to_owned()
                            }
                            Err(_) => {
                                exec_duration = std::time::Duration::ZERO;
                                success = false;
                                "（错误）确认通道意外关闭".to_owned()
                            }
                        }
                    }
                },
                }
                }
            };

            // v3.0 W6-7: PostToolUse hook —— exit code 仅记日志，不回灌 LLM
            // （工具结果已成型，PostHook 不应能"事后否决"）
            // 跳过 propose_plan（plan_proposal safety class，meta 工具不触发）
            if safety_str != "plan_proposal" {
                let _post_execs = hooks::run_post_tool_use(
                    &job.hooks_config,
                    tool_name,
                    raw_args,
                    success,
                    &result_text,
                );
            }

            // v2 Stage 3.2：写审计日志（失败静默，不影响工具结果回传）
            let audit = audit_log::build_audit(
                tool_name,
                raw_args,
                safety_str,
                user_confirmed,
                exec_duration,
                success,
                &result_text,
            );
            audit_log::append(&audit);

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
    finish_turn(tx, &job.hooks_config);
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

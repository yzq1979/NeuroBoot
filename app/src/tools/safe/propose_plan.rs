//! [Safe] propose_plan —— Plan Mode 入口（Cline 风格）。v3.0 W3-4。
//!
//! AI 在执行多步任务（>2 工具调用 OR 含 dangerous 工具）前先调本工具：
//! 1. 把完整计划（step 列表）提交给 UI
//! 2. UI 弹窗 / chat 内联展示 plan + Approve / Reject 按钮
//! 3. 用户决定后通过 mpsc responder 回传
//! 4. Approve → tool 返回「批准」→ AI 按 plan 执行
//! 5. Reject → tool 返回「拒绝」→ AI 重新思考或问用户
//!
//! ## 双模式行为
//!
//! - **GUI 模式**：本工具的 execute() **不会被调用** —— agent loop 拦截 tool name
//!   `propose_plan` 走双向 mpsc 与 UI 通信（参考 ConfirmationRequest 的实现路径）
//! - **MCP server 模式**：execute() 被调用，返回 placeholder 消息让 AI 继续
//!   （client 端如 Claude Desktop 自己有 UI 展示 plan，NeuroBoot 不阻塞）

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct ProposePlan;

impl Tool for ProposePlan {
    fn name(&self) -> &str {
        "propose_plan"
    }

    fn description(&self) -> &str {
        "**[Plan Mode 入口]** 多步任务执行前先提交计划给用户审批。Cline 风格。\n\
         \n\
         **When to use**: 满足任一条件就先调本工具，不要直接闷头跑：\n\
         - 预计要调 **> 2 个工具** 才能完成（如蓝屏诊断需 4+ 工具）\n\
         - 计划包含**任何 dangerous 工具**（chkdsk / sfc / delete_path / bootrec / 等）\n\
         - 用户**明确说**「先告诉我你要做什么」「先看下你的计划」\n\
         - 已 load_skill 拿到剧本 → 把 skill 的 step 列表作为 plan 提交给用户审\n\
         \n\
         **When NOT to use**: 单工具回答（如「我有几块硬盘」→ 直接 list_disks 不要 plan）；\
         纯文字对话不调任何工具时。\n\
         \n\
         **Parameters**:\n\
         - `summary` (string, required): 一句话整体计划描述（如「诊断蓝屏：4 步收证 + 1 步报告」）\n\
         - `steps` (array, required): 步骤列表，每个含：\n\
           - `tool` (string): 工具名（如 list_minidumps）\n\
           - `args_preview` (string): 参数预览（如 `{\"hours\": 72}`）\n\
           - `why` (string): 为什么这步（中文一句话）\n\
           - `safety` (string, optional): 'safe' / 'dangerous'，UI 标识用\n\
         \n\
         **Returns**: 用户决定的同步反馈：\n\
         - 批准 → `\"（用户已批准 plan）请按 steps 依次执行。完成后给中文总结。\"`\n\
         - 拒绝 → `\"（用户拒绝了 plan）请重新规划或问用户为什么拒绝（如某步多余 / 顺序不对 / 缺关键步骤）。\"`\n\
         \n\
         **Example output**: ```\n\
         （用户已批准 plan）请按 steps 依次执行。完成后给中文总结。\n\
         ```\n\
         \n\
         **Notes**: GUI 模式下本工具阻塞等用户点 Approve/Reject（最长 10 分钟超时）；\
         MCP 模式下立即返回 placeholder（远程 client 自己显示 plan）；\
         批准后不要再次调 propose_plan —— 直接调实际工具；\
         拒绝后**不要原样重提**，要重新思考。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "一句话整体计划描述"
                },
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string", "description": "工具名" },
                            "args_preview": { "type": "string", "description": "参数预览 JSON 字符串或简述" },
                            "why": { "type": "string", "description": "中文一句话说为什么这步" },
                            "safety": { "type": "string", "enum": ["safe", "dangerous"], "description": "可选：标记 dangerous 让 UI 醒目显示" }
                        },
                        "required": ["tool", "why"]
                    }
                }
            },
            "required": ["summary", "steps"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        // **GUI 模式下本路径不会被调用** —— agent loop 拦截 propose_plan tool name
        // 走 PlanProposal event 与 UI 双向 mpsc。这里仅为 MCP / 测试 / 兜底路径。
        let summary = args
            .get("summary")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 summary 参数")
            })?;
        let steps = args
            .get("steps")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 steps 参数（必须是数组）")
            })?;
        if steps.is_empty() {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                "steps 数组不能为空",
            ));
        }

        // MCP 模式 / 兜底：不阻塞 UI，告诉 AI 继续执行（client 自己负责展示）
        Ok(format!(
            "（非 GUI 模式，plan 已记录但未阻塞确认）summary={}, steps={}。\
             请直接按 steps 依次调用实际工具，无需再次 propose_plan。",
            summary,
            steps.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ProposePlan);
    }

    #[test]
    fn rejects_missing_summary() {
        let r = ProposePlan.execute(&json!({ "steps": [{"tool": "x", "why": "y"}] }));
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().kind, ToolErrorKind::InvalidArgument);
    }

    #[test]
    fn rejects_missing_steps() {
        let r = ProposePlan.execute(&json!({ "summary": "test" }));
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().kind, ToolErrorKind::InvalidArgument);
    }

    #[test]
    fn rejects_empty_steps() {
        let r = ProposePlan.execute(&json!({ "summary": "test", "steps": [] }));
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().kind, ToolErrorKind::InvalidArgument);
    }

    #[test]
    fn fallback_mode_accepts_valid_plan() {
        // MCP / fallback path: 不阻塞，直接返回 placeholder
        let r = ProposePlan.execute(&json!({
            "summary": "诊断蓝屏：3 步",
            "steps": [
                { "tool": "list_minidumps", "args_preview": "{}", "why": "看 dump 文件" },
                { "tool": "analyze_minidump", "args_preview": "{}", "why": "解析罪魁驱动" },
                { "tool": "read_event_log_errors", "args_preview": "{\"hours\":72}", "why": "看最近错误" }
            ]
        }));
        assert!(r.is_ok());
        let out = r.unwrap();
        assert!(out.contains("plan 已记录"));
        assert!(out.contains("steps=3"));
    }
}

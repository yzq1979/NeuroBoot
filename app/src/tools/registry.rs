//! Tool trait + ToolRegistry：所有 Agent 可调用工具的统一抽象与注册中心。
//!
//! 阶段 3.1 只搭框架，未注册任何具体工具；阶段 3.3 起在 `tools::safe::*` 下注册。
//! Agent 调用工具的流程（阶段 3.2 实现）：
//!   模型返回 tool_calls → registry.get(name) → tool.execute(args) → 把输出作为
//!   Role::Tool 消息回传 → 模型继续推理或给最终答案。

#![allow(dead_code)] // 阶段 3.1 框架先就位，3.2~3.3 起 wire up 后自然用上

use std::collections::BTreeMap;

use serde_json::Value;

/// 安全等级 —— 决定 Agent 是否可以自动执行该工具。
///
/// - `Safe`：只读 / 无副作用，Agent 决定调用就直接执行
/// - `Dangerous`：有副作用（写盘、删文件、改引导等），阶段 4 起会被
///   Agent 主循环拦截，弹窗展示具体命令 + 用户确认后才执行
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyClass {
    Safe,
    Dangerous,
}

/// 工具执行结果。
pub type ToolOutput = Result<String, ToolError>;

/// 工具执行的错误描述（中文，可直接给模型当 observation 看）。
#[derive(Debug, Clone)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// 所有可被 Agent 调用的工具实现此 trait。
///
/// JSON schema 用于在 OpenAI tools 字段里告诉模型工具签名 —— 模型据此构造调用参数。
pub trait Tool: Send + Sync {
    /// 工具唯一名字（snake_case），OpenAI tool_calls 通过此名匹配
    fn name(&self) -> &str;

    /// 中文描述，给模型看 —— 用来决策何时调用该工具
    fn description(&self) -> &str;

    /// 安全等级
    fn safety(&self) -> SafetyClass;

    /// 参数的 JSON Schema（OpenAI function calling 协议）
    /// 无参数则返回 `{"type": "object", "properties": {}}`
    fn parameters_schema(&self) -> Value;

    /// 用模型生成的参数 JSON 执行此工具，返回输出文本（给模型当 observation）
    fn execute(&self, args: &Value) -> ToolOutput;
}

/// 工具注册中心 —— 按名字注册和查找所有可用工具。
pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    /// 注册一个工具（按 `tool.name()` 索引）。
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// 按名字查找工具。
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| b.as_ref())
    }

    /// 遍历所有已注册工具（用于构造 OpenAI tools[] 清单）。
    pub fn all(&self) -> impl Iterator<Item = &dyn Tool> {
        self.tools.values().map(|b| b.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

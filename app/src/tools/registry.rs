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

/// 工具执行的错误分类 —— v2 Stage 3.4。
///
/// LLM 看到 kind 能更好决策（比单纯看 message 字符串）：
/// - `PermissionDenied`: 缺权限 → 告诉用户切 admin
/// - `NotFound`: 路径/资源不存在 → 别重试，问用户路径对不对
/// - `Timeout`: 操作超时 → 可以重试一次
/// - `ParseError`: 解析输出失败 → 工具本身 bug，跳过
/// - `InvalidArgument`: 参数不合法 → 调整参数重试
/// - `ExternalCommandFailed`: 外部命令非零退出 → 看 message 决定
/// - `Other`: 未分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    PermissionDenied,
    NotFound,
    Timeout,
    ParseError,
    InvalidArgument,
    ExternalCommandFailed,
    Other,
}

impl ToolErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolErrorKind::PermissionDenied => "permission_denied",
            ToolErrorKind::NotFound => "not_found",
            ToolErrorKind::Timeout => "timeout",
            ToolErrorKind::ParseError => "parse_error",
            ToolErrorKind::InvalidArgument => "invalid_argument",
            ToolErrorKind::ExternalCommandFailed => "external_command_failed",
            ToolErrorKind::Other => "other",
        }
    }
}

/// 工具执行的错误描述（中文 message + 机器可读 kind）。
///
/// 给模型当 observation 看的格式：`(<kind>) <message>`
#[derive(Debug, Clone)]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub message: String,
}

impl ToolError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Other,
            message: msg.into(),
        }
    }

    pub fn with_kind(kind: ToolErrorKind, msg: impl Into<String>) -> Self {
        Self {
            kind,
            message: msg.into(),
        }
    }

    /// 给模型看的格式化（带 kind 前缀）。
    pub fn display_for_model(&self) -> String {
        format!("({}) {}", self.kind.as_str(), self.message)
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

/// v3.0 W1 description 重写约定的统一断言（仅测试构建可用）。
///
/// 检查项（4 必需 + 长度 + 命名）：
/// 1. 必含 4 个 section marker：`**When to use**` / `**Parameters**` / `**Returns**` / `**Notes**`
/// 2. 长度在 [200, 1500] 字符
/// 3. name 为 snake_case（仅小写字母 + 下划线）
/// 4. name 含至少一个下划线（verb_object 形式）
///
/// **Example output** 是推荐但非必须（输出复杂的工具应该有；list_disks 的 6
/// 个详细单测仍保留作为示例参考）。每个工具的 tests 模块调用本函数即可。
///
/// 参考：[Anthropic: Writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
#[cfg(test)]
pub fn assert_v30_description_convention(tool: &dyn Tool) {
    let desc = tool.description();
    let name = tool.name();

    // 1. 必需的 4 个 section marker
    for marker in ["**When to use**", "**Parameters**", "**Returns**", "**Notes**"] {
        assert!(
            desc.contains(marker),
            "[{name}] description missing required section `{marker}`"
        );
    }

    // 2. 长度区间
    let len = desc.chars().count();
    assert!(
        (200..=1500).contains(&len),
        "[{name}] description length {len} out of band [200, 1500]"
    );

    // 3. name snake_case
    assert!(
        name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
        "[{name}] name must be snake_case (lowercase + underscore only)"
    );

    // 4. verb_object 含至少一个下划线
    assert!(
        name.contains('_'),
        "[{name}] name should be verb_object form (snake_case with at least one underscore)"
    );
}

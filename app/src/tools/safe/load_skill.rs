//! [Safe] load_skill —— Tier 2 加载：AI 触发读取 skill 完整 body markdown。
//!
//! v3.0 W1.5 progressive disclosure 核心入口。AI 看到 system prompt 里所有 skill
//! summary（Tier 1）后，判断某 skill 与用户请求相关时调本工具读取完整 body。
//!
//! 触发模式：
//! - 用户问「电脑蓝屏了」+ system prompt 列了 `/diagnose-bsod: 用户报告蓝屏后剧本`
//!   → AI 调 `load_skill(name="/diagnose-bsod")` → 拿到 body 后按剧本走
//!
//! 工具结果作为 tool_result 进入下一轮 context（不污染 system prompt）。
//! 用户手动激活模式（UI 下拉框）走 ui/skills::load_skill_body 直接注入 system prompt。

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct LoadSkill;

impl Tool for LoadSkill {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "**[Progressive Disclosure Tier 2]** 按 name 加载 skill 完整诊断剧本 —— 返回 markdown body。\n\
         \n\
         **When to use**: system prompt 顶部列了所有可用 skill 的 name + description。\
         当用户请求匹配某 skill 的 description（如「我电脑蓝屏了」匹配 `/diagnose-bsod`）时，\
         **先调本工具**拿到完整剧本，然后按 body 里的步骤逐步执行（调对应工具 + 总结）。\n\
         \n\
         **When NOT to use**: 用户请求与所有 skill 都无关时（直接答 / 调常规工具）；\
         已经加载过同一 skill 的 body 时（context 里有了，不重复调）。\n\
         \n\
         **Parameters**:\n\
         - `name` (string, required): skill 全名，含开头 `/`（如 `/diagnose-bsod`）。\
         必须是 system prompt 列出的 name 之一\n\
         \n\
         **Returns**: 该 skill 的完整 markdown body —— 含步骤、工具调用建议、输出格式约束等。\n\
         \n\
         **Example output**: ```\n\
         # 蓝屏诊断流程\n\
         \n\
         ## 步骤 1：收集证据（并行调）\n\
         - list_minidumps —— 看 C:\\Windows\\Minidump 有几个 dump\n\
         - read_event_log_errors(hours=72) —— 最近 3 天 critical / error 事件\n\
         - list_recent_shutdowns —— 看是否有 41 / 6008 异常重启\n\
         ...\n\
         ```\n\
         \n\
         **Notes**: skill 文件位于 `X:\\NeuroBoot\\skills\\*.md`（PE 内置）或 `C:\\NeuroBoot\\skills\\*.md`；\
         未找到 name 返回 NotFound（提示 AI 看 system prompt 里的 skill 列表）；\
         本工具 I/O 微秒级 —— 重复调用代价低但浪费 context，**同一 skill 在单次会话只调一次**。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "skill 全名，必须以 / 开头（如 '/diagnose-bsod'）。\
                                    可选范围由 system prompt 列出"
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 name 参数")
            })?
            .trim();

        if name.is_empty() {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                "name 不能为空",
            ));
        }
        if !name.starts_with('/') {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                format!("skill name 必须以 '/' 开头，收到 '{name}'"),
            ));
        }

        match crate::ui::load_skill_body(name) {
            Some(body) => {
                // 返回完整 body（含 name 头部便于 AI context 内识别来源）
                Ok(format!(
                    "# Skill loaded: {}\n\
                     # Description: {}\n\
                     # Source: {}\n\
                     \n\
                     {}",
                    body.name,
                    body.description,
                    body.source_path.display(),
                    body.body
                ))
            }
            None => Err(ToolError::with_kind(
                ToolErrorKind::NotFound,
                format!(
                    "skill '{name}' 未找到。检查：\
                     (1) name 跟 system prompt 列出的完全一致（含 '/'）；\
                     (2) 文件存在于 X:\\NeuroBoot\\skills\\ 或 C:\\NeuroBoot\\skills\\；\
                     (3) frontmatter 的 name 字段对得上"
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&LoadSkill);
    }

    #[test]
    fn rejects_missing_name() {
        let result = LoadSkill.execute(&json!({}));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
        assert!(err.message.contains("name"));
    }

    #[test]
    fn rejects_name_without_slash() {
        let result = LoadSkill.execute(&json!({ "name": "diagnose-bsod" }));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
        assert!(err.message.contains("/"));
    }

    #[test]
    fn returns_not_found_for_unknown_skill() {
        // 没装任何 skill 时（典型开发机 C:\NeuroBoot\skills 不存在），任意 name 都 NotFound
        let result = LoadSkill.execute(&json!({ "name": "/definitely-does-not-exist-xyz" }));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::NotFound);
    }
}

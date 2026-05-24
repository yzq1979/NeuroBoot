//! Dangerous 工具：delete_path —— 删除指定路径（文件或目录）。
//!
//! 此工具有不可撤销副作用。Agent 调用前 worker 会发送 ConfirmationRequest，
//! 必须用户在 UI 上点「确认执行」才会真删除。
//!
//! 防御层：
//! - safety 标 Dangerous → agent loop 走确认弹窗（用户能看到具体参数再点）
//! - 黑名单 → 拒绝删除 `C:\`、`C:\Windows`、`C:\Program Files` 等关键根
//! - `-LiteralPath` → 防 PowerShell wildcards 误展开
//! - `-Recurse -Force` → 包含 hidden / readonly，但仅在用户确认后

use std::process::Command;

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolOutput};

pub struct DeletePath;

/// 拒绝删除的关键根路径（lowercase 比较，结尾的 `\` `/` 都拒绝）。
const BANNED_ROOTS: &[&str] = &[
    "c:",
    "c:\\",
    "c:/",
    "d:",
    "d:\\",
    "d:/",
    "c:\\windows",
    "c:/windows",
    "c:\\windows\\system32",
    "c:/windows/system32",
    "c:\\program files",
    "c:/program files",
    "c:\\program files (x86)",
    "c:/program files (x86)",
    "c:\\users",
    "c:/users",
];

impl Tool for DeletePath {
    fn name(&self) -> &str {
        "delete_path"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS] 不可撤销** —— 删除指定路径的文件或目录（递归）。\n\
         \n\
         **When to use**: 仅当用户**明确请求删除某个文件/目录**时才调用。\
         例如「帮我删除桌面上的 test.txt」「清空 D:\\old-backups\\ 目录」。\n\
         \n\
         **When NOT to use**（重要！）:\n\
         - 用户说「电脑慢」「想清理空间」时**不要**调用 —— 这是诊断请求不是删除请求\n\
         - 用户说「重置」「恢复出厂」「修复 Windows」时**不要**用本工具，那些有专门修复工具\n\
         - 系统路径（C:\\Windows、C:\\Windows\\System32、C:\\Program Files、C:\\Program Files (x86)、\
         C:\\ProgramData、C:\\Users 根本身）**绝对不要**调 —— 模型层就该拒，会触发黑名单\n\
         - 用户没明确说删除时**不要**自作主张删除「看起来没用」的文件\n\
         \n\
         **Parameters**:\n\
         - `path` (string, required): 绝对路径，文件或目录。Windows 路径分隔符 `\\\\` 或 `/` 都可\n\
         \n\
         **Returns**: 成功返回 `(已删除) <path>`；路径不存在返回 `(不存在) ...`；命中黑名单返回 error。\n\
         \n\
         **执行流程**: UI 必弹确认弹窗 → 用户必须人工点「确认执行」 → 才真删除。\
         用户点取消 → 工具返回 `(用户拒绝)` → **不要重试相同操作**，问用户是否换个方式。\n\
         \n\
         **未来**: v2 Stage 4 会改成 move-to-trash 模式（move to X:\\trash\\<timestamp>\\），\
         给用户翻车后的恢复机会；当前直接 Remove-Item。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要删除的绝对路径，文件或目录"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::new("缺少必须参数 path（字符串）"))?;

        if path.is_empty() {
            return Err(ToolError::new("path 不能为空字符串"));
        }

        // 第二层防御：拒绝关键根路径（即使用户在 UI 上确认了也拦）
        let normalized = path.trim().trim_end_matches(['\\', '/']).to_lowercase();
        if BANNED_ROOTS.contains(&normalized.as_str()) {
            return Err(ToolError::new(format!(
                "拒绝删除关键系统路径 `{path}`（命中黑名单）"
            )));
        }

        // PowerShell 单引号字符串里，引号字面量是连写两个单引号
        let path_escaped = path.replace('\'', "''");

        let ps_script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$p = '{path_escaped}'
if (-not (Test-Path -LiteralPath $p)) {{
    Write-Output "(不存在) 路径 $p 不存在，未执行删除"
    exit 0
}}
Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction Stop
Write-Output "(已删除) $p"
"#
        );

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &ps_script,
            ])
            .output()
            .map_err(|e| ToolError::new(format!("启动 powershell.exe 失败：{e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::new(format!(
                "Remove-Item 失败 (exit {}):\n{}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(stdout)
    }
}

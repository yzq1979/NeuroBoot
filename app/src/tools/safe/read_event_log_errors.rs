//! 工具：read_event_log_errors —— 读取系统日志最近的 Critical/Error 事件。
//!
//! 用 PowerShell `Get-WinEvent` 查 System 日志的 Level=1（Critical）+ Level=2（Error），
//! 默认最近 24 小时、最多 20 条。可通过参数调整时间窗与上限。
//! Message 字段截前 200 字符避免 token 浪费。

use std::process::Command;

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolOutput};

pub struct ReadEventLogErrors;

impl Tool for ReadEventLogErrors {
    fn name(&self) -> &str {
        "read_event_log_errors"
    }

    fn description(&self) -> &str {
        "查询 Windows 系统日志（System log）中最近的严重（Critical）和错误（Error）事件。\
         参数：hours（最近多少小时，默认 24，范围 1~720）；max_events（最多返回几条，\
         默认 20，范围 1~100）。返回 JSON 数组，每条事件含 Time / Level / Source / Id / \
         Message（消息截前 200 字符）。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "hours": {
                    "type": "integer",
                    "description": "查询最近多少小时的事件",
                    "default": 24,
                    "minimum": 1,
                    "maximum": 720
                },
                "max_events": {
                    "type": "integer",
                    "description": "最多返回几条事件",
                    "default": 20,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        // 解析参数；越界裁切到合理范围（防模型给 -1 或 99999）
        let hours = args
            .get("hours")
            .and_then(Value::as_i64)
            .unwrap_or(24)
            .clamp(1, 720);
        let max_events = args
            .get("max_events")
            .and_then(Value::as_i64)
            .unwrap_or(20)
            .clamp(1, 100);

        // PS 脚本用 -FilterHashtable 高效过滤；注意 Rust format! 里 PowerShell 的
        // `{` `}` 都要写成 `{{` `}}`，`$` 不用 escape。
        let ps_script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$events = Get-WinEvent -FilterHashtable @{{LogName='System'; Level=1,2; StartTime=(Get-Date).AddHours(-{hours})}} -MaxEvents {max_events} -ErrorAction SilentlyContinue | Select-Object @{{N='Time';E={{$_.TimeCreated.ToString('yyyy-MM-dd HH:mm:ss')}}}}, @{{N='Level';E={{if ($_.Level -eq 1) {{'Critical'}} elseif ($_.Level -eq 2) {{'Error'}} else {{"Lvl$($_.Level)"}}}}}}, @{{N='Source';E={{$_.ProviderName}}}}, Id, @{{N='Message';E={{if ($_.Message -and $_.Message.Length -gt 200) {{$_.Message.Substring(0,200) + '...'}} else {{$_.Message}}}}}}
ConvertTo-Json @($events) -Depth 3 -Compress"#
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
                "Get-WinEvent 失败 (exit {}):\n{}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            // 时间窗内无 Critical/Error events —— 合法的空数组
            return Ok("[]".to_owned());
        }
        Ok(stdout)
    }
}

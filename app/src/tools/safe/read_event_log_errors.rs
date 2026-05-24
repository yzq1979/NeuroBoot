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
        "查 Windows 系统日志（System log）最近的 Critical 和 Error 事件。\n\
         \n\
         **When to use**: 用户说「电脑出问题了」「最近不稳定」「时不时蓝屏 / 自动重启」「装了某软件后开始报错」时 —— \
         系统日志是 Windows 故障诊断的**第一信息源**；几乎所有诊断流程都应该先调这个。\n\
         \n\
         **Parameters**:\n\
         - `hours` (integer, 1~720, 默认 24): 查最近多少小时。一般用 24（一天）；用户说「最近一周」用 168；首次问诊用 48 收集更全证据\n\
         - `max_events` (integer, 1~100, 默认 20): 返回上限。事件多时用 20 看最新；调高到 50 看完整\n\
         \n\
         **Returns**: JSON 数组（按时间从新到旧），每条含：\n\
         - `Time`: yyyy-MM-dd HH:mm:ss\n\
         - `Level`: Critical / Error / Lvl<N>\n\
         - `Source`: 事件来源（如 disk / Application Error / Kernel-Power）—— **关键诊断信息**\n\
         - `Id`: 事件 ID（如 41 = Kernel-Power 异常重启；7026 = 关键启动驱动加载失败）\n\
         - `Message`: 截前 200 字符（避免 token 浪费；要完整看 Event Viewer）\n\
         \n\
         **Example output**: `[{\"Time\":\"2026-05-24 14:30:01\",\"Level\":\"Error\",\
         \"Source\":\"disk\",\"Id\":11,\"Message\":\"驱动器 \\\\Device\\\\Harddisk1\\\\DR1 上有控制器错误...\"}]`\n\
         \n\
         **Notes**: 只看 System log；应用层错误用未来的 read_event_log_application；\
         返回空数组 `[]` 是好消息（窗口内无严重错误）。"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ReadEventLogErrors);
    }
}

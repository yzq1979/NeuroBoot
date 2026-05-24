//! 工具：list_recent_shutdowns —— 列最近的关机/重启/异常断电事件。
//!
//! System log 里的关键 event id：
//! - 6005 = Event Log service started（系统启动）
//! - 6006 = Event Log service stopped（干净关机/重启）
//! - 6008 = previous shutdown unexpected（异常关机：断电 / 长按电源 / 蓝屏自动重启）
//! - 41   = Kernel-Power（异常重启 / 突然断电）
//! - 1074 = USER32 reports shutdown initiated by ...（谁触发的关机/重启）
//!
//! 结合看能区分：用户主动关机 / 蓝屏自动重启 / 突然断电 / Windows Update 重启 / 等。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListRecentShutdowns;

impl Tool for ListRecentShutdowns {
    fn name(&self) -> &str {
        "list_recent_shutdowns"
    }

    fn description(&self) -> &str {
        "列最近的关机 / 重启 / 异常断电事件（Event IDs 6005 / 6006 / 6008 / 41 / 1074）。\
         参数 max_events: 默认 20，最大 100。\
         返回 JSON 数组，每条含 Time / Id / Source / Description / Message 字段。\
         Id 含义：6005=系统启动，6006=干净关机，6008=异常关机（断电/长按电源/蓝屏自动重启），\
         41=Kernel-Power 异常重启，1074=用户/进程触发的关机。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_events": {
                    "type": "integer",
                    "description": "最多返回几条",
                    "default": 20,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let max_events = args
            .get("max_events")
            .and_then(Value::as_i64)
            .unwrap_or(20)
            .clamp(1, 100);

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$events = Get-WinEvent -FilterHashtable @{{LogName='System'; Id=6005,6006,6008,41,1074}} -MaxEvents {max_events} -ErrorAction SilentlyContinue | Select-Object @{{N='Time';E={{$_.TimeCreated.ToString('yyyy-MM-dd HH:mm:ss')}}}}, @{{N='Id';E={{$_.Id}}}}, @{{N='Source';E={{$_.ProviderName}}}}, @{{N='Description';E={{switch ($_.Id) {{ 6005 {{'系统启动'}} 6006 {{'干净关机'}} 6008 {{'异常关机/断电'}} 41 {{'Kernel-Power 异常重启'}} 1074 {{'用户/进程触发关机'}} default {{"Event $($_.Id)"}} }}}}}}, @{{N='Message';E={{if ($_.Message -and $_.Message.Length -gt 200) {{$_.Message.Substring(0,200) + '...'}} else {{$_.Message}}}}}}
ConvertTo-Json @($events) -Depth 3 -Compress"#
        );

        run_ps_json_array(&script)
    }
}

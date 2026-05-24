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
        "列最近的关机 / 重启 / 异常断电事件 —— 区分正常关机 vs 蓝屏 vs 断电。\n\
         \n\
         **When to use**: 用户说「电脑昨晚自己重启了」「不知道什么时候关的机」「怀疑系统在自动重启」时；\
         配合 list_minidumps + read_event_log_errors 三件套确诊 BSOD；\
         判断电源稳定性（异常断电频繁 = 电源 / 电池 / 插座问题）。\n\
         \n\
         **Parameters**:\n\
         - `max_events` (integer, 1~100, 默认 20): 返回上限。看「最近几次」用 10，看「最近一周关机模式」用 50\n\
         \n\
         **Returns**: JSON 数组（按时间从新到旧），每条含：\n\
         - `Time` / `Id` / `Source` / `Description` / `Message`\n\
         \n\
         **Event ID 含义速查**（Description 字段已翻译，但 Id 也给出便于交叉引用）：\n\
         - `6005`: Event Log 服务启动 = **系统启动**\n\
         - `6006`: Event Log 服务停止 = **干净关机 / 重启**（用户菜单关的 / `shutdown /r`）\n\
         - `6008`: 上次关机异常 = **断电 / 长按电源键 / 蓝屏自动重启**（**警告级**）\n\
         - `41`:   Kernel-Power = **异常重启 / 突然断电**（**最严重**，可能硬件问题）\n\
         - `1074`: USER32 关机原因 = **某用户或进程触发的关机**（含 Windows Update 重启）\n\
         \n\
         **Example output**: `[{\"Time\":\"2026-05-24 08:00:01\",\"Id\":6005,\"Source\":\"EventLog\",\
         \"Description\":\"系统启动\",\"Message\":\"事件日志服务已启动。\"},\
         {\"Time\":\"2026-05-23 23:45:12\",\"Id\":41,\"Source\":\"Microsoft-Windows-Kernel-Power\",\
         \"Description\":\"Kernel-Power 异常重启\",\"Message\":\"系统已重新启动，但未先正常关闭...\"}]`\n\
         \n\
         **Notes**: 关机时序通常是 `6006 → 6005`（干净）或直接 `6008 → 6005`（异常）。\
         **6008 + 41 在同一时间点出现 = 几乎肯定是蓝屏自动重启**，下一步去 list_minidumps 找 dump。"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ListRecentShutdowns);
    }
}

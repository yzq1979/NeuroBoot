//! 工具：list_services —— 列 Windows 服务。可按状态过滤。
//!
//! Get-Service 返回所有服务的当前状态。默认只列 Running 的（最常用）；
//! 想看挂掉的服务传 status="Stopped"；查全部传 status="all"。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListServices;

impl Tool for ListServices {
    fn name(&self) -> &str {
        "list_services"
    }

    fn description(&self) -> &str {
        "列 Windows 服务（按状态过滤）—— 看哪些服务在跑、哪些挂了。\n\
         \n\
         **When to use**: 用户说「某功能不能用」（如打印、声音、网络共享、Defender）—— \
         多数 Windows 功能依赖对应服务（Print Spooler / Audio / Server / WinDefend），\
         先查服务是否在跑；\
         判断是被恶意软件 / 优化软件禁用了某个服务（StartType=Disabled）。\n\
         \n\
         **Parameters**:\n\
         - `status` (string, 'Running' 默认 / 'Stopped' / 'all'): 按状态过滤\n\
         \n\
         **Returns**: JSON 数组，每服务含：\n\
         - `Name`: 服务系统名（如 Spooler / wuauserv）—— 跟用户讲时换 DisplayName 更易懂\n\
         - `DisplayName`: 用户友好名（如「Print Spooler」「Windows Update」）\n\
         - `Status`: Running / Stopped / StartPending / StopPending / Paused\n\
         - `StartType`: Automatic / Manual / Disabled / Boot / System（**Disabled = 被人故意禁的，可疑**）\n\
         \n\
         **Example output**: `[{\"Name\":\"Spooler\",\"DisplayName\":\"Print Spooler\",\
         \"Status\":\"Running\",\"StartType\":\"Automatic\"},{\"Name\":\"WinDefend\",\
         \"DisplayName\":\"Microsoft Defender Antivirus Service\",\"Status\":\"Running\",\
         \"StartType\":\"Automatic\"}]`\n\
         \n\
         **Notes**: 默认只列 Running 节省 token；查「为啥某功能没用」用 status='Stopped' 看挂的；\
         全面审计用 status='all'。**重要**：PE 里大量主系统服务不会启动，**不要拿 PE 的服务状态推断主系统状态**。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["Running", "Stopped", "all"],
                    "description": "按状态过滤；'all' 不过滤",
                    "default": "Running"
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let status = args
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("Running");
        let filter = match status {
            "all" => String::new(),
            other => format!("| Where-Object {{$_.Status -eq '{other}'}}"),
        };

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
ConvertTo-Json @(Get-Service -ErrorAction SilentlyContinue {filter} | Select-Object Name, DisplayName, @{{N='Status';E={{$_.Status.ToString()}}}}, @{{N='StartType';E={{$_.StartType.ToString()}}}}) -Depth 3 -Compress"#
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
        assert_v30_description_convention(&ListServices);
    }
}

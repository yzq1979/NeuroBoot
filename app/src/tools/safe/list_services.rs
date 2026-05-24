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
        "列 Windows 服务。参数 status：'Running'（默认）/ 'Stopped' / 'all'。\
         返回 JSON 数组，每条含 Name/DisplayName/Status/StartType 字段。\
         适合诊断「某服务该跑的没跑」「禁用了导致功能不可用」类。"
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

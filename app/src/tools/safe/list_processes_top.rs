//! 工具：list_processes_top —— 按 CPU 或内存排序，返回前 N 个进程。
//!
//! 用于诊断「卡顿」「内存爆」「CPU 100%」 —— 排前面的进程通常是元凶。
//! Get-Process 的 CPU 字段是累计 CPU 秒数（启动以来），非瞬时 CPU%；
//! 想要瞬时 CPU% 要 Get-Counter，本工具用累计值即可（够定位重负载进程）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListProcessesTop;

impl Tool for ListProcessesTop {
    fn name(&self) -> &str {
        "list_processes_top"
    }

    fn description(&self) -> &str {
        "按 CPU 累计秒数（默认）或内存（WorkingSet）排序，返回前 N 个进程。\
         参数：sort_by ('cpu' 或 'memory'，默认 'cpu')；top_n (1~50，默认 20)。\
         返回 JSON 数组，每条含 PID/Name/CPU/MemoryMB/StartTime/Path 字段。\
         CPU 是启动以来累计的 CPU 秒数（不是瞬时 CPU%）；MemoryMB 是 WorkingSet 物理内存占用。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sort_by": {
                    "type": "string",
                    "enum": ["cpu", "memory"],
                    "description": "按 cpu 累计秒数还是 memory(WorkingSet) 排序",
                    "default": "cpu"
                },
                "top_n": {
                    "type": "integer",
                    "description": "返回前 N 个",
                    "default": 20,
                    "minimum": 1,
                    "maximum": 50
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let sort_by = args
            .get("sort_by")
            .and_then(Value::as_str)
            .unwrap_or("cpu");
        let sort_field = if sort_by == "memory" { "WS" } else { "CPU" };
        let top_n = args
            .get("top_n")
            .and_then(Value::as_i64)
            .unwrap_or(20)
            .clamp(1, 50);

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
ConvertTo-Json @(Get-Process -ErrorAction SilentlyContinue | Sort-Object {sort_field} -Descending | Select-Object -First {top_n} @{{N='PID';E={{$_.Id}}}}, @{{N='Name';E={{$_.ProcessName}}}}, @{{N='CPU';E={{if ($_.CPU) {{[math]::Round($_.CPU,1)}} else {{0}}}}}}, @{{N='MemoryMB';E={{[math]::Round($_.WorkingSet/1MB,1)}}}}, @{{N='StartTime';E={{try {{$_.StartTime.ToString('HH:mm:ss')}} catch {{'?'}}}}}}, @{{N='Path';E={{try {{$_.Path}} catch {{$null}}}}}}) -Depth 3 -Compress"#
        );

        run_ps_json_array(&script)
    }
}

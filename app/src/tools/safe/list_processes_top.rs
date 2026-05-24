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
        "按 CPU 累计秒数或内存排序，返回 top N 进程 —— 找性能元凶。\n\
         \n\
         **When to use**: 用户说「电脑卡」「CPU 100%」「内存爆了」「风扇一直转（可能挖矿病毒）」时；\
         排查异常进程（按 sort_by='memory' 看谁吃内存，sort_by='cpu' 看谁吃 CPU）；\
         判断是真的负载高还是某个失控进程。\n\
         \n\
         **Parameters**:\n\
         - `sort_by` (string, 'cpu' 或 'memory', 默认 'cpu'): 按累计 CPU 秒数还是 WorkingSet 物理内存排序\n\
         - `top_n` (integer, 1~50, 默认 20): 返回前 N 个\n\
         \n\
         **Returns**: JSON 数组（按指定字段降序），每进程含：\n\
         - `PID`: 进程 ID\n\
         - `Name`: 进程名（不含 .exe）\n\
         - `CPU`: 启动以来累计 CPU 秒数（**注意**：不是瞬时 CPU%；瞬时 % 要用 Get-Counter）\n\
         - `MemoryMB`: WorkingSet 物理内存占用（这是真的 RAM 占用）\n\
         - `StartTime`: HH:mm:ss 或 '?'（系统进程读不到）\n\
         - `Path`: exe 完整路径（'?'  = 权限不够；**关键安全信号**：路径在 %TEMP% / %APPDATA% / 用户目录的奇怪进程可能是恶意软件）\n\
         \n\
         **Example output**: `[{\"PID\":1234,\"Name\":\"chrome\",\"CPU\":3621.5,\"MemoryMB\":1245.8,\
         \"StartTime\":\"09:15:32\",\"Path\":\"C:\\\\Program Files\\\\Google\\\\Chrome\\\\Application\\\\chrome.exe\"},\
         {\"PID\":4,\"Name\":\"System\",\"CPU\":892.1,\"MemoryMB\":24.3,\"StartTime\":\"?\",\"Path\":null}]`\n\
         \n\
         **Notes**: CPU 字段是累计值不是瞬时 —— 长时间运行的进程（如 svchost）天生 CPU 累计大，**不一定有问题**。\
         真要看瞬时 CPU% 等未来加专门工具。"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ListProcessesTop);
    }
}

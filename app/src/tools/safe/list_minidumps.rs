//! 工具：list_minidumps —— 列出系统蓝屏 dump 文件。
//!
//! Win10/11 默认开启自动 minidump，蓝屏后写 `C:\Windows\Minidump\<datetime>.dmp`；
//! 全部内存 dump 在 `C:\Windows\MEMORY.DMP`。本工具列两个位置的 dump 文件清单。
//!
//! v2 后续：analyze_minidump 工具（调 BlueScreenView 解析每个 dump 的 driver / bug code）。
//! 当前版本只列文件让模型知道有哪些 dump 可分析。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListMinidumps;

impl Tool for ListMinidumps {
    fn name(&self) -> &str {
        "list_minidumps"
    }

    fn description(&self) -> &str {
        "列系统蓝屏 dump 文件清单 —— 看有几个崩溃记录、什么时候崩的。\n\
         \n\
         **When to use**: 用户说「电脑蓝屏」「最近频繁蓝屏」「想知道是不是真的蓝屏过」时；\
         诊断 BSOD 的第一步 —— 先看有没有 dump、什么时候崩的、密集程度（10 分钟连崩 = 严重硬件 / 驱动问题）；\
         结合 read_event_log_errors 一起看（dump 文件 + Event ID 1001 \"BugCheck\" 对应）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON 数组（按时间从新到旧），每文件含：\n\
         - `Path`: 完整路径（`C:\\Windows\\Minidump\\<datetime>.dmp` 或 `C:\\Windows\\MEMORY.DMP`）\n\
         - `SizeMB`: 文件大小（minidump 通常 0.1~1 MB；MEMORY.DMP 是全量内存 dump 通常几个 GB）\n\
         - `LastWriteTime`: 崩溃时间 yyyy-MM-dd HH:mm:ss\n\
         \n\
         **Example output**: `[{\"Path\":\"C:\\\\Windows\\\\Minidump\\\\052426-12345-01.dmp\",\
         \"SizeMB\":0.3,\"LastWriteTime\":\"2026-05-24 14:32:11\"},\
         {\"Path\":\"C:\\\\Windows\\\\MEMORY.DMP\",\"SizeMB\":1024.0,\
         \"LastWriteTime\":\"2026-05-23 09:15:48\"}]`\n\
         \n\
         **Notes**: 空数组的可能原因：① 没崩过 ② 已被清理（CCleaner 之类）③ 系统设了不写 minidump（控制面板「启动和故障恢复」）。\
         本工具只**列出 dump 文件**，**不分析内容**；要解码某个 dump 的 driver / bug check code 需 analyze_minidump（W7 配套）。\n\
         **重要**：dump 文件是关键诊断证据，**绝不要建议用户删除**，除非确认不再需要排查。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: &Value) -> ToolOutput {
        let script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$dumps = @()
$dumps += Get-ChildItem -Path 'C:\Windows\Minidump\*.dmp' -ErrorAction SilentlyContinue
if (Test-Path 'C:\Windows\MEMORY.DMP') { $dumps += Get-Item 'C:\Windows\MEMORY.DMP' }
ConvertTo-Json @($dumps | Sort-Object LastWriteTime -Descending | Select-Object @{N='Path';E={$_.FullName}}, @{N='SizeMB';E={[math]::Round($_.Length/1MB,1)}}, @{N='LastWriteTime';E={$_.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss')}}) -Depth 3 -Compress"#;

        run_ps_json_array(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ListMinidumps);
    }
}

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
        "列出系统蓝屏 dump 文件：C:\\Windows\\Minidump\\*.dmp 与 C:\\Windows\\MEMORY.DMP。\
         返回 JSON 数组，每条含 Path/SizeMB/LastWriteTime 字段，按时间从新到旧排序。\
         空数组 = 系统没有崩溃 dump（要么没崩过，要么 dump 被清了 / 没启用自动写入）。\
         适合诊断「最近频繁蓝屏」—— 先看 dump 数量 + 时间分布，再决定下一步要不要分析具体某个 dump。"
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

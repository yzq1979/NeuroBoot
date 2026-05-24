//! 工具：list_partitions —— 列出所有硬盘的分区详情。
//!
//! `Get-Partition` cmdlet 返回字段：DiskNumber, PartitionNumber, DriveLetter,
//! Size, Type (Basic/GPT/MBR), IsBoot, IsSystem, IsActive, GptType。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListPartitions;

impl Tool for ListPartitions {
    fn name(&self) -> &str {
        "list_partitions"
    }

    fn description(&self) -> &str {
        "列出本机所有硬盘的分区详情。可选参数 disk_number 仅查指定磁盘。\
         返回 JSON 数组，每条含 DiskNumber/PartitionNumber/DriveLetter/SizeGB/Type/\
         IsBoot/IsSystem/IsActive 字段。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "disk_number": {
                    "type": "integer",
                    "description": "可选：只查这块物理硬盘的分区（盘号，从 0 开始；用 list_disks 查盘号）",
                    "minimum": 0
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let disk_filter = args
            .get("disk_number")
            .and_then(Value::as_i64)
            .map(|d| format!("-DiskNumber {d} "))
            .unwrap_or_default();

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
ConvertTo-Json @(Get-Partition {disk_filter}-ErrorAction SilentlyContinue | Select-Object DiskNumber, PartitionNumber, @{{N='DriveLetter';E={{if ($_.DriveLetter) {{$_.DriveLetter.ToString()}} else {{$null}}}}}}, @{{N='SizeGB';E={{[math]::Round($_.Size/1GB,2)}}}}, @{{N='Type';E={{$_.Type.ToString()}}}}, IsBoot, IsSystem, IsActive) -Depth 3 -Compress"#
        );

        run_ps_json_array(&script)
    }
}

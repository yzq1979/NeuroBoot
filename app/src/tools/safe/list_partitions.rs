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
        "列出硬盘的分区表（GPT/MBR）—— 比 list_volumes 更底层。\n\
         \n\
         **When to use**: 用户问「分区怎么排的」「EFI 分区在哪」「为什么我看不到 D 盘了」时；\
         诊断启动问题（System / Boot 标记是否正确）；\
         判断该机是 GPT 还是 MBR（Type 字段）；\
         分区表损坏 / 误删分区 排查的第一手数据。\n\
         \n\
         **Parameters**:\n\
         - `disk_number` (integer, 可选): 只查这块物理硬盘的分区。盘号从 list_disks 取。不传 = 查所有盘\n\
         \n\
         **Returns**: JSON 数组，每条分区含：\n\
         - `DiskNumber` / `PartitionNumber`\n\
         - `DriveLetter`: 盘符（可能 null —— EFI / RECOVERY 等隐藏分区无盘符）\n\
         - `SizeGB`: 容量\n\
         - `Type`: Basic / GPT / MBR / IFS / Unknown 等\n\
         - `IsBoot` / `IsSystem` / `IsActive`: 启动标志位（System 分区是含 BCD 的；Boot 是含 Windows 文件夹的；这俩可能不在一个分区）\n\
         \n\
         **Notes**: 跟 list_volumes 互补 —— 本工具看分区表层（GPT entries），list_volumes 看文件系统层（卷使用率）；\
         数据恢复 / 启动修复要先看本工具确定分区表是否完整。"
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

//! 工具：list_volumes —— 列出所有卷（含未挂载盘符的分区，如 EFI/RECOVERY）。
//!
//! `Get-Volume` 与 `Get-Partition` 互补：Get-Volume 关注文件系统层（FreeSpace 等使用率），
//! Get-Partition 关注分区表层（Boot/System/GPT type）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListVolumes;

impl Tool for ListVolumes {
    fn name(&self) -> &str {
        "list_volumes"
    }

    fn description(&self) -> &str {
        "列所有卷（文件系统层）—— 看容量使用率和文件系统类型。\n\
         \n\
         **When to use**: 用户问「C 盘还有多少空间」「D 盘满了」「电脑变慢是不是 SSD 满了」时；\
         判断哪个分区可以扩容 / 哪个该清理；\
         检查文件系统类型（NTFS / FAT32 / exFAT / RAW —— RAW 意味着文件系统损坏）；\
         判断盘是固定盘还是 USB（DriveType）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON 数组，每卷含：\n\
         - `DriveLetter`: 盘符（隐藏卷会是 null —— EFI / RECOVERY）\n\
         - `FileSystemLabel`: 卷标（用户自定义）\n\
         - `FileSystem`: NTFS / FAT32 / exFAT / RAW / null\n\
         - `DriveType`: Fixed（固定盘）/ Removable（U 盘）/ Network / CD-ROM / Unknown\n\
         - `SizeGB` / `FreeGB`: 总量和剩余\n\
         - `UsedPct`: 已用 %（**> 90% 时该提示用户清理**）\n\
         - `HealthStatus`: Healthy / Warning / Unhealthy / Unknown\n\
         \n\
         **Example output**: `[{\"DriveLetter\":\"C\",\"FileSystemLabel\":\"OS\",\"FileSystem\":\"NTFS\",\
         \"DriveType\":\"Fixed\",\"SizeGB\":238.4,\"FreeGB\":42.1,\"UsedPct\":82.3,\"HealthStatus\":\"Healthy\"},\
         {\"DriveLetter\":null,\"FileSystemLabel\":\"\",\"FileSystem\":\"FAT32\",\"DriveType\":\"Fixed\",\
         \"SizeGB\":0.1,\"FreeGB\":0.08,\"UsedPct\":20.0,\"HealthStatus\":\"Healthy\"}]`\n\
         \n\
         **Notes**: 跟 list_partitions 互补 —— 看「能不能写文件」用本工具，看「分区表对不对」用 list_partitions。\
         FileSystem 是 `RAW` 时**严重警告**（用户数据可能仍在但文件系统结构损坏，需要 TestDisk 救援）。"
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
ConvertTo-Json @(Get-Volume -ErrorAction SilentlyContinue | Select-Object @{N='DriveLetter';E={if ($_.DriveLetter) {$_.DriveLetter.ToString()} else {$null}}}, FileSystemLabel, FileSystem, @{N='DriveType';E={$_.DriveType.ToString()}}, @{N='SizeGB';E={[math]::Round($_.Size/1GB,2)}}, @{N='FreeGB';E={[math]::Round($_.SizeRemaining/1GB,2)}}, @{N='UsedPct';E={if ($_.Size -gt 0) {[math]::Round(100*($_.Size - $_.SizeRemaining)/$_.Size,1)} else {0}}}, @{N='HealthStatus';E={$_.HealthStatus.ToString()}}) -Depth 3 -Compress"#;

        run_ps_json_array(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ListVolumes);
    }
}

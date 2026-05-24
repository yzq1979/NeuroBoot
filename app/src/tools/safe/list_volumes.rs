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
        "列出本机所有卷（含 EFI / RECOVERY 等未分配盘符的）。返回 JSON 数组，每条含 \
         DriveLetter/FileSystemLabel/FileSystem/DriveType (Fixed/Removable/Network)/\
         SizeGB/FreeGB/UsedPct/HealthStatus 字段。"
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

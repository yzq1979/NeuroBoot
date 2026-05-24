//! [Safe] read_smart —— 通过 smartctl 读硬盘 SMART 详细数据。
//!
//! Windows `Get-PhysicalDisk` 的 ReliabilityCounter 只是表层；smartctl 给所有 SMART
//! 属性 raw value + 阈值，是真正诊断硬盘老化的工具。
//!
//! smartmontools (smartctl.exe) ~5 MB portable，GPL 协议可商用；默认不在 NeuroBoot ISO，
//! 按 docs/BUILD.md 下载放 `X:\NeuroBoot\tools\smartmontools\smartctl.exe`。

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct ReadSmart;

fn find_smartctl() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"X:\NeuroBoot\tools\smartmontools\smartctl.exe"),
        PathBuf::from(r"C:\NeuroBoot\tools\smartmontools\smartctl.exe"),
        PathBuf::from(r"C:\Program Files\smartmontools\bin\smartctl.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

impl Tool for ReadSmart {
    fn name(&self) -> &str {
        "read_smart"
    }

    fn description(&self) -> &str {
        "读硬盘 **SMART** 详细数据 —— `smartctl -a /dev/sdX`（Windows 用 \\\\.\\PhysicalDriveN）。\n\
         \n\
         **When to use**: 用户怀疑硬盘老化、坏道、温度高、寿命到了；list_disks 报 Health=Warning；\
         详细诊断的「最后一公里」（前面的 list_disks 是浅 health flag，smartctl 是 raw SMART 属性）。\n\
         \n\
         **Parameters**:\n\
         - `physical_drive` (integer, required): PhysicalDriveN 的 N（用 list_disks 的 Number 字段填）\n\
         \n\
         **Returns**: smartctl 完整文本 —— 包含 Power_On_Hours、Reallocated_Sector_Ct、\
         Wear_Leveling_Count（SSD 寿命）、Temperature_Celsius、Self-test 历史等。\n\
         \n\
         **Notes**: smartctl.exe 默认不在 NeuroBoot ISO 里；按 docs/BUILD.md 下载放 \
         `X:\\NeuroBoot\\tools\\smartmontools\\smartctl.exe`；smartmontools 是 GPL 可商用；\
         **关键看的字段**：Reallocated_Sector_Ct > 0 = 已有坏块；\
         Wear_Leveling_Count 数值大 = SSD 老化；Temperature > 60°C 持续 = 散热问题。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "physical_drive": {
                    "type": "integer",
                    "description": "PhysicalDriveN 的 N（0/1/...，跟 list_disks 的 Number 一致）",
                    "minimum": 0
                }
            },
            "required": ["physical_drive"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let drive_num = args
            .get("physical_drive")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 physical_drive 参数")
            })?;
        if !(0..=64).contains(&drive_num) {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                format!("physical_drive 越界 (0~64)，收到 {drive_num}"),
            ));
        }

        let exe = find_smartctl().ok_or_else(|| {
            ToolError::with_kind(
                ToolErrorKind::NotFound,
                "smartctl.exe 未找到。NeuroBoot 默认 ISO 不带 smartmontools；\
                 请按 docs/BUILD.md 「救援工具下载」节下载（~5 MB GPL 可商用）放到 \
                 X:\\NeuroBoot\\tools\\smartmontools\\ 后再试。",
            )
        })?;

        let device = format!(r"\\.\PhysicalDrive{drive_num}");
        let output = Command::new(&exe)
            .args(["-a", &device])
            .output()
            .map_err(|e| ToolError::new(format!("启动 smartctl 失败：{e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        // smartctl 返回非零有时仅是 bitmask warning，不是真错；只看 stderr 严重程度
        if !stderr.is_empty() && stdout.is_empty() {
            return Err(ToolError::with_kind(
                ToolErrorKind::ExternalCommandFailed,
                format!("smartctl 失败：{}", stderr.trim()),
            ));
        }
        Ok(stdout)
    }
}

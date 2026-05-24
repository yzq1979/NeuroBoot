//! 工具：list_disks —— 列出本机物理硬盘。
//!
//! 通过 PowerShell `Get-Disk` cmdlet 查询；返回 JSON 化的硬盘信息。
//! 字段：Number / FriendlyName / SizeGB / HealthStatus / OperationalStatus / BusType
//!
//! 实现要点：
//! - `[Console]::OutputEncoding = UTF-8`：避免中文 Windows 默认 GBK 让 Rust 解码乱码
//! - `@(...)` 强制成数组：PS 5.1 的 ConvertTo-Json 对单元素不会自动包数组
//! - `-ErrorAction SilentlyContinue`：单块磁盘读取失败时不中断整体输出
//! - `-Compress`：紧凑 JSON 节省 token

use std::process::Command;

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolOutput};

/// list_disks 工具的零大小标志结构体（无内部状态）。
pub struct ListDisks;

impl Tool for ListDisks {
    fn name(&self) -> &str {
        "list_disks"
    }

    fn description(&self) -> &str {
        "列出本机所有物理硬盘的基本信息。\n\
         \n\
         **When to use**: 用户问「我有几块硬盘」「硬盘列表」「硬盘型号是什么」时；\
         诊断硬盘相关问题（坏盘、容量不足、找不到分区）的第一步收集物理硬盘清单；\
         需要后续用 list_partitions / list_volumes 时先拿盘号。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON 数组，每元素含：\n\
         - `Number`: 盘号 (0/1/2/...) —— 用于 list_partitions 的 disk_number 参数\n\
         - `FriendlyName`: 硬盘型号字符串（如 Samsung SSD 870 EVO 1TB）\n\
         - `SizeGB`: 容量 GB，1 位小数\n\
         - `HealthStatus`: Healthy / Warning / Unhealthy / Unknown\n\
         - `OperationalStatus`: Online / Offline 等\n\
         - `BusType`: SATA / NVMe / USB / SAS 等\n\
         \n\
         **Notes**: 只看物理硬盘层，**不**显示分区或卷信息；那些要用 list_partitions / list_volumes。\
         返回空数组 `[]` 表示读不到任何盘（极少见，可能 WMI 服务挂了）。"
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
        // PowerShell 脚本：见模块文档说明
        let ps_script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
ConvertTo-Json @(Get-Disk -ErrorAction SilentlyContinue | Select-Object Number, FriendlyName, @{N='SizeGB';E={[math]::Round($_.Size/1GB,1)}}, @{N='HealthStatus';E={$_.HealthStatus.ToString()}}, @{N='OperationalStatus';E={$_.OperationalStatus.ToString()}}, @{N='BusType';E={$_.BusType.ToString()}}) -Depth 3 -Compress"#;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                ps_script,
            ])
            .output()
            .map_err(|e| ToolError::new(format!("启动 powershell.exe 失败：{e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::new(format!(
                "Get-Disk 执行失败 (exit {}):\n{}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            // Get-Disk 无任何输出（可能权限不足或机器特殊）—— 返回合法空数组让模型解读
            return Ok("[]".to_owned());
        }
        Ok(stdout)
    }
}

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
//!
//! ---
//!
//! **v3.0 description 重写约定**（W1 试点）—— 22 工具会统一遵循：
//! 1. **首句**：动词开头 + 列出具体字段名（替代「基本信息」这种模糊词）。
//!    搜索引擎用 name + description 匹配，字段名出现在描述里能让 AI 更准定位
//! 2. **`**When to use**`**：列触发关键词 + 典型用户问句 + 串联场景
//! 3. **`**Parameters**`**：每个参数标 type，无参数明示「无」+ 是否需 admin
//! 4. **`**Returns**`**：JSON 形状用 `字段名 (type): 含义` 一行一字段，明示
//!    哪些字段是下游工具的输入（建立 tool-chain 显式联系）
//! 5. **`**Example output**`**：1 行真实输出片段，AI 见过形状不易编造字段
//! 6. **`**Notes**`**：边界、错误模式、PE vs Windows 差异、license/外部依赖

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
        "列出本机所有物理硬盘 —— 盘号 / 型号 / 容量 / 健康状态 / 总线类型。\n\
         \n\
         **When to use**: 用户问「我有几块硬盘 / SSD / 磁盘」「硬盘型号」「容量多少」时；\
         诊断硬盘问题（坏盘 / 容量不足 / 找不到分区 / SMART 异常）的第一步；\
         调用 list_partitions / list_volumes / read_smart 前先拿盘号。\n\
         \n\
         **Parameters**: 无（无需管理员权限）。\n\
         \n\
         **Returns**: JSON 数组，每元素 6 个固定字段：\n\
         - `Number` (int): 盘号 0/1/2/... —— 用作 list_partitions(disk_number) 和 \
         read_smart(physical_drive) 的输入\n\
         - `FriendlyName` (str): 硬盘型号，例如 \"Samsung SSD 870 EVO 1TB\"\n\
         - `SizeGB` (float): 容量 GB，1 位小数\n\
         - `HealthStatus` (str): Healthy / Warning / Unhealthy / Unknown\n\
         - `OperationalStatus` (str): Online / Offline\n\
         - `BusType` (str): SATA / NVMe / USB / SAS / RAID\n\
         \n\
         **Example output**: `[{\"Number\":0,\"FriendlyName\":\"Samsung SSD 870 EVO 1TB\",\
         \"SizeGB\":931.5,\"HealthStatus\":\"Healthy\",\"OperationalStatus\":\"Online\",\
         \"BusType\":\"SATA\"}]`\n\
         \n\
         **Notes**: 只看物理硬盘层 —— **不**含分区 / 卷 / 文件系统（那些用 list_partitions / \
         list_volumes）。PE 环境下能看到目标机所有硬盘（含系统盘 C:），盘号与主系统启动后一致。\
         返回空数组 `[]` 表示 WMI 服务不可用（极少见，可能要重启 Winmgmt 服务）。"
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

#[cfg(test)]
mod tests {
    //! W1 试点：为 v3.0 description 重写约定建立单测样本。
    //! 这些测试不依赖 PowerShell 执行，只验证 Tool trait 元信息符合约定 —— 防回归。
    //! 批量推完 22 工具后，可考虑把通用约定测试上移到 registry.rs。

    use super::*;

    #[test]
    fn name_is_snake_case_verb_object() {
        let t = ListDisks;
        let n = t.name();
        // 全小写 + 下划线
        assert!(
            n.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "name '{n}' must be snake_case (lowercase + underscore only)"
        );
        // 至少有一个下划线（verb_object 形式）
        assert!(n.contains('_'), "name '{n}' should be verb_object form");
        assert_eq!(n, "list_disks");
    }

    #[test]
    fn description_has_required_sections() {
        let desc = ListDisks.description();
        for marker in [
            "**When to use**",
            "**Parameters**",
            "**Returns**",
            "**Example output**",
            "**Notes**",
        ] {
            assert!(
                desc.contains(marker),
                "description missing section marker `{marker}`"
            );
        }
    }

    #[test]
    fn description_length_within_band() {
        // 200~1500 字符是合理区间：太短 = 信息不足，太长 = 浪费 context。
        // list_disks 这种基础工具偏简单，目标 ~800 字符
        let len = ListDisks.description().chars().count();
        assert!(
            (200..=1500).contains(&len),
            "description length {len} out of range [200, 1500]"
        );
    }

    #[test]
    fn description_lists_concrete_field_names() {
        // Anthropic 2026 best practice：字段名出现在 description 让 AI 不易编字段
        let desc = ListDisks.description();
        for field in [
            "Number",
            "FriendlyName",
            "SizeGB",
            "HealthStatus",
            "OperationalStatus",
            "BusType",
        ] {
            assert!(
                desc.contains(field),
                "description must mention return field `{field}` so AI knows the schema"
            );
        }
    }

    #[test]
    fn parameters_schema_is_empty_object() {
        let schema = ListDisks.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["properties"].as_object().map(|m| m.is_empty()).unwrap_or(false),
            "list_disks takes no parameters - properties should be {{}}"
        );
    }

    #[test]
    fn safety_is_safe() {
        assert_eq!(ListDisks.safety(), SafetyClass::Safe);
    }
}

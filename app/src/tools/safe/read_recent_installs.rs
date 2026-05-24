//! [Safe] read_recent_installs —— 最近 N 天装的 Windows KB 更新清单。
//!
//! v3.0 W7。配套多个 skill：`/diagnose-bsod` / `/diagnose-slow` / `/fix-boot-failure` /
//! `/recover-bitlocker`。KB 安装常引入 BSOD / 启动失败 / BitLocker 循环 —— 时间线对齐
//! KB 安装时间是排查的关键。
//!
//! 用 `Get-WmiObject Win32_QuickFixEngineering`（比 Get-HotFix 跨版本更稳）。
//! 不查 Get-Package / Win32_Product（前者无安装日期，后者超慢且会触发 self-heal）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ReadRecentInstalls;

impl Tool for ReadRecentInstalls {
    fn name(&self) -> &str {
        "read_recent_installs"
    }

    fn description(&self) -> &str {
        "查最近 N 天装的 Windows KB 更新清单 —— 时间线对齐故障 / BSOD / BitLocker 循环。\n\
         \n\
         **When to use**: 用户说「最近 / 这几天 / 上周开始 出问题」时，**第一步**查最近 KB；\
         蓝屏诊断（/diagnose-bsod）的关联证据；\
         启动失败诊断（/fix-boot-failure）的 KB-触发判断；\
         BitLocker 恢复键循环（/recover-bitlocker）的根因定位（2025-2026 多次大规模 KB 触发，\
         如 KB5083769 / KB5089549）。\n\
         \n\
         **Parameters**:\n\
         - `days` (int, optional, 默认 30): 查最近多少天的 KB。一般用 7（最近一周）或 30（最近一月）；\
         BitLocker 排查可用 60\n\
         \n\
         **Returns**: JSON 数组（按安装时间从新到旧），每 KB 含：\n\
         - `HotFixID` (str): KB 编号（如 'KB5089549'）\n\
         - `Description` (str): 类型（Security Update / Update / Hotfix）\n\
         - `InstalledOn` (str): 安装日期 yyyy-MM-dd\n\
         - `InstalledBy` (str): 谁触发的（'NT AUTHORITY\\\\SYSTEM' = Windows Update 自动；\
         具体用户名 = 手动安装）\n\
         \n\
         **Example output**: `[{\"HotFixID\":\"KB5089549\",\"Description\":\"Security Update\",\
         \"InstalledOn\":\"2026-05-14\",\"InstalledBy\":\"NT AUTHORITY\\\\SYSTEM\"},\
         {\"HotFixID\":\"KB5083769\",\"Description\":\"Security Update\",\"InstalledOn\":\"2026-04-09\",\
         \"InstalledBy\":\"NT AUTHORITY\\\\SYSTEM\"}]`\n\
         \n\
         **Notes**: 只列 KB（QuickFixEngineering），**不**列普通软件安装（Get-Package 无日期，\
         Win32_Product 超慢且有副作用）；返回空数组 `[]` 表示该时间窗内无 KB 安装；\
         **关键对照**：把 KB 安装时间与用户报的「问题开始时间」对齐 —— 时间差 < 24 小时 = 高度嫌疑。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "days": {
                    "type": "integer",
                    "description": "查最近多少天的 KB（默认 30）",
                    "default": 30,
                    "minimum": 1,
                    "maximum": 365
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let days = args
            .get("days")
            .and_then(Value::as_i64)
            .unwrap_or(30)
            .clamp(1, 365);

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$cutoff = (Get-Date).AddDays(-{days})
ConvertTo-Json @(Get-WmiObject Win32_QuickFixEngineering -ErrorAction SilentlyContinue | Where-Object {{ $_.InstalledOn -and ($_.InstalledOn -gt $cutoff) }} | Sort-Object InstalledOn -Descending | Select-Object HotFixID, Description, @{{N='InstalledOn';E={{$_.InstalledOn.ToString('yyyy-MM-dd')}}}}, InstalledBy) -Depth 3 -Compress"#
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
        assert_v30_description_convention(&ReadRecentInstalls);
    }
}

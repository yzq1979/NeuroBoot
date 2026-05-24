//! [Safe] list_winre_status —— Windows Recovery Environment (WinRE) 状态 + 默认 boot 配置。
//!
//! v3.0 W7。配套 `/fix-boot-failure` skill。组合两个原生命令的输出：
//! - `reagentc /info` —— WinRE 是否启用、location、recovery image 等
//! - `bcdedit /enum {default}` —— 默认 boot 项的 device / osdevice / path / description
//!
//! 启动失败诊断的第一步：先看 WinRE 是否可达 + 默认 boot 配置是否完整。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListWinreStatus;

impl Tool for ListWinreStatus {
    fn name(&self) -> &str {
        "list_winre_status"
    }

    fn description(&self) -> &str {
        "查 WinRE（Windows Recovery Environment）状态 + 默认 boot 配置 —— 启动失败诊断第一步。\n\
         \n\
         **When to use**: 用户说「开不了机 / 卡 logo / no bootable device / 进自动修复循环」时；\
         需要判断 WinRE 是否可达（用户能不能进 Advanced Startup）；\
         需要看 BCD 默认 boot 项的 device / osdevice 配置；\
         配套 /fix-boot-failure skill 的第一步。\n\
         \n\
         **Parameters**: 无（无需管理员权限，但 admin 看到的更全）。\n\
         \n\
         **Returns**: JSON object 含：\n\
         - `WinReRaw` (str): `reagentc /info` 原始文本 —— 含 `Windows RE status: Enabled/Disabled`、\
         `Windows RE location: \\\\?\\GLOBALROOT\\device\\harddisk0\\partition4\\Recovery\\WindowsRE` 等\n\
         - `BcdDefaultRaw` (str): `bcdedit /enum {default}` 原始文本 —— 含 `device`、\
         `osdevice`、`path \\Windows\\system32\\winload.efi`、`description` 等\n\
         - `WinReEnabled` (bool): 解析 `Windows RE status: Enabled` 得到的 bool\n\
         \n\
         **Example output**: `{\"WinReRaw\":\"Windows Recovery Environment (Windows RE) and system reset configuration Information:\\n\\n    Windows RE status:         Enabled\\n    Windows RE location:       \\\\\\\\?\\\\GLOBALROOT\\\\device\\\\harddisk0\\\\partition4\\\\Recovery\\\\WindowsRE\\n    ...\",\"BcdDefaultRaw\":\"Windows Boot Loader\\n-------------------\\nidentifier              {current}\\ndevice                  partition=C:\\nosdevice                partition=C:\\n...\",\"WinReEnabled\":true}`\n\
         \n\
         **Notes**: WinReEnabled=false → 用户进不了 Advanced Startup，**只能用** PE / 安装介质救援；\
         BcdDefault 中 `device` 不是有效分区时 = BCD 损坏（下一步用 bootrec_rebuild_bcd）；\
         `Windows RE location` 是 GLOBALROOT 路径 = 在 recovery 分区里（多数 Win10/11 是 partition4）。"
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
        // reagentc 和 bcdedit 都是 native exe，stderr 偶有警告但不 fatal；
        // 用 try/catch 包，失败的子命令返回 "(failed)" 而非阻断
        let script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$reagent = try { (reagentc /info 2>&1 | Out-String).Trim() } catch { "(reagentc 调用失败：$_)" }
$bcd = try { (bcdedit /enum '{default}' 2>&1 | Out-String).Trim() } catch { "(bcdedit 调用失败：$_)" }
$enabled = $reagent -match 'Windows RE status:\s+Enabled'
ConvertTo-Json @{
    WinReRaw = $reagent
    BcdDefaultRaw = $bcd
    WinReEnabled = $enabled
} -Depth 3 -Compress"#;
        run_ps(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ListWinreStatus);
    }
}

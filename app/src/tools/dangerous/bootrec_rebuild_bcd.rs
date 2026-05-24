//! [DANGEROUS] bootrec_rebuild_bcd —— 重建启动配置 BCD store。
//!
//! `bootrec /rebuildbcd` —— 扫所有盘找 Windows 安装，重建 BCD store。
//! 启动修复经典工具。在 PE / WinRE 里跑（主系统起不来时）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct BootrecRebuildBcd;

impl Tool for BootrecRebuildBcd {
    fn name(&self) -> &str {
        "bootrec_rebuild_bcd"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS] 重建启动配置 BCD store** —— `bootrec /rebuildbcd`。\n\
         \n\
         **When to use**: 用户说「主系统起不来」「卡在 Windows 徽标」「找不到操作系统」「INACCESSIBLE_BOOT_DEVICE」；\
         **典型 PE 救援场景**：进 PE 修主系统启动；\
         双系统装新系统后旧系统消失（BCD 覆盖了）；\
         BCD 损坏 / 误删后。\n\
         \n\
         **When NOT to use**: 主系统能正常启动（这个是「起不来才用」的工具）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: bootrec stdout（找到的 Windows 安装清单 + 是否加进 BCD）。\n\
         \n\
         **Notes**: 工具会**交互式提问**「是否把这个 Windows 加进 BCD」—— 在 PE 里跑时**不会真的弹问** \
         （因为 stdin 非 tty），可能直接跳过；如果跳过，下一步可以用 `bcdboot C:\\Windows /s S: /f UEFI`（v2.x 加）；\
         BCD 错误重写后**必须重启**才能验证主系统是否能起来。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
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
bootrec /rebuildbcd"#;
        run_ps(script)
    }
}

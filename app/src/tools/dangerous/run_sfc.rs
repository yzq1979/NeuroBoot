//! [DANGEROUS] run_sfc_scannow —— System File Checker 修系统文件。
//!
//! `sfc /scannow` —— 扫所有受保护系统文件，损坏的从 WinSxS cache 还原。
//! 低风险（只动 WinSxS 里有备份的文件），但耗时（10~30 分钟）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct RunSfcScannow;

impl Tool for RunSfcScannow {
    fn name(&self) -> &str {
        "run_sfc_scannow"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS, 但低风险] 修系统文件** —— `sfc /scannow`。\n\
         \n\
         **When to use**: 系统文件可疑损坏（Windows 错误、组件加载失败、Event Log 报 SideBySide）；\
         安装 update 后异常；恶意软件感染后清理；用户说「Windows 不对劲」。\n\
         \n\
         **When NOT to use**: 仅是性能问题；用户没具体报错。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: sfc 完整 stdout，结尾会告诉用户「没发现完整性损坏」/「修复了 X 个」/\
         「发现损坏但无法修复（要跑 dism）」。\n\
         \n\
         **Notes**: 耗时 10~30 分钟（取决于盘 IO）；如果它报修复失败，下一步调 `run_dism_restorehealth`；\
         比 chkdsk 安全得多（只动 WinSxS 缓存里有的文件，不动用户数据）。"
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
sfc /scannow"#;
        run_ps(script)
    }
}

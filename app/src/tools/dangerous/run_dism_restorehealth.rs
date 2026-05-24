//! [DANGEROUS] run_dism_restorehealth —— 在线修复系统镜像 (Component Store)。
//!
//! `DISM /Online /Cleanup-Image /RestoreHealth` —— sfc 修不了的时候用，
//! 从 Windows Update / 本地 WIM 拉缺失文件来修复 Component Store。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct RunDismRestoreHealth;

impl Tool for RunDismRestoreHealth {
    fn name(&self) -> &str {
        "run_dism_restorehealth"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS, 低风险] 在线修复系统镜像** —— `DISM /Online /Cleanup-Image /RestoreHealth`。\n\
         \n\
         **When to use**: sfc /scannow 报「修复失败」时；Windows Update 装不上；\
         系统组件加载失败需要从源拉缺失文件；sfc 之后的下一步排查。\n\
         \n\
         **When NOT to use**: 仅是单个软件出问题；网络不通时（要联网拉 WU 源）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: DISM stdout（进度百分比 + 最终成功/失败）。\n\
         \n\
         **Example output**（截选末尾）: ```\n\
         [==========================100.0%==========================]\n\
         The restore operation completed successfully.\n\
         The operation completed successfully.\n\
         ```\n\
         \n\
         **Notes**: 默认从 Windows Update 拉，**需要联网**；联网失败可以指定 `/Source:wim:<wim_path>:1` 走本地，但本工具走默认；\
         耗时 10~60 分钟（看网速）；跑完一般应再跑一次 sfc /scannow 确认。"
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
DISM.exe /Online /Cleanup-Image /RestoreHealth"#;
        run_ps(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&RunDismRestoreHealth);
    }
}

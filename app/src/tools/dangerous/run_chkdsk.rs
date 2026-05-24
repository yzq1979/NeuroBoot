//! [DANGEROUS] run_chkdsk —— 修磁盘错误 + 坏块重映射。
//!
//! `chkdsk <drive> /f /r` —— /f 修文件系统错误，/r 检测并标记坏扇区 + 重映射。
//! 注意：跑盘必须当时未被独占使用；C: 跑这个会要求重启后扫描。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct RunChkdsk;

impl Tool for RunChkdsk {
    fn name(&self) -> &str {
        "run_chkdsk"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS] 修文件系统错误 + 坏扇区重映射** —— `chkdsk <drive> /f /r`。\n\
         \n\
         **When to use**: 用户硬盘有可疑文件损坏、读取卡死、CHKDSK 警告时；\
         接 read_event_log_errors 看到 Source=Ntfs 类报错；read_storage_reliability 报错率高。\n\
         \n\
         **When NOT to use**: 仅是「电脑慢」「想清理空间」时；用户没明确说要修复文件系统。\n\
         \n\
         **Parameters**:\n\
         - `drive` (string, 必须): 盘符，1 字符（如 'D'）或带冒号（如 'D:'）。**不要传 'C:'** 除非用户明确同意重启后扫\n\
         \n\
         **Returns**: chkdsk 完整 stdout（多行进度 + 最终统计）。\n\
         \n\
         **Example output**（截选末尾统计）: ```\n\
         Windows has scanned the file system and found no problems.\n\
         No further action is required.\n\
         \n\
           976559615 KB total disk space.\n\
           742891024 KB in 287456 files.\n\
                4096 KB in 23478 indexes.\n\
                   0 KB in bad sectors.\n\
         ```\n\
         \n\
         **Notes**: 跑 C: 会触发「下次启动时扫描」（无法在线修复系统盘）；用户需手动重启确认；\
         其它盘可能要求当时未被独占使用（关掉占用程序后重试）；过程不可中断 —— **跑大盘可能数小时**。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "drive": {
                    "type": "string",
                    "description": "盘符（如 'D' 或 'D:'）"
                }
            },
            "required": ["drive"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let drive = args
            .get("drive")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 drive 参数"))?
            .trim()
            .trim_end_matches([':', '\\', '/']);

        if drive.len() != 1 || !drive.chars().next().unwrap().is_ascii_alphabetic() {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                format!("drive 必须是单个字母 A~Z，收到 `{drive}`"),
            ));
        }

        let drive_upper = drive.to_uppercase();
        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
chkdsk {drive_upper}: /f /r"#
        );
        run_ps(&script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&RunChkdsk);
    }
}

//! [DANGEROUS] reset_local_admin_password —— 通过 NTPWEdit 重置本地 Admin 密码 / 解锁账户。
//!
//! NTPWEdit (~500 KB portable, freeware) 直接编辑 SAM hive，把指定账户的密码清空 / 解锁。
//! **PE 救援的旗舰场景**：用户忘了管理员密码，进 PE 跑这工具清空，重启就能登录。
//!
//! 工具二进制不在 NeuroBoot ISO 默认带 —— 由用户 / 构建者按 docs/BUILD.md 下载放
//! `X:\NeuroBoot\tools\NTPWEdit\NTPWEdit.exe` (PE) 或 `C:\NeuroBoot\tools\NTPWEdit\NTPWEdit.exe`。

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct ResetLocalAdminPassword;

fn find_ntpwedit() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"X:\NeuroBoot\tools\NTPWEdit\NTPWEdit.exe"),
        PathBuf::from(r"C:\NeuroBoot\tools\NTPWEdit\NTPWEdit.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

impl Tool for ResetLocalAdminPassword {
    fn name(&self) -> &str {
        "reset_local_admin_password"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS, 救援旗舰] 清空 Windows 本地账户密码** —— 通过 NTPWEdit 编辑 SAM hive。\n\
         \n\
         **When to use**: 用户**明确说**「忘了 Windows 登录密码」「账户被锁」「重置管理员密码」。\
         **PE 救援盘的金牌场景**：进 PE 跑这个，重启回主系统就能登。\n\
         \n\
         **When NOT to use**: 任何其它情况。**绝不**为「测试」「玩玩看」调用。\
         在他人电脑上跑前必须确认有合法权限（自己电脑 / 客户授权 / 公司 IT）。\n\
         \n\
         **Parameters**:\n\
         - `sam_path` (string, required): SAM hive 路径。典型：`C:\\Windows\\System32\\config\\SAM`\n\
         - `username` (string, required): 要清密码的账户名（如 `Administrator`、`admin`、用户自定义）\n\
         \n\
         **Returns**: 启动确认信息（NTPWEdit 是 GUI 工具，实际操作在弹窗里完成）。\n\
         \n\
         **Example output**: ```\n\
         已启动 NTPWEdit GUI（pid 不返回，请在弹窗里：\n\
         1. 选中账户 `Administrator`\n\
         2. 点「Change password」清空（不输入新密码就是清空）\n\
         ...\n\
         ```\n\
         \n\
         **Notes**: SAM hive 路径在 PE 里挂载主系统盘后才能访问；典型 `C:\\Windows\\System32\\config\\SAM` \
         实际是主系统盘下；NTPWEdit 是 freeware（自用 OK，公开重分发需复查 license）；\
         **需要 NeuroBoot ISO 带 NTPWEdit.exe**（默认不带，按 docs/BUILD.md 下载放到 \
         `X:\\NeuroBoot\\tools\\NTPWEdit\\NTPWEdit.exe`）；未找到 binary 返回 NotFound。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sam_path": {
                    "type": "string",
                    "description": "SAM hive 路径，PE 里通常是 C:\\Windows\\System32\\config\\SAM"
                },
                "username": {
                    "type": "string",
                    "description": "账户名"
                }
            },
            "required": ["sam_path", "username"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let sam_path = args
            .get("sam_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 sam_path 参数")
            })?;
        let username = args
            .get("username")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 username 参数")
            })?;

        let exe = find_ntpwedit().ok_or_else(|| {
            ToolError::with_kind(
                ToolErrorKind::NotFound,
                "NTPWEdit.exe 未找到。NeuroBoot 默认 ISO 不带这个工具；\
                 请按 docs/BUILD.md 「救援工具下载」节下载（~500 KB）放到 \
                 X:\\NeuroBoot\\tools\\NTPWEdit\\ 后再试。",
            )
        })?;

        // NTPWEdit CLI: `NTPWEdit.exe <sam_path>` 然后交互；
        // 由于 PE / non-tty 环境交互式 fail，建议用户用 GUI 版手动操作。
        // 这里仅启动 GUI 让用户在弹窗里完成。
        Command::new(&exe)
            .arg(sam_path)
            .spawn()
            .map_err(|e| ToolError::new(format!("启动 NTPWEdit 失败：{e}")))?;

        Ok(format!(
            "已启动 NTPWEdit GUI（pid 不返回，请在弹窗里：\n\
             1. 选中账户 `{username}`\n\
             2. 点「Change password」清空（不输入新密码就是清空）\n\
             3. 点「Save changes」+ 确认\n\
             4. 关闭 NTPWEdit\n\
             5. 重启回主系统验证（重启前确认主系统未挂载 SAM）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ResetLocalAdminPassword);
    }
}

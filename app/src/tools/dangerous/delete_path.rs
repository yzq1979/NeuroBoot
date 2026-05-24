//! Dangerous 工具：delete_path —— 把路径移到回收站（v2 Stage 4.2 起 move-to-trash 模式）。
//!
//! v1 行为：`Remove-Item -LiteralPath -Recurse -Force`（真删）
//! **v2 Stage 4.2 行为**：`Move-Item` 到 `X:\trash\<timestamp>\<name>`（不真删，给翻车机会）
//!
//! 给模型看的工具名仍是 delete_path —— 模型不知道这层包装。用户在 UI 「清空 trash」可手动彻底删。
//!
//! 防御层：
//! - safety = Dangerous → agent loop 走确认弹窗
//! - **preflight::check_path_safety()** 拒系统目录 + 整盘根（v2 Stage 4.5 通用化的黑名单）
//! - `-LiteralPath` 防 PowerShell wildcards 误展开

use serde_json::{json, Value};

use crate::tools::preflight::check_path_safety;
use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct DeletePath;

impl Tool for DeletePath {
    fn name(&self) -> &str {
        "delete_path"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS] 移到回收站** —— 把指定路径移到 `X:\\trash\\<timestamp>\\` 而非真删除（v2 Stage 4.2 起）。\n\
         \n\
         **When to use**: 仅当用户**明确请求删除某个文件/目录**时才调用。\n\
         \n\
         **When NOT to use**（重要！）:\n\
         - 用户说「电脑慢」「想清理空间」时**不要**调用 —— 那是诊断请求不是删除请求\n\
         - 用户说「重置」「恢复出厂」「修复 Windows」时**不要**用本工具\n\
         - 系统路径（C:\\Windows、System32、Program Files、ProgramData）**绝不**调 —— 会被 preflight 拒\n\
         - 用户没明确说删除时**不要**自作主张删除「看起来没用」的文件\n\
         \n\
         **Parameters**:\n\
         - `path` (string, required): 绝对路径。Windows 路径分隔符 `\\\\` 或 `/` 都可\n\
         \n\
         **Returns**: 成功返回「(已移到回收站) <trash 路径>」；路径不存在返回「(不存在) ...」；\
         命中 preflight 黑名单返回 error (PermissionDenied)。\n\
         \n\
         **Example output**: ```\n\
         (已移到回收站) 从 D:\\old-installer.exe 移到 X:\\trash\\20260524-153012-4783\\old-installer.exe\n\
         如需恢复：剪切 X:\\trash\\...\\old-installer.exe 回原位置；如需彻底删除，UI 点「清空 trash」按钮\n\
         ```\n\
         \n\
         **Notes**:\n\
         - **执行流程**: UI 必弹确认弹窗 → 用户必须人工点「确认执行」 → 才真移动。\
         用户点取消 → 工具返回（用户拒绝）→ **不要重试相同操作**，问用户是否换个方式\n\
         - **恢复**: 移到回收站后，用户可在 UI 顶栏「日志」附近找到「清空 trash」按钮彻底删；\
         在彻底删之前用户可以从 `X:\\trash\\<timestamp>\\` 手动 cut 回去"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要移到回收站的绝对路径，文件或目录"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少必须参数 path（字符串）")
            })?;

        // v2 Stage 4.5: 通用 path 安全 pre-check（替代 v1 的局部黑名单）
        check_path_safety(path)?;

        // PowerShell 单引号字符串里，引号字面量是连写两个单引号
        let path_escaped = path.replace('\'', "''");

        // v2 Stage 4.2: move-to-trash 模式
        // - X:\trash\<yyyyMMdd-HHmmss-rand>\<orig-name>
        // - 同名冲突时加随机后缀
        // - 跨盘 Move-Item 退化成 Copy + Remove（PS 自动处理）
        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$p = '{path_escaped}'
if (-not (Test-Path -LiteralPath $p)) {{
    Write-Output "(不存在) 路径 $p 不存在，未执行任何操作"
    exit 0
}}

# 选回收站根：X:\trash 优先（PE），回落 C:\NeuroBoot\trash（开发机）
$trashRoot = 'X:\trash'
if (-not (Test-Path 'X:\')) {{
    $trashRoot = 'C:\NeuroBoot\trash'
}}
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$rand = Get-Random -Maximum 9999
$destDir = Join-Path $trashRoot "$timestamp-$rand"
New-Item -ItemType Directory -Path $destDir -Force | Out-Null

# Move 到回收站；保留原文件名
$srcName = Split-Path $p -Leaf
$dest = Join-Path $destDir $srcName
Move-Item -LiteralPath $p -Destination $dest -Force -ErrorAction Stop
Write-Output "(已移到回收站) 从 $p 移到 $dest"
Write-Output "如需恢复：剪切 $dest 回原位置；如需彻底删除，UI 点「清空 trash」按钮"
"#
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
        assert_v30_description_convention(&DeletePath);
    }
}

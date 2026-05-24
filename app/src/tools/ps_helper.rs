//! PowerShell 工具执行 helper —— 把每个工具里复制的 spawn / stderr 检查 / UTF-8 解码集中。
//!
//! v2 增加多个 PS 驱动的 Safe 工具时引入；4 个 v1 工具迁过来减少重复。
//!
//! 调用约定：
//! - 调用方传完整 PS 脚本（已含 `[Console]::OutputEncoding = UTF-8` 前缀 + `ConvertTo-Json` 收尾）
//! - 本 helper 用固定参数 `-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command`
//! - 非零退出 → 返回 ToolError 含 stderr trim 后的内容（给模型读以决策）
//! - 空 stdout → 由调用方决定怎么处理（通过 `run_ps_json_array` 自动返回 "[]"，或用 `run_ps` 拿 raw）

use std::process::Command;

use crate::tools::registry::{ToolError, ToolOutput};

/// 跑 PS 脚本拿 raw stdout。空 stdout 返回 Ok("") 不当错误。
pub fn run_ps(script: &str) -> ToolOutput {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| ToolError::new(format!("启动 powershell.exe 失败：{e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::new(format!(
            "PowerShell 命令失败 (exit {}):\n{}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 跑 PS 脚本拿 JSON 数组输出。空 stdout 时返回 `"[]"`（让 LLM 看到合法空数组而不是空字符串）。
///
/// 注意：这要求 PS 脚本用 `@(...)` 强制成数组传给 `ConvertTo-Json`，
/// 因为 PS 5.1 的 ConvertTo-Json 对单元素不会自动包数组（会输出对象而非数组）。
pub fn run_ps_json_array(script: &str) -> ToolOutput {
    let stdout = run_ps(script)?;
    if stdout.is_empty() {
        Ok("[]".to_owned())
    } else {
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ps_simple_echo() {
        // Smoke test：能 spawn powershell.exe 拿到 UTF-8 stdout
        let out = run_ps("Write-Output 'hello'").expect("PS should run");
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn run_ps_json_array_handles_empty() {
        // 输出空时返回 "[]"（合法空数组让 LLM 看到，而非歧义空串）
        let out = run_ps_json_array("Write-Output ''").expect("PS should run");
        assert_eq!(out, "[]");
    }

    #[test]
    fn run_ps_nonzero_exit_returns_err() {
        // Throw 异常 -> exit code 非 0 -> 返回 ToolError
        let err = run_ps("throw 'forced-error'").expect_err("should error");
        assert!(err.message.contains("PowerShell"), "msg: {}", err.message);
    }
}


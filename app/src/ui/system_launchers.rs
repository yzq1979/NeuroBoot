//! 系统启动器：打开 cmd / 文件管理器 —— 让用户能在不退出 NeuroBoot 的前提下做点 PE 维护。
//!
//! 阶段 v1.0.1+ 新增。两个按钮的设计要点：
//!
//! 1. **打开 cmd**：`cmd.exe /k cd /d X:\NeuroBoot && cls`
//!    - `/k` = 跑完保持窗口（用户继续敲命令），不像 `/c` 跑完就关
//!    - 切到 `X:\NeuroBoot` 让用户能直接看到 logs/ 等目录
//!    - PE 里 cmd.exe 一定存在（PE 的默认 shell），这条不会失败
//!
//! 2. **打开文件管理器**：先试 `explorer.exe`，**默认 ADK WinPE 不带**所以基本会失败 → 回落到 cmd
//!    带 `dir` 列当前盘，提示用户 PE 里没图形文件管理器；将来可以打包 Q-Dir / Total Commander portable
//!
//! 主系统调试时这俩按钮也能用 —— cmd 弹真窗口、explorer 真打开。

use std::process::Command;

/// 启动器成功后返回的描述（UI 在消息流里追加显示让用户知道点了什么）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchResult {
    pub program: String,
    pub note: String,
}

/// 打开一个新的 cmd 窗口，工作目录切到 `X:\NeuroBoot`（PE）或 `C:\NeuroBoot`（主系统调试 fallback）。
pub fn launch_cmd() -> Result<LaunchResult, String> {
    // 选 cwd：PE 优先 X:\NeuroBoot；主系统调试时 X:\ 不存在，回落 C:\NeuroBoot
    let cwd = if std::path::Path::new("X:\\NeuroBoot").exists() {
        "X:\\NeuroBoot"
    } else if std::path::Path::new("C:\\NeuroBoot").exists() {
        "C:\\NeuroBoot"
    } else {
        "C:\\"
    };

    // /k = 跑完保持窗口；cls 清屏让欢迎信息可见
    Command::new("cmd.exe")
        .args(["/k", &format!("cd /d \"{cwd}\" && cls && echo NeuroBoot cmd && echo cwd: {cwd}")])
        .spawn()
        .map_err(|e| format!("启动 cmd.exe 失败：{e}"))?;

    Ok(LaunchResult {
        program: "cmd.exe".to_owned(),
        note: format!("工作目录: {cwd}"),
    })
}

/// 打开文件管理器。
///
/// 流程：
/// 1. 试 `explorer.exe <path>` —— ADK WinPE 默认不带，但用户可能自己加了或用了第三方 PE
/// 2. explorer 失败 → 回落 cmd 在目标目录跑 `dir`，告诉用户「PE 没图形管理器」
pub fn launch_file_manager() -> Result<LaunchResult, String> {
    let target = if std::path::Path::new("X:\\").exists() {
        "X:\\"
    } else {
        "C:\\"
    };

    // 试 explorer
    let explorer_result = Command::new("explorer.exe").arg(target).spawn();
    if let Ok(_child) = explorer_result {
        return Ok(LaunchResult {
            program: "explorer.exe".to_owned(),
            note: format!("浏览 {target}"),
        });
    }

    // 回落到 cmd dir 列表
    Command::new("cmd.exe")
        .args([
            "/k",
            &format!(
                "cd /d \"{target}\" && cls && echo [Fallback] explorer.exe \
                 not available in PE. Showing dir list:&& dir"
            ),
        ])
        .spawn()
        .map_err(|e| format!("explorer 不存在 + cmd fallback 也失败：{e}"))?;

    Ok(LaunchResult {
        program: "cmd.exe (fallback)".to_owned(),
        note: format!("PE 默认不带 explorer.exe；在 cmd 里 dir 列了 {target}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_result_struct_is_clonable() {
        let r1 = LaunchResult {
            program: "test".to_owned(),
            note: "note".to_owned(),
        };
        let r2 = r1.clone();
        assert_eq!(r1, r2);
    }
    // 注：不在单测里真起 cmd / explorer 进程（会污染 CI / 开发环境）。
    // 真验证靠 cargo run 手测 + PE 真测。
}

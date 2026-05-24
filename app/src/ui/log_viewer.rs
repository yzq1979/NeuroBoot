//! 工具执行日志查看 —— v2 Stage 3.3。
//!
//! 用户在 UI 点「查看日志」 → 启动一个新 cmd 窗口 cd 到日志目录 + 列出今天的日志。
//! 实际查看仍走 cmd（PE 没文本编辑器；用 `type` / `more` 翻日志）。

use std::process::Command;

/// 跟 system_launchers 同款返回结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogOpenResult {
    pub note: String,
}

/// 启动 cmd 窗口列出今天的 audit log，方便用户翻看。
///
/// 流程：
/// - 找日志目录（X:\NeuroBoot\logs 优先 → C:\NeuroBoot\logs）
/// - 启动 cmd /k cd /d <dir> && dir + 提示 type 命令
pub fn open_log_dir() -> Result<LogOpenResult, String> {
    let candidates = [
        "X:\\NeuroBoot\\logs",
        "C:\\NeuroBoot\\logs",
    ];
    let dir = candidates
        .iter()
        .find(|d| std::path::Path::new(d).exists())
        .copied()
        .ok_or_else(|| {
            "日志目录不存在（X:\\NeuroBoot\\logs 和 C:\\NeuroBoot\\logs 都没有）。\
             如果你还没调过任何工具，日志文件还不会被创建。"
                .to_owned()
        })?;

    Command::new("cmd.exe")
        .args([
            "/k",
            &format!(
                "cd /d \"{dir}\" && cls && echo NeuroBoot 工具执行日志目录: {dir}&& echo. && \
                 echo 文件列表：&& dir /b *.jsonl 2>nul && echo. && \
                 echo 查看今天的日志，敲：type tool-YYYYMMDD.jsonl ^| more"
            ),
        ])
        .spawn()
        .map_err(|e| format!("启动 cmd.exe 失败：{e}"))?;

    Ok(LogOpenResult {
        note: format!("已打开 {dir} 在新 cmd 窗口"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_clonable() {
        let r1 = LogOpenResult {
            note: "test".to_owned(),
        };
        assert_eq!(r1, r1.clone());
    }
}

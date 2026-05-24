//! 工具执行审计日志 —— v2 Stage 3.2。
//!
//! 每次 agent 调一个工具（无论 safe / dangerous / 成功 / 失败），追加一行 JSONL 到
//! `<log_dir>/tool-YYYYMMDD.jsonl`。让用户事后可以审计「Agent 在我的电脑上做过什么」。
//!
//! 日志目录选择策略：
//! - 优先 `X:\NeuroBoot\logs\` （PE 内运行）
//! - 否则 `C:\NeuroBoot\logs\` （开发机或非 PE）
//! - 都不行（写权限不足）→ 日志记录被跳过，**不影响工具本身执行**
//!
//! 字段：
//! - `ts`: ISO 时间戳（yyyy-MM-dd HH:mm:ss）
//! - `tool`: 工具名（如 "list_disks"）
//! - `args`: 模型给的 JSON 参数字符串（截前 500 字符）
//! - `safety`: "safe" / "dangerous"
//! - `user_confirmed`: dangerous 工具用户是否确认（safe 工具为 null）
//! - `duration_ms`: 工具执行毫秒数（不含等待用户确认的时间）
//! - `success`: 是否成功（exit code 0 + 无 Err）
//! - `result_summary`: stdout 首 500 字符（避免日志爆）

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolAudit {
    pub ts: String,
    pub tool: String,
    pub args: String,
    pub safety: String,
    pub user_confirmed: Option<bool>,
    pub duration_ms: u128,
    pub success: bool,
    pub result_summary: String,
}

/// 选日志目录。优先 PE 的 `X:\NeuroBoot\logs\`，回落到 dev `C:\NeuroBoot\logs\`，
/// 都不行返回 None（调用方知道日志被跳过）。
fn pick_log_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("X:\\NeuroBoot\\logs"),
        PathBuf::from("C:\\NeuroBoot\\logs"),
    ];
    for dir in candidates {
        // 父目录必须已存在（X:\NeuroBoot 是 PE 有的；C:\NeuroBoot 是开发机有的）
        if let Some(parent) = dir.parent() {
            if !parent.exists() {
                continue;
            }
        }
        if std::fs::create_dir_all(&dir).is_ok() {
            return Some(dir);
        }
    }
    None
}

/// 取当前本地时间作日期串（用于文件名 + 时间戳字段）。
///
/// 用 Win32 GetLocalTime FFI（已在 ui::status_bar 用同样方式），免引入 chrono。
fn local_time_components() -> (u16, u16, u16, u16, u16, u16) {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(lpSystemTime: *mut SystemTime);
    }
    let mut st = SystemTime::default();
    unsafe { GetLocalTime(&mut st) };
    (st.year, st.month, st.day, st.hour, st.minute, st.second)
}

/// 截前 N 字符（按 char count，不按 byte，防中文截断）+ 加省略号。
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("...");
    out
}

/// 构造审计记录（不写盘 —— 适合给单测和调用方组装数据）。
pub fn build_audit(
    tool: &str,
    args: &str,
    safety: &str,
    user_confirmed: Option<bool>,
    duration: Duration,
    success: bool,
    result: &str,
) -> ToolAudit {
    let (y, mo, d, h, mi, s) = local_time_components();
    let ts = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s);
    ToolAudit {
        ts,
        tool: tool.to_owned(),
        args: truncate_chars(args, 500),
        safety: safety.to_owned(),
        user_confirmed,
        duration_ms: duration.as_millis(),
        success,
        result_summary: truncate_chars(result, 500),
    }
}

/// 追加一行 JSONL 到日志文件。失败静默忽略（日志非关键路径）。
pub fn append(audit: &ToolAudit) {
    let Some(log_dir) = pick_log_dir() else {
        return;
    };
    let (y, mo, d, _, _, _) = local_time_components();
    let file_path = log_dir.join(format!("tool-{:04}{:02}{:02}.jsonl", y, mo, d));

    let json = match serde_json::to_string(audit) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = writeln!(file, "{}", json);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate_chars("hello", 100), "hello");
    }

    #[test]
    fn truncate_long_gets_ellipsis() {
        let s = "a".repeat(600);
        let out = truncate_chars(&s, 500);
        assert!(out.ends_with("..."));
        // 500 chars + "..."
        assert_eq!(out.chars().count(), 503);
    }

    #[test]
    fn truncate_chinese_safe() {
        let s = "你好".repeat(300); // 600 chars
        let out = truncate_chars(&s, 500);
        // 必须按 char count 而非 byte 截
        assert_eq!(out.chars().count(), 503);
        // 不会切坏中文 char
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn build_audit_round_trip() {
        let a = build_audit(
            "list_disks",
            "{}",
            "safe",
            None,
            Duration::from_millis(120),
            true,
            "[{\"Number\":0}]",
        );
        assert_eq!(a.tool, "list_disks");
        assert_eq!(a.duration_ms, 120);
        assert!(a.success);
        assert!(a.user_confirmed.is_none());
        // ts 格式 yyyy-MM-dd HH:mm:ss
        assert_eq!(a.ts.len(), 19);

        // 能 JSON 序列化
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("list_disks"));
        assert!(json.contains("\"safety\":\"safe\""));
    }
}

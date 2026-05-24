//! 通用 path 安全 pre-check —— v2 Stage 4.5。
//!
//! 任何 dangerous 工具 path 参数走这里检查；命中关键系统目录直接拒，不到 execute。
//! 是 delete_path 已有黑名单的通用化，给未来的 move_path / rename_file / format_partition 复用。

use crate::tools::registry::{ToolError, ToolErrorKind};

/// 关键系统根（lowercase；不能用作 dangerous 操作目标）。
const BANNED_PREFIXES: &[&str] = &[
    "c:\\windows",
    "c:/windows",
    "c:\\program files",
    "c:/program files",
    "c:\\program files (x86)",
    "c:/program files (x86)",
    "c:\\programdata",
    "c:/programdata",
];

/// 整盘根（删 = 全盘擦）。
const BANNED_EXACT: &[&str] = &[
    "c:", "c:\\", "c:/",
    "d:", "d:\\", "d:/",
    "e:", "e:\\", "e:/",
    "f:", "f:\\", "f:/",
    "g:", "g:\\", "g:/",
    "h:", "h:\\", "h:/",
    "x:", "x:\\", "x:/",  // PE ramdisk
];

/// 检查 path 是否安全（不在系统目录 / 不是整盘根）。
///
/// 成功 → Ok(())；命中黑名单 → Err(ToolError::with_kind(PermissionDenied, ...))
/// 给模型看的 message 含命中规则名，方便 LLM 改路径再试。
pub fn check_path_safety(path: &str) -> Result<(), ToolError> {
    let normalized = path.trim().trim_end_matches(['\\', '/']).to_lowercase();

    if normalized.is_empty() {
        return Err(ToolError::with_kind(
            ToolErrorKind::InvalidArgument,
            "path 不能为空字符串",
        ));
    }

    if BANNED_EXACT.contains(&normalized.as_str()) {
        return Err(ToolError::with_kind(
            ToolErrorKind::PermissionDenied,
            format!("拒绝操作整盘根 `{path}` —— 模型层 pre-check。如要清理空间，请指定具体子目录。"),
        ));
    }

    for prefix in BANNED_PREFIXES {
        if normalized == *prefix || normalized.starts_with(&format!("{prefix}\\"))
            || normalized.starts_with(&format!("{prefix}/"))
        {
            return Err(ToolError::with_kind(
                ToolErrorKind::PermissionDenied,
                format!(
                    "拒绝操作系统目录 `{path}` —— 命中黑名单前缀 `{prefix}`。\
                     系统目录由 Windows 自管理，第三方工具不应碰；如要清理用户数据，请改去 \
                     `C:\\Users\\<username>\\` 下找。"
                ),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_path_ok() {
        assert!(check_path_safety("C:\\Users\\me\\Desktop\\test.txt").is_ok());
        assert!(check_path_safety("D:\\old-backups\\stuff").is_ok());
        assert!(check_path_safety("X:\\NeuroBoot\\logs").is_ok());
    }

    #[test]
    fn system_root_blocked() {
        for p in [
            "C:\\Windows",
            "c:\\windows",
            "C:\\Windows\\System32",
            "C:\\Windows\\System32\\drivers",
            "C:/Windows/System32",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\ProgramData",
        ] {
            let result = check_path_safety(p);
            assert!(result.is_err(), "{p} should be blocked");
            let err = result.unwrap_err();
            assert_eq!(err.kind, ToolErrorKind::PermissionDenied);
        }
    }

    #[test]
    fn drive_root_blocked() {
        for p in ["C:", "C:\\", "C:/", "D:\\", "X:"] {
            assert!(
                check_path_safety(p).is_err(),
                "{p} drive root should be blocked"
            );
        }
    }

    #[test]
    fn empty_path_invalid_argument() {
        let err = check_path_safety("").unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
        let err = check_path_safety("   ").unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
    }

    #[test]
    fn case_insensitive() {
        assert!(check_path_safety("C:\\WINDOWS\\System32").is_err());
        assert!(check_path_safety("c:\\windows\\system32").is_err());
        assert!(check_path_safety("C:\\Windows\\SYSTEM32").is_err());
    }
}

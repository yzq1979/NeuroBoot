//! Persistent Memory —— v3.0 W6-7。
//!
//! 跨 PE 重启 / 跨会话保留的 markdown 文件存储。Memory root 在 U 盘
//! `<USB>\NeuroBoot\memories\` 下，跟 [`crate::ui::prompts_file`] 用同样的扫盘逻辑
//! （所有非 X: 盘根 + 取第一个找到的）。
//!
//! ## 灵感
//!
//! [Anthropic Memory Tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
//! + Claude Code 的 `~/.claude/memory/MEMORY.md` 模型。
//!
//! ## 6 命令（[`MemoryOp`]）
//!
//! | 命令 | 用途 |
//! |---|---|
//! | `view`      | 读单个文件全文，或列目录内容 |
//! | `create`    | 写新文件或覆盖现有文件（含 MEMORY.md） |
//! | `str_replace` | 替换文件里指定子串第 1 次出现 |
//! | `insert`    | 在指定行号前插入新行 |
//! | `delete`    | 删除文件 |
//! | `rename`    | 改名 / 移动（仍限 root 内） |
//!
//! ## 路径安全（path traversal guard）
//!
//! 用户 / LLM 给的路径都通过 [`resolve_inside_root`] 校验：
//! 1. 拼到 `root/<user-path>` 后用 `std::path::absolute` 规范化（**不**用 canonicalize ——
//!    后者要文件存在，create 场景失败）
//! 2. 校验结果路径必须以 `root` 开头（防 `..` 跳出）
//! 3. 拒绝绝对路径输入（`C:\foo` / `/etc/passwd`）和含 `..` 的输入
//! 4. 拒绝 NUL 字节 / 含驱动器 prefix
//!
//! ## SessionStart 行为
//!
//! [`load_memory_md`] 读 `<root>/MEMORY.md` —— 启动时由 main.rs 拼到 system prompt
//! （行为对齐 Claude Code 的 auto memory）。

use std::path::{Component, Path, PathBuf};

/// Memory 根目录约定的相对路径（U 盘根下）。
pub const MEMORY_SUBDIR: &str = r"NeuroBoot\memories";

/// 扫所有非 `X:` 盘根，找第一个 `NeuroBoot\memories\` 目录。
///
/// 如果该目录**不存在**也返回 None；调用方（create 场景）应该用 [`ensure_root_for_create`]
/// 在第一个可写盘上自动建。
pub fn scan_memory_root() -> Option<PathBuf> {
    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        let candidate = PathBuf::from(format!("{c}:\\{MEMORY_SUBDIR}"));
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// create 场景：找第一个可写非 X 盘，确保 `NeuroBoot\memories\` 存在。
///
/// 行为：
/// 1. 先用 [`scan_memory_root`] 找已存在的 —— 优先复用
/// 2. 找不到 → 扫所有非 X 盘，对每个测试根目录可写性（试建临时文件）
/// 3. 找到第一个可写的 → mkdir `NeuroBoot\memories\` → 返回路径
pub fn ensure_root_for_create() -> Result<PathBuf, MemoryError> {
    if let Some(p) = scan_memory_root() {
        return Ok(p);
    }
    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        let root_drive = PathBuf::from(format!("{c}:\\"));
        if !root_drive.is_dir() {
            continue;
        }
        let test_file = root_drive.join(".neuroboot_write_test.tmp");
        if std::fs::write(&test_file, b"x").is_ok() {
            let _ = std::fs::remove_file(&test_file);
            let target = PathBuf::from(format!("{c}:\\{MEMORY_SUBDIR}"));
            std::fs::create_dir_all(&target).map_err(|e| {
                MemoryError::IoError(format!("创建 {} 失败：{e}", target.display()))
            })?;
            return Ok(target);
        }
    }
    Err(MemoryError::NoWritableDrive)
}

/// 加载 `<root>/MEMORY.md` 全文 —— SessionStart 用。返回 None 表示文件不存在或 root 没找到。
pub fn load_memory_md() -> Option<String> {
    let root = scan_memory_root()?;
    let path = root.join("MEMORY.md");
    std::fs::read_to_string(&path).ok()
}

/// memory 模块的错误分类（映射到 ToolError）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    /// 路径越界 / 含 `..` / 绝对路径输入
    PathTraversal(String),
    /// 没找到 memory root 且没可写盘建
    NoWritableDrive,
    /// 文件不存在
    NotFound(String),
    /// 子串没找到（str_replace 用）
    SubstringNotFound,
    /// 子串出现多次（要求精确替换，模糊时拒绝）
    SubstringAmbiguous(usize),
    /// 行号越界（insert 用）
    LineOutOfRange { given: usize, total: usize },
    /// 其它 I/O 错误
    IoError(String),
    /// 参数非法
    InvalidArgument(String),
}

impl MemoryError {
    pub fn display_for_model(&self) -> String {
        match self {
            MemoryError::PathTraversal(p) => {
                format!("路径不安全（拒绝越界 memory root）：{p}")
            }
            MemoryError::NoWritableDrive => {
                "找不到 memory root 且没可写盘（PE 的 X: 盘是 ramdisk，不能持久化；\
                 请插 U 盘）"
                    .to_owned()
            }
            MemoryError::NotFound(p) => format!("文件不存在：{p}"),
            MemoryError::SubstringNotFound => "要替换的 old_str 在文件里没找到".to_owned(),
            MemoryError::SubstringAmbiguous(n) => format!(
                "要替换的 old_str 在文件里出现 {n} 次（要求精确，请提供更长的 old_str 缩小范围）"
            ),
            MemoryError::LineOutOfRange { given, total } => {
                format!("insert_line {given} 越界（文件共 {total} 行；合法范围 0..={total}）")
            }
            MemoryError::IoError(s) => format!("I/O 错误：{s}"),
            MemoryError::InvalidArgument(s) => format!("参数非法：{s}"),
        }
    }
}

/// path traversal 防护核心：把用户输入的相对路径拼到 root 下并校验仍在 root 内。
///
/// 不要求文件存在（与 canonicalize 不同）—— create 场景需要先建路径。
/// 拒绝条件：
/// - 输入为空
/// - 输入是绝对路径（含 Windows `C:\` / UNC `\\?\` / `/`）
/// - 输入含 `..` 组件
/// - 输入含 NUL 字节
/// - 规范化后路径不以 root 开头（防御性兜底）
pub fn resolve_inside_root(root: &Path, user_input: &str) -> Result<PathBuf, MemoryError> {
    let trimmed = user_input.trim();
    if trimmed.is_empty() {
        return Err(MemoryError::InvalidArgument("路径为空".into()));
    }
    if trimmed.contains('\0') {
        return Err(MemoryError::PathTraversal(user_input.into()));
    }
    let p = Path::new(trimmed);
    if p.is_absolute() {
        return Err(MemoryError::PathTraversal(user_input.into()));
    }
    // 显式拒绝任何 `..` / `.\` 之外的特殊组件
    for comp in p.components() {
        match comp {
            Component::ParentDir => return Err(MemoryError::PathTraversal(user_input.into())),
            Component::Prefix(_) => return Err(MemoryError::PathTraversal(user_input.into())),
            Component::RootDir => return Err(MemoryError::PathTraversal(user_input.into())),
            Component::Normal(_) | Component::CurDir => {}
        }
    }
    let joined = root.join(p);

    // 二次防御：normalize（去 .）后仍须以 root 开头
    let normalized = normalize_components(&joined);
    let root_norm = normalize_components(root);
    if !normalized.starts_with(&root_norm) {
        return Err(MemoryError::PathTraversal(user_input.into()));
    }
    Ok(normalized)
}

/// 弱化版规范化 —— 只去掉 `.` 组件；保留 prefix / root；不解软链；不要求存在。
fn normalize_components(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 1. view：文件 → 全文；目录 → 子项列表（每行 1 个名字 + `/` 标记目录）。
pub fn view(root: &Path, rel: &str) -> Result<String, MemoryError> {
    let path = resolve_inside_root(root, rel)?;
    if !path.exists() {
        return Err(MemoryError::NotFound(rel.to_owned()));
    }
    if path.is_dir() {
        let mut entries: Vec<String> = std::fs::read_dir(&path)
            .map_err(|e| MemoryError::IoError(e.to_string()))?
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    format!("{name}/")
                } else {
                    name
                }
            })
            .collect();
        entries.sort();
        Ok(entries.join("\n"))
    } else {
        std::fs::read_to_string(&path).map_err(|e| MemoryError::IoError(e.to_string()))
    }
}

/// 2. create：写文件（含覆盖）。父目录自动建。
pub fn create(root: &Path, rel: &str, content: &str) -> Result<(), MemoryError> {
    let path = resolve_inside_root(root, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MemoryError::IoError(e.to_string()))?;
    }
    std::fs::write(&path, content).map_err(|e| MemoryError::IoError(e.to_string()))?;
    Ok(())
}

/// 3. str_replace：替换 old_str 第 1 次出现为 new_str。
///
/// 行为：
/// - old_str 在文件里**未出现** → `SubstringNotFound`
/// - 出现 ≥ 2 次 → `SubstringAmbiguous(n)`（强制要求 caller 拉长 old_str 区分）
/// - 出现 1 次 → 替换并写回
pub fn str_replace(
    root: &Path,
    rel: &str,
    old_str: &str,
    new_str: &str,
) -> Result<(), MemoryError> {
    if old_str.is_empty() {
        return Err(MemoryError::InvalidArgument("old_str 不能为空".into()));
    }
    let path = resolve_inside_root(root, rel)?;
    if !path.exists() {
        return Err(MemoryError::NotFound(rel.to_owned()));
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| MemoryError::IoError(e.to_string()))?;
    let count = content.matches(old_str).count();
    match count {
        0 => Err(MemoryError::SubstringNotFound),
        1 => {
            let new_content = content.replacen(old_str, new_str, 1);
            std::fs::write(&path, new_content)
                .map_err(|e| MemoryError::IoError(e.to_string()))?;
            Ok(())
        }
        n => Err(MemoryError::SubstringAmbiguous(n)),
    }
}

/// 4. insert：在第 `insert_line` 行之前插入 `new_lines`（可含多行）。
///
/// `insert_line=0` = 文件最前；`insert_line=N`（N=原行数）= 文件末尾追加。
pub fn insert(
    root: &Path,
    rel: &str,
    insert_line: usize,
    new_lines: &str,
) -> Result<(), MemoryError> {
    let path = resolve_inside_root(root, rel)?;
    if !path.exists() {
        return Err(MemoryError::NotFound(rel.to_owned()));
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| MemoryError::IoError(e.to_string()))?;
    let lines: Vec<&str> = content.lines().collect();
    if insert_line > lines.len() {
        return Err(MemoryError::LineOutOfRange {
            given: insert_line,
            total: lines.len(),
        });
    }
    let mut out = Vec::with_capacity(lines.len() + 1);
    out.extend_from_slice(&lines[..insert_line]);
    for nl in new_lines.lines() {
        out.push(nl);
    }
    out.extend_from_slice(&lines[insert_line..]);
    let mut new_content = out.join("\n");
    // 保留原结尾换行符（如果原内容有）
    if content.ends_with('\n') && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    std::fs::write(&path, new_content).map_err(|e| MemoryError::IoError(e.to_string()))?;
    Ok(())
}

/// 5. delete：删除文件（不递归删目录 —— 防误操作）。
pub fn delete(root: &Path, rel: &str) -> Result<(), MemoryError> {
    let path = resolve_inside_root(root, rel)?;
    if !path.exists() {
        return Err(MemoryError::NotFound(rel.to_owned()));
    }
    if path.is_dir() {
        return Err(MemoryError::InvalidArgument(
            "delete 拒绝删目录 —— 请逐个 delete 文件后再手工清理".into(),
        ));
    }
    std::fs::remove_file(&path).map_err(|e| MemoryError::IoError(e.to_string()))?;
    Ok(())
}

/// 6. rename：改名 / 移动。源和目标都必须在 root 内。
pub fn rename(root: &Path, old_rel: &str, new_rel: &str) -> Result<(), MemoryError> {
    let src = resolve_inside_root(root, old_rel)?;
    let dst = resolve_inside_root(root, new_rel)?;
    if !src.exists() {
        return Err(MemoryError::NotFound(old_rel.to_owned()));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MemoryError::IoError(e.to_string()))?;
    }
    std::fs::rename(&src, &dst).map_err(|e| MemoryError::IoError(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_root() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    // ---- path traversal guards ----

    #[test]
    fn rejects_absolute_path_input() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), r"C:\Windows\System32\cmd.exe");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn rejects_dotdot_traversal() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), r"..\..\Windows\System32\drivers\etc\hosts");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn rejects_unix_root_input() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), "/etc/passwd");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn rejects_unc_input() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), r"\\?\C:\foo");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn rejects_nul_byte() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), "foo\0bar.md");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn rejects_empty_path() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), "   ");
        assert!(matches!(r, Err(MemoryError::InvalidArgument(_))), "{r:?}");
    }

    #[test]
    fn accepts_simple_relative() {
        let tmp = make_test_root();
        let r = resolve_inside_root(tmp.path(), "MEMORY.md").expect("should accept");
        assert!(r.starts_with(tmp.path()));
        assert!(r.ends_with("MEMORY.md"));
    }

    #[test]
    fn accepts_subdir_relative() {
        let tmp = make_test_root();
        let r =
            resolve_inside_root(tmp.path(), r"projects\foo.md").expect("subdir should accept");
        assert!(r.starts_with(tmp.path()));
    }

    // ---- 6 commands ----

    #[test]
    fn create_and_view_roundtrip() {
        let tmp = make_test_root();
        create(tmp.path(), "MEMORY.md", "Hello\nWorld\n").unwrap();
        let v = view(tmp.path(), "MEMORY.md").unwrap();
        assert_eq!(v, "Hello\nWorld\n");
    }

    #[test]
    fn view_directory_lists_entries() {
        let tmp = make_test_root();
        create(tmp.path(), "a.md", "x").unwrap();
        create(tmp.path(), "b.md", "y").unwrap();
        let v = view(tmp.path(), ".").unwrap();
        let mut lines: Vec<&str> = v.lines().collect();
        lines.sort();
        assert!(lines.iter().any(|l| *l == "a.md"));
        assert!(lines.iter().any(|l| *l == "b.md"));
    }

    #[test]
    fn view_missing_file_returns_not_found() {
        let tmp = make_test_root();
        let r = view(tmp.path(), "nope.md");
        assert!(matches!(r, Err(MemoryError::NotFound(_))), "{r:?}");
    }

    #[test]
    fn str_replace_unique_substring() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "foo BAR baz\n").unwrap();
        str_replace(tmp.path(), "m.md", "BAR", "QUX").unwrap();
        let v = view(tmp.path(), "m.md").unwrap();
        assert_eq!(v, "foo QUX baz\n");
    }

    #[test]
    fn str_replace_missing_substring_errors() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "foo\n").unwrap();
        let r = str_replace(tmp.path(), "m.md", "missing", "x");
        assert!(matches!(r, Err(MemoryError::SubstringNotFound)), "{r:?}");
    }

    #[test]
    fn str_replace_ambiguous_substring_errors() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "foo foo foo").unwrap();
        let r = str_replace(tmp.path(), "m.md", "foo", "bar");
        assert!(matches!(r, Err(MemoryError::SubstringAmbiguous(3))), "{r:?}");
    }

    #[test]
    fn insert_at_zero_prepends() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "line2\nline3\n").unwrap();
        insert(tmp.path(), "m.md", 0, "line1").unwrap();
        let v = view(tmp.path(), "m.md").unwrap();
        assert_eq!(v, "line1\nline2\nline3\n");
    }

    #[test]
    fn insert_at_end_appends() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "a\nb\n").unwrap();
        insert(tmp.path(), "m.md", 2, "c").unwrap();
        let v = view(tmp.path(), "m.md").unwrap();
        assert_eq!(v, "a\nb\nc\n");
    }

    #[test]
    fn insert_out_of_range_errors() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "a\nb\n").unwrap();
        let r = insert(tmp.path(), "m.md", 99, "x");
        assert!(
            matches!(r, Err(MemoryError::LineOutOfRange { given: 99, total: 2 })),
            "{r:?}"
        );
    }

    #[test]
    fn delete_removes_file() {
        let tmp = make_test_root();
        create(tmp.path(), "m.md", "x").unwrap();
        delete(tmp.path(), "m.md").unwrap();
        let r = view(tmp.path(), "m.md");
        assert!(matches!(r, Err(MemoryError::NotFound(_))));
    }

    #[test]
    fn delete_refuses_directory() {
        let tmp = make_test_root();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();
        let r = delete(tmp.path(), "subdir");
        assert!(matches!(r, Err(MemoryError::InvalidArgument(_))), "{r:?}");
    }

    #[test]
    fn rename_moves_file() {
        let tmp = make_test_root();
        create(tmp.path(), "old.md", "content").unwrap();
        rename(tmp.path(), "old.md", "new.md").unwrap();
        assert_eq!(view(tmp.path(), "new.md").unwrap(), "content");
        assert!(matches!(
            view(tmp.path(), "old.md"),
            Err(MemoryError::NotFound(_))
        ));
    }

    #[test]
    fn rename_to_traversal_target_blocked() {
        let tmp = make_test_root();
        create(tmp.path(), "src.md", "x").unwrap();
        let r = rename(tmp.path(), "src.md", r"..\escape.md");
        assert!(matches!(r, Err(MemoryError::PathTraversal(_))), "{r:?}");
    }

    #[test]
    fn create_in_subdir_creates_parent_dirs() {
        let tmp = make_test_root();
        create(tmp.path(), r"projects\foo\bar.md", "hi").unwrap();
        assert_eq!(view(tmp.path(), r"projects\foo\bar.md").unwrap(), "hi");
    }
}

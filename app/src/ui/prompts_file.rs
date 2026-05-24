//! U 盘 / 数据分区 `prompts.txt` 加载 —— 用户预编辑好的中文候选问题。
//!
//! 阶段 v1.0.1 新增：U 盘真测发现 PE 无 IME，中文输入要靠应用层兜底。
//! 用户在主系统里把常用问题敲到 `<U>\NeuroBoot.prompts.txt`，PE 启动时扫到 → UI 下拉框选用。
//!
//! 格式：每非空行是一个候选问题；行首 `#` 是注释。
//!
//! 路径搜索：所有非 X: 盘符的 `<root>\NeuroBoot.prompts.txt` 或 `<root>\NeuroBoot\prompts.txt`。
//! 找到第一个就停（不合并多个 U 盘）。

use std::path::PathBuf;

/// 单条用户预设问题的元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrompt {
    /// 来源标签（如 "D#3" 表示 D 盘的第 3 行）—— UI 下拉框前缀显示用
    pub label: String,
    /// 完整问题文本
    pub text: String,
}

/// 扫所有非 X: 盘根，找到第一个有效 prompts 文件并解析。空 vec 表示没找到。
pub fn scan_user_prompts() -> Vec<UserPrompt> {
    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        for filename in &["NeuroBoot.prompts.txt", "NeuroBoot\\prompts.txt"] {
            let path = PathBuf::from(format!("{c}:\\{filename}"));
            if let Ok(content) = std::fs::read_to_string(&path) {
                let prompts = parse_prompts(&content, c);
                if !prompts.is_empty() {
                    return prompts;
                }
            }
        }
    }
    Vec::new()
}

/// 解析 prompts.txt 内容：每非空非注释行一个 prompt，label 格式 `<drive>#<line>`。
///
/// 提取出来给单测用 —— 不依赖文件系统。
pub fn parse_prompts(content: &str, drive_letter: char) -> Vec<UserPrompt> {
    let mut out = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(UserPrompt {
            label: format!("{drive_letter}#{}", idx + 1),
            text: line.to_owned(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_prompts() {
        let content = "\
# 这是注释
我的电脑蓝屏了

# 下面是另一个问题
D 盘 RAW 怎么恢复？
";
        let prompts = parse_prompts(content, 'E');
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].label, "E#2");
        assert_eq!(prompts[0].text, "我的电脑蓝屏了");
        assert_eq!(prompts[1].label, "E#5");
        assert_eq!(prompts[1].text, "D 盘 RAW 怎么恢复？");
    }

    #[test]
    fn ignores_all_blanks_and_comments() {
        let prompts = parse_prompts("\n\n# only comments\n  # also\n", 'F');
        assert!(prompts.is_empty());
    }

    #[test]
    fn trims_whitespace() {
        let prompts = parse_prompts("   有空格的问题   \n", 'G');
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].text, "有空格的问题");
    }
}

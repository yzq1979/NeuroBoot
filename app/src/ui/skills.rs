//! Skill 系统 —— v2 Stage 7.2。
//!
//! 每个 skill 是一份 markdown 文件，提供「诊断剧本」的预设 prompt 增量。
//! 启动时扫两个目录加载：
//! - `X:\NeuroBoot\skills\*.md`（PE 内置）
//! - `C:\NeuroBoot\skills\*.md`（开发机 / 用户 U 盘挂在 C:\NeuroBoot 时的备份位）
//!
//! 文件格式约定：
//! - **第一行**：`# /skill-name` —— 必须，skill 标识符（UI 显示用，模型不见）
//! - **可选第二行**：`> short description` —— blockquote 一句话描述（UI tooltip）
//! - **正文**：markdown 任意内容，被注入到下一轮 system prompt 作为附加段
//!
//! 比 Claude Code 的 skill 简化得多：没 file watcher / 没 file globbing / 没 progressive disclosure；
//! PE 单进程救援场景不需要那些复杂度。

use std::path::PathBuf;

/// 单个 skill 元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// 显示名（含开头的 `/`）—— UI 下拉框文本
    pub name: String,
    /// 一句话描述（UI tooltip；空字符串 = 没写）
    pub description: String,
    /// skill prompt body —— 注入到 system prompt 末尾的增量
    pub body: String,
    /// 来源路径（debug / UI 显示用）
    pub source_path: PathBuf,
}

/// 扫所有候选目录、加载所有 .md skill。
pub fn scan_skills() -> Vec<Skill> {
    let mut out = Vec::new();
    let dirs = [
        PathBuf::from("X:\\NeuroBoot\\skills"),
        PathBuf::from("C:\\NeuroBoot\\skills"),
    ];
    for dir in dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue, // 目录不存在/不可读 —— 跳过
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(skill) = parse_skill(&content, path.clone()) {
                out.push(skill);
            }
        }
    }
    out
}

/// 解析 skill markdown。返回 None 表示文件格式不对。
///
/// **v3 Quick Win 4** —— 支持两种格式：
///
/// **新格式（推荐，YAML frontmatter，对齐 Claude Code SKILL.md spec）**：
/// ```markdown
/// ---
/// name: /diagnose-bsod
/// description: 用户报告蓝屏后，按此剧本走
/// ---
/// 当用户报告蓝屏时...
/// （body）
/// ```
///
/// **旧格式（v2 Stage 7.2，向后兼容）**：
/// ```markdown
/// # /diagnose-bsod
/// > 用户报告蓝屏后，按此剧本走
///
/// 当用户报告蓝屏时...
/// ```
///
/// 自动检测：首行 `---` → YAML frontmatter；否则按旧 `# /name` + `> desc` 解析。
pub fn parse_skill(content: &str, source_path: PathBuf) -> Option<Skill> {
    let trimmed = content.trim_start();
    if trimmed.starts_with("---") {
        parse_yaml_frontmatter(trimmed, source_path)
    } else {
        parse_legacy_format(content, source_path)
    }
}

/// 解析新版 YAML frontmatter 格式。
fn parse_yaml_frontmatter(content: &str, source_path: PathBuf) -> Option<Skill> {
    // 找 `---` ... `---` 之间的 frontmatter，剩下是 body
    let mut lines = content.lines();
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }

    let mut frontmatter = String::new();
    let mut found_closing = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    if !found_closing {
        return None;
    }

    // 简化 YAML parser：只支持 `key: value` 形式（不引入 serde_yaml 依赖）
    let mut name = String::new();
    let mut description = String::new();
    for fm_line in frontmatter.lines() {
        let trimmed = fm_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            match key {
                "name" => name = value.to_owned(),
                "description" => description = value.to_owned(),
                _ => {} // 未知 key 忽略
            }
        }
    }

    if name.is_empty() || !name.starts_with('/') {
        return None; // 必须有 name 且以 / 开头
    }

    let body: String = lines.collect::<Vec<_>>().join("\n");

    Some(Skill {
        name,
        description,
        body: body.trim().to_owned(),
        source_path,
    })
}

/// v2 Stage 7.2 老格式解析（向后兼容）。
fn parse_legacy_format(content: &str, source_path: PathBuf) -> Option<Skill> {
    let mut lines = content.lines();
    let first = lines.next()?.trim();
    if !first.starts_with("# /") {
        return None;
    }
    let name = first.trim_start_matches('#').trim().to_owned();
    if name.is_empty() {
        return None;
    }

    let mut description = String::new();
    let mut body_start_lookahead: Option<&str> = None;
    if let Some(second) = lines.next() {
        let t = second.trim();
        if let Some(desc) = t.strip_prefix('>') {
            description = desc.trim().to_owned();
        } else {
            body_start_lookahead = Some(second);
        }
    }

    let mut body = String::new();
    if let Some(first_body_line) = body_start_lookahead {
        body.push_str(first_body_line);
        body.push('\n');
    }
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }

    Some(Skill {
        name,
        description,
        body: body.trim().to_owned(),
        source_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_skill() {
        let md = "\
# /diagnose-bsod
> 用户报告蓝屏后，按此剧本走

When the user reports a blue screen:
1. Call read_event_log_errors with hours=72
2. Call list_minidumps
3. Call list_recent_shutdowns
";
        let skill = parse_skill(md, PathBuf::from("test.md")).unwrap();
        assert_eq!(skill.name, "/diagnose-bsod");
        assert_eq!(skill.description, "用户报告蓝屏后，按此剧本走");
        assert!(skill.body.contains("read_event_log_errors"));
        assert!(skill.body.contains("list_recent_shutdowns"));
    }

    #[test]
    fn parse_skill_without_description() {
        let md = "\
# /test
Body starts immediately
more body
";
        let skill = parse_skill(md, PathBuf::from("t.md")).unwrap();
        assert_eq!(skill.name, "/test");
        assert_eq!(skill.description, "");
        assert!(skill.body.starts_with("Body starts"));
        assert!(skill.body.contains("more body"));
    }

    #[test]
    fn parse_skill_rejects_no_slash_header() {
        let md = "# diagnose-bsod\nbody";
        assert!(parse_skill(md, PathBuf::from("t.md")).is_none());
    }

    #[test]
    fn parse_skill_rejects_no_h1() {
        let md = "diagnose-bsod\nbody";
        assert!(parse_skill(md, PathBuf::from("t.md")).is_none());
    }

    // v3 Quick Win 4: YAML frontmatter 解析测试
    #[test]
    fn parse_yaml_frontmatter_full() {
        let md = "---\nname: /diagnose-bsod\ndescription: 用户报告蓝屏后剧本\n---\n\n当用户报告蓝屏时：\n1. 查 event log\n2. 查 dump\n";
        let skill = parse_skill(md, PathBuf::from("test.md")).unwrap();
        assert_eq!(skill.name, "/diagnose-bsod");
        assert_eq!(skill.description, "用户报告蓝屏后剧本");
        assert!(skill.body.contains("查 event log"));
        assert!(skill.body.contains("查 dump"));
    }

    #[test]
    fn parse_yaml_frontmatter_with_quotes() {
        let md = "---\nname: \"/test\"\ndescription: 'a quoted description'\n---\nbody";
        let skill = parse_skill(md, PathBuf::from("t.md")).unwrap();
        assert_eq!(skill.name, "/test");
        assert_eq!(skill.description, "a quoted description");
    }

    #[test]
    fn parse_yaml_frontmatter_missing_close_rejected() {
        let md = "---\nname: /test\n# missing close ---\nbody";
        assert!(parse_skill(md, PathBuf::from("t.md")).is_none());
    }

    #[test]
    fn parse_yaml_frontmatter_name_must_start_with_slash() {
        let md = "---\nname: test\n---\nbody";
        assert!(parse_skill(md, PathBuf::from("t.md")).is_none());
    }
}

//! Skill 系统 —— v2 Stage 7.2 起，v3.0 W1.5 升级为 progressive disclosure。
//!
//! 每个 skill 是一份 markdown 文件，提供「诊断剧本」的预设 prompt 增量。
//! 启动时扫两个目录加载：
//! - `X:\NeuroBoot\skills\*.md`（PE 内置）
//! - `C:\NeuroBoot\skills\*.md`（开发机 / 用户 U 盘挂在 C:\NeuroBoot 时的备份位）
//!
//! 文件格式约定（支持新 / 旧两种 —— 见 `parse_skill`）。
//!
//! ## v3.0 W1.5：Progressive Disclosure
//!
//! Anthropic 2025-12 开放标准，OpenAI / Google / GitHub / Cursor 几周内全部接入。
//! 三层模型：
//! - **Tier 1（启动加载）**：仅 frontmatter（`name` + `description`），~80 tokens/skill。
//!   全部 skill summary 注入 system prompt，AI 总能看到「有哪些剧本可用」
//! - **Tier 2（AI 触发加载）**：AI 判断某 skill 相关时，调 `load_skill(name)` 工具
//!   读完整 body markdown。返回结果作为 tool_result 进入下轮 context
//! - **Tier 3（按需加载）**：body 内 `@reference.md` 引用。v3.0 暂作文档约定（AI
//!   通过 `read_file` 工具按需读，等 v3.1 加 `load_skill_reference` 显式工具）
//!
//! 收益（Anthropic 公布数据）：60~80% token 节省 + 准确度提升 —— 启动时全部 skill
//! body 不再占 context，AI 按需调用，body 只在相关 turn 出现。
//!
//! 用户手动下拉激活（v2 Stage 7.2 UX）保持向后兼容 —— 不影响 AI 自主调用 load_skill。

use std::path::PathBuf;

/// Skill summary（Tier 1，启动加载，~80 tokens/skill）—— v3.0 W1.5。
///
/// 只含 frontmatter 的 name + description + 源路径。
/// 全部 SkillSummary 注入 system prompt，AI 据此判断何时调 `load_skill`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    /// 显示名（含开头的 `/`）—— UI 下拉框文本 + load_skill 工具的 name 参数
    pub name: String,
    /// 一句话描述（UI tooltip + AI 触发判断依据；空字符串 = 没写）
    pub description: String,
    /// 来源路径（debug / UI 显示用）
    pub source_path: PathBuf,
}

/// Skill body（Tier 2，按需加载）—— v3.0 W1.5。
///
/// 由 [`load_skill_body`] 在需要时（AI 调 load_skill 工具 或 用户手动激活）读取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillBody {
    /// 显示名（同 SkillSummary.name）
    pub name: String,
    /// 一句话描述
    pub description: String,
    /// 完整 markdown body —— 注入到 system prompt（手动模式）或 tool_result（AI 模式）
    pub body: String,
    /// 来源路径
    pub source_path: PathBuf,
}

/// 候选 skill 目录（按优先级；前面优先）。
///
/// v3.0：抽出常量便于 `scan_skills` 和 `load_skill_body` 共用。
pub fn skill_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"X:\NeuroBoot\skills"),
        PathBuf::from(r"C:\NeuroBoot\skills"),
    ]
}

/// 扫所有候选目录、加载所有 .md skill 的 **summary（Tier 1）**。
///
/// v3.0 W1.5：返回类型从 `Vec<Skill>` 改为 `Vec<SkillSummary>` —— 不再保留 body，
/// 节省启动 context。Body 通过 [`load_skill_body`] 按需获取。
pub fn scan_skills() -> Vec<SkillSummary> {
    let mut out = Vec::new();
    for dir in skill_dirs() {
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
            if let Some(body) = parse_skill(&content, path.clone()) {
                // 丢弃 body，只保留 summary
                out.push(SkillSummary {
                    name: body.name,
                    description: body.description,
                    source_path: body.source_path,
                });
            }
        }
    }
    out
}

/// 按 name 加载 skill body（Tier 2）—— v3.0 W1.5。
///
/// 在所有候选 [`skill_dirs`] 中找名为 `<name>` 的 skill 并完整解析返回。
/// 优先级：X: 盘（PE 内置）优先于 C: 盘（开发机）；目录内按文件系统枚举顺序。
///
/// 返回 None：未找到匹配 name 的 skill / 找到但解析失败。
///
/// **性能**：每次 I/O 重扫，对小 markdown 文件可忽略（< 1 ms）。频繁调用可考虑外层缓存。
pub fn load_skill_body(name: &str) -> Option<SkillBody> {
    for dir in skill_dirs() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(&path).ok()?;
            if let Some(body) = parse_skill(&content, path.clone()) {
                if body.name == name {
                    return Some(body);
                }
            }
        }
    }
    None
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
pub fn parse_skill(content: &str, source_path: PathBuf) -> Option<SkillBody> {
    let trimmed = content.trim_start();
    if trimmed.starts_with("---") {
        parse_yaml_frontmatter(trimmed, source_path)
    } else {
        parse_legacy_format(content, source_path)
    }
}

/// 解析新版 YAML frontmatter 格式。
fn parse_yaml_frontmatter(content: &str, source_path: PathBuf) -> Option<SkillBody> {
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

    Some(SkillBody {
        name,
        description,
        body: body.trim().to_owned(),
        source_path,
    })
}

/// v2 Stage 7.2 老格式解析（向后兼容）。
fn parse_legacy_format(content: &str, source_path: PathBuf) -> Option<SkillBody> {
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

    Some(SkillBody {
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

    // v3.0 W1.5 Progressive Disclosure tests

    #[test]
    fn skill_summary_does_not_carry_body() {
        // 模拟 scan_skills 行为：解析完整 body 后只保留 summary 字段
        let md = "---\nname: /test\ndescription: A test skill\n---\nThis is a long body that should NOT be in summary.";
        let body = parse_skill(md, PathBuf::from("t.md")).unwrap();
        let summary = SkillSummary {
            name: body.name.clone(),
            description: body.description.clone(),
            source_path: body.source_path.clone(),
        };
        // Summary 没 body 字段 —— 类型系统保证
        assert_eq!(summary.name, "/test");
        assert_eq!(summary.description, "A test skill");
        // body 仍可单独取
        assert!(body.body.contains("This is a long body"));
    }

    #[test]
    fn skill_body_struct_keeps_full_content() {
        // SkillBody 是 Tier 2 完整内容，包含 body 字段
        let md = "---\nname: /diag\ndescription: Diagnostic\n---\nStep 1: foo\nStep 2: bar";
        let body = parse_skill(md, PathBuf::from("d.md")).unwrap();
        assert_eq!(body.name, "/diag");
        assert_eq!(body.description, "Diagnostic");
        assert!(body.body.contains("Step 1: foo"));
        assert!(body.body.contains("Step 2: bar"));
    }

    #[test]
    fn legacy_format_still_returns_skill_body() {
        // 向后兼容：旧 `# /name` 格式仍能 parse 出 SkillBody
        let md = "# /old-style\n> A legacy skill\n\nBody line 1\nBody line 2";
        let body = parse_skill(md, PathBuf::from("old.md")).unwrap();
        assert_eq!(body.name, "/old-style");
        assert_eq!(body.description, "A legacy skill");
        assert!(body.body.contains("Body line 1"));
    }

    // v3.0 W2-3：验证仓库里的 distributed skill 模板全部能 parse + 元数据完整。
    // 测试时 cwd 是 cargo workspace root（即 app/），通过 ../ 到 docs/usb-templates/skills。
    #[test]
    fn distributed_skill_templates_all_parse() {
        let skills_dir = std::path::Path::new("../docs/usb-templates/skills");
        if !skills_dir.exists() {
            // 测试可能在不同工作目录跑（IDE / CI），找不到就跳
            eprintln!(
                "skip: skills dir not at {} (cwd={:?})",
                skills_dir.display(),
                std::env::current_dir().ok()
            );
            return;
        }
        let entries = std::fs::read_dir(skills_dir).expect("read skills dir");
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            let parsed = parse_skill(&content, path.clone());
            assert!(
                parsed.is_some(),
                "skill file {} failed to parse",
                path.display()
            );
            let skill = parsed.unwrap();
            assert!(
                skill.name.starts_with('/'),
                "{}: name '{}' must start with /",
                path.display(),
                skill.name
            );
            assert!(
                !skill.description.is_empty(),
                "{}: description must not be empty (W2-3 convention)",
                path.display()
            );
            // body 至少 100 字符 —— 防 skill 内容退化为空壳
            assert!(
                skill.body.chars().count() >= 100,
                "{}: body too short ({} chars) - real skill should have substantive content",
                path.display(),
                skill.body.chars().count()
            );
            found.push(skill.name);
        }
        // v3.0 W2-3 完成后预期至少 8 个 skill 模板
        assert!(
            found.len() >= 5,
            "expected >= 5 distributed skill templates, got {}: {:?}",
            found.len(),
            found
        );
        eprintln!("[OK] {} skill templates parsed: {:?}", found.len(), found);
    }
}

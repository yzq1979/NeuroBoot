//! YAML schema + loader for golden-prompt cases.
//!
//! Each `*.yaml` file in `app/tests/eval-fixtures/` decodes into one [`EvalCase`].
//! See the directory's `README.md` (not shipped, in the test fixtures dir)
//! for the canonical example.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One golden test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// Stable identifier (kebab-case). Used in reports.
    pub id: String,
    /// Free-text category for grouping: "skill" / "free-form" / "multi-step"
    /// / "dangerous" / "edge-case". Used in reports.
    pub category: String,
    /// Human-readable summary of what we're testing.
    pub description: String,
    /// What the user says to the agent.
    pub input: Input,
    /// What we expect.
    pub expected: Expected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    /// The user message text. Single-turn only for v3.0.
    pub user: String,
}

/// Expectations against the agent's full execution (tool calls + response).
///
/// All fields are optional — empty / default means "no check on this axis".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Expected {
    /// Tools that MUST appear in the agent's tool_calls list (any order,
    /// subset match — extra tools are OK).
    #[serde(default)]
    pub must_call_tools: Vec<String>,
    /// Tools that MUST NOT appear. Hits trigger a failure.
    #[serde(default)]
    pub must_not_call: Vec<String>,
    /// Regex patterns the final assistant text must match (case-insensitive).
    #[serde(default)]
    pub must_match: Vec<String>,
    /// Regex patterns that, if matched, fail the case (hallucination /
    /// over-claiming / fake drive letters).
    #[serde(default)]
    pub must_not_match: Vec<String>,
    /// Performance budget — wall clock. Defaults are loose.
    #[serde(default = "default_max_seconds")]
    pub max_response_seconds: u64,
    /// Performance budget — token estimate. 0 = unbounded.
    #[serde(default)]
    pub max_tokens: usize,
    /// Optional list of tools the agent is allowed to use (any tool name
    /// outside this list is treated as a hallucinated / unknown tool).
    /// Empty Vec means "no allowlist filter".
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

fn default_max_seconds() -> u64 {
    180
}

/// What can go wrong loading a YAML file.
#[derive(Debug)]
pub enum LoadError {
    Io(String, std::io::Error),
    Parse(String, String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(path, e) => write!(f, "I/O reading {path}: {e}"),
            LoadError::Parse(path, e) => write!(f, "parse error in {path}: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Read every `*.yaml` / `*.yml` in `dir` (non-recursive) and parse it into
/// an [`EvalCase`]. Cases are returned sorted by `id` so reports are stable.
pub fn load_cases_dir(dir: &Path) -> Result<Vec<EvalCase>, LoadError> {
    let mut files: Vec<PathBuf> = Vec::new();
    let entries =
        fs::read_dir(dir).map_err(|e| LoadError::Io(dir.display().to_string(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml" | "yml")
        ) {
            files.push(path);
        }
    }
    files.sort();
    let mut cases = Vec::with_capacity(files.len());
    for path in files {
        let content =
            fs::read_to_string(&path).map_err(|e| LoadError::Io(path.display().to_string(), e))?;
        let case: EvalCase = serde_yaml::from_str(&content)
            .map_err(|e| LoadError::Parse(path.display().to_string(), e.to_string()))?;
        cases.push(case);
    }
    cases.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_yaml() {
        let yaml = r#"
id: my-case
category: skill
description: trivial
input:
  user: "你好"
expected: {}
"#;
        let case: EvalCase = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(case.id, "my-case");
        assert_eq!(case.input.user, "你好");
        assert!(case.expected.must_call_tools.is_empty());
        assert_eq!(case.expected.max_response_seconds, 180);
    }

    #[test]
    fn parses_full_yaml() {
        let yaml = r#"
id: full-case
category: multi-step
description: BSOD diagnosis flow
input:
  user: "我的电脑昨天蓝屏了"
expected:
  must_call_tools:
    - read_event_log_errors
    - list_minidumps
  must_not_call:
    - delete_path
  must_match:
    - "蓝屏"
    - "(event|dump|minidump)"
  must_not_match:
    - "已修复"
    - "Y:\\"
  max_response_seconds: 60
  max_tokens: 1500
  allowed_tools:
    - read_event_log_errors
    - list_minidumps
    - list_recent_shutdowns
"#;
        let case: EvalCase = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(case.expected.must_call_tools.len(), 2);
        assert_eq!(case.expected.must_not_call, vec!["delete_path"]);
        assert_eq!(case.expected.max_response_seconds, 60);
        assert_eq!(case.expected.max_tokens, 1500);
        assert_eq!(case.expected.allowed_tools.len(), 3);
    }

    #[test]
    fn load_cases_dir_handles_missing() {
        let r = load_cases_dir(Path::new("Z:\\definitely_not_there"));
        assert!(r.is_err());
    }

    #[test]
    fn load_cases_dir_sorts_by_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::write(
            tmp.path().join("b.yaml"),
            "id: zebra\ncategory: x\ndescription: x\ninput:\n  user: x\nexpected: {}\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("a.yaml"),
            "id: alpha\ncategory: x\ndescription: x\ninput:\n  user: x\nexpected: {}\n",
        )
        .unwrap();
        // Non-YAML file should be ignored.
        fs::write(tmp.path().join("readme.md"), "ignore me").unwrap();
        let cases = load_cases_dir(tmp.path()).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].id, "alpha");
        assert_eq!(cases[1].id, "zebra");
    }
}

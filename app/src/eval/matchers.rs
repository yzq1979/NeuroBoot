//! Pure matchers for [`super::spec::Expected`] against an [`ExecutionStats`].
//!
//! Each matcher emits zero or more [`MatchFailure`]s describing exactly what
//! went wrong. Empty `Vec` = case passed.

use regex::Regex;

use super::runner::ExecutionStats;
use super::spec::Expected;

/// One assertion that didn't hold.
#[derive(Debug, Clone)]
pub enum MatchFailure {
    MissingTool(String),
    ForbiddenTool(String),
    UnknownTool(String),
    MissingPattern(String),
    ForbiddenPattern(String),
    TooSlow {
        actual_ms: u128,
        budget_ms: u128,
    },
    TooManyTokens {
        actual: usize,
        budget: usize,
    },
    BadRegex {
        pattern: String,
        error: String,
    },
}

impl std::fmt::Display for MatchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchFailure::MissingTool(t) => write!(f, "expected tool `{t}` not called"),
            MatchFailure::ForbiddenTool(t) => write!(f, "forbidden tool `{t}` was called"),
            MatchFailure::UnknownTool(t) => {
                write!(f, "tool `{t}` is not in the allowed_tools list (typo or hallucinated?)")
            }
            MatchFailure::MissingPattern(p) => write!(f, "response did not match required pattern /{p}/i"),
            MatchFailure::ForbiddenPattern(p) => write!(f, "response matched forbidden pattern /{p}/i"),
            MatchFailure::TooSlow { actual_ms, budget_ms } => {
                write!(f, "response took {actual_ms}ms (budget {budget_ms}ms)")
            }
            MatchFailure::TooManyTokens { actual, budget } => {
                write!(f, "response ~{actual} tokens (budget {budget})")
            }
            MatchFailure::BadRegex { pattern, error } => {
                write!(f, "invalid regex /{pattern}/ in expectations: {error}")
            }
        }
    }
}

/// Apply every matcher in `expected` against the case outcome.
pub fn check_expectations(expected: &Expected, stats: &ExecutionStats) -> Vec<MatchFailure> {
    let mut failures = Vec::new();
    check_tools(expected, stats, &mut failures);
    check_patterns(expected, stats, &mut failures);
    check_budgets(expected, stats, &mut failures);
    failures
}

fn check_tools(expected: &Expected, stats: &ExecutionStats, out: &mut Vec<MatchFailure>) {
    // must_call: every required tool name must appear (any order, subset OK).
    for required in &expected.must_call_tools {
        if !stats.tool_calls.iter().any(|c| &c.name == required) {
            out.push(MatchFailure::MissingTool(required.clone()));
        }
    }
    // must_not_call: zero hits required.
    for forbidden in &expected.must_not_call {
        if stats.tool_calls.iter().any(|c| &c.name == forbidden) {
            out.push(MatchFailure::ForbiddenTool(forbidden.clone()));
        }
    }
    // allowed_tools: any tool outside this list is flagged. Empty list disables this check.
    if !expected.allowed_tools.is_empty() {
        for called in &stats.tool_calls {
            if !expected.allowed_tools.contains(&called.name) {
                out.push(MatchFailure::UnknownTool(called.name.clone()));
            }
        }
    }
}

fn check_patterns(expected: &Expected, stats: &ExecutionStats, out: &mut Vec<MatchFailure>) {
    for pat in &expected.must_match {
        match compile_ci(pat) {
            Ok(re) => {
                if !re.is_match(&stats.final_text) {
                    out.push(MatchFailure::MissingPattern(pat.clone()));
                }
            }
            Err(e) => out.push(MatchFailure::BadRegex {
                pattern: pat.clone(),
                error: e,
            }),
        }
    }
    for pat in &expected.must_not_match {
        match compile_ci(pat) {
            Ok(re) => {
                if re.is_match(&stats.final_text) {
                    out.push(MatchFailure::ForbiddenPattern(pat.clone()));
                }
            }
            Err(e) => out.push(MatchFailure::BadRegex {
                pattern: pat.clone(),
                error: e,
            }),
        }
    }
}

fn check_budgets(expected: &Expected, stats: &ExecutionStats, out: &mut Vec<MatchFailure>) {
    let budget_ms = (expected.max_response_seconds as u128) * 1000;
    if budget_ms > 0 && stats.duration_ms > budget_ms {
        out.push(MatchFailure::TooSlow {
            actual_ms: stats.duration_ms,
            budget_ms,
        });
    }
    if expected.max_tokens > 0 && stats.estimated_tokens > expected.max_tokens {
        out.push(MatchFailure::TooManyTokens {
            actual: stats.estimated_tokens,
            budget: expected.max_tokens,
        });
    }
}

/// Compile a case-insensitive regex. Returns the error message on failure.
fn compile_ci(pattern: &str) -> Result<Regex, String> {
    Regex::new(&format!("(?i){pattern}")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::runner::{ToolCallObserved, ExecutionStats};

    fn make_stats(tools: &[&str], text: &str) -> ExecutionStats {
        ExecutionStats {
            tool_calls: tools
                .iter()
                .map(|n| ToolCallObserved {
                    name: (*n).to_owned(),
                    arguments: "{}".to_owned(),
                })
                .collect(),
            final_text: text.to_owned(),
            duration_ms: 500,
            estimated_tokens: 100,
        }
    }

    #[test]
    fn empty_expected_always_passes() {
        let stats = make_stats(&["list_disks"], "嗯");
        let failures = check_expectations(&Expected::default(), &stats);
        assert!(failures.is_empty());
    }

    #[test]
    fn missing_tool_flagged() {
        let stats = make_stats(&["list_disks"], "");
        let exp = Expected {
            must_call_tools: vec!["analyze_minidump".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::MissingTool(_)));
    }

    #[test]
    fn forbidden_tool_flagged() {
        let stats = make_stats(&["delete_path"], "");
        let exp = Expected {
            must_not_call: vec!["delete_path".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::ForbiddenTool(_)));
    }

    #[test]
    fn unknown_tool_flagged_when_allowlist_set() {
        let stats = make_stats(&["list_disks", "weird_tool"], "");
        let exp = Expected {
            allowed_tools: vec!["list_disks".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::UnknownTool(ref n) if n == "weird_tool"));
    }

    #[test]
    fn missing_pattern_flagged() {
        let stats = make_stats(&[], "回答是 yes");
        let exp = Expected {
            must_match: vec!["蓝屏".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::MissingPattern(_)));
    }

    #[test]
    fn must_match_is_case_insensitive() {
        let stats = make_stats(&[], "INACCESSIBLE_BOOT_DEVICE");
        let exp = Expected {
            must_match: vec!["inaccessible".into()],
            ..Default::default()
        };
        assert!(check_expectations(&exp, &stats).is_empty());
    }

    #[test]
    fn forbidden_pattern_flagged() {
        let stats = make_stats(&[], "我已为您格式化了 C 盘");
        let exp = Expected {
            must_not_match: vec!["已为您格式化".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::ForbiddenPattern(_)));
    }

    #[test]
    fn slow_response_flagged() {
        let mut stats = make_stats(&[], "");
        stats.duration_ms = 70_000;
        let exp = Expected {
            max_response_seconds: 30,
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::TooSlow { .. }));
    }

    #[test]
    fn too_many_tokens_flagged() {
        let mut stats = make_stats(&[], "");
        stats.estimated_tokens = 5_000;
        let exp = Expected {
            max_tokens: 1_000,
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::TooManyTokens { .. }));
    }

    #[test]
    fn bad_regex_flagged_not_panicked() {
        let stats = make_stats(&[], "x");
        let exp = Expected {
            must_match: vec!["[unclosed".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0], MatchFailure::BadRegex { .. }));
    }

    #[test]
    fn multiple_failures_aggregated() {
        let stats = make_stats(&["delete_path"], "回答");
        let exp = Expected {
            must_call_tools: vec!["list_disks".into()],
            must_not_call: vec!["delete_path".into()],
            must_match: vec!["蓝屏".into()],
            ..Default::default()
        };
        let failures = check_expectations(&exp, &stats);
        assert_eq!(failures.len(), 3, "expect 3 failures: missing/forbidden/missing-pattern");
    }
}

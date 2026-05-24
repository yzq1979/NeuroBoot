//! Eval framework — v3.0 W8.
//!
//! Golden-prompt regression tests against the agent. Each case is a YAML
//! file in `app/tests/eval-fixtures/` describing one user prompt plus the
//! expectations (which tools must / must not be called, which patterns
//! must / must not appear in the final response, time / token budgets).
//!
//! ## Scope
//!
//! - **Not** a benchmarking harness. We track wall-clock + an estimated
//!   token count for trend awareness, but the source of truth for pass /
//!   fail is the behavioral expectations.
//! - **Not** a load tester. One prompt at a time, no concurrency.
//! - **Skippable.** If the configured LLM endpoint is unreachable the
//!   integration test reports a [`SkipReason::EndpointUnreachable`] for
//!   each case and exits 0. This way `cargo test --features eval` on CI
//!   without a model never fails for environmental reasons; you only
//!   trip it when you have a model loaded + the agent regresses.
//!
//! ## Design choices
//!
//! - **No promptfoo / DeepEval.** Per Anthropic's eval guide, a thin
//!   in-house runner gives better signal because we control exactly which
//!   AgentEvents we collect; off-the-shelf harnesses assume single
//!   request/response and don't see tool calls.
//! - **Feature-gated.** `cargo` feature `eval` pulls in `regex` and
//!   `serde_yaml`. Release builds are unaffected.
//!
//! ## Module map
//!
//! - [`spec`]: YAML schema (EvalCase / Expected / etc.) and YAML loader
//! - [`matchers`]: pure scoring functions (regex / tool-set / hallucination)
//! - [`runner`]: drives the agent and produces an [`EvalOutcome`]
//!
//! ## Public entry points
//!
//! - [`load_cases_dir`]: read all `*.yaml` in a directory into [`EvalCase`]s.
//! - [`run_cases`]: run a slice of cases against an LLM endpoint, returns
//!   a [`EvalReport`].

pub mod matchers;
pub mod runner;
pub mod spec;

pub use matchers::{check_expectations, MatchFailure};
pub use runner::{run_case, EndpointConfig, EvalOutcome, ExecutionStats, SkipReason};
pub use spec::{load_cases_dir, Expected, EvalCase};

use std::path::Path;
use std::time::Instant;

/// Result of evaluating one case against the agent.
#[derive(Debug, Clone)]
pub enum EvalResult {
    /// Case ran and all expectations passed.
    Passed(ExecutionStats),
    /// Case ran but at least one expectation failed.
    Failed {
        stats: ExecutionStats,
        failures: Vec<MatchFailure>,
    },
    /// Case wasn't run (no LLM, parser error, etc.).
    Skipped(SkipReason),
    /// Infrastructure failure (panic in runner, etc.).
    Error(String),
}

impl EvalResult {
    pub fn is_passing(&self) -> bool {
        matches!(self, EvalResult::Passed(_) | EvalResult::Skipped(_))
    }

    pub fn label(&self) -> &'static str {
        match self {
            EvalResult::Passed(_) => "PASS",
            EvalResult::Failed { .. } => "FAIL",
            EvalResult::Skipped(_) => "SKIP",
            EvalResult::Error(_) => "ERROR",
        }
    }
}

/// Per-case result line in a final summary.
#[derive(Debug, Clone)]
pub struct CaseReport {
    pub case_id: String,
    pub category: String,
    pub result: EvalResult,
}

/// Aggregate report for a full eval suite run.
#[derive(Debug, Clone)]
pub struct EvalReport {
    pub cases: Vec<CaseReport>,
    pub total_duration_ms: u128,
}

impl EvalReport {
    pub fn passed_count(&self) -> usize {
        self.cases.iter().filter(|c| matches!(c.result, EvalResult::Passed(_))).count()
    }

    pub fn failed_count(&self) -> usize {
        self.cases.iter().filter(|c| matches!(c.result, EvalResult::Failed { .. })).count()
    }

    pub fn skipped_count(&self) -> usize {
        self.cases.iter().filter(|c| matches!(c.result, EvalResult::Skipped(_))).count()
    }

    pub fn error_count(&self) -> usize {
        self.cases.iter().filter(|c| matches!(c.result, EvalResult::Error(_))).count()
    }

    /// `true` if no case ended in Failed / Error (Skipped is OK for CI).
    pub fn is_clean(&self) -> bool {
        self.failed_count() == 0 && self.error_count() == 0
    }

    /// Pretty-print to a string suitable for `cargo test --nocapture` output.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("\n========== Eval Report ==========\n");
        for c in &self.cases {
            s.push_str(&format!("[{}] {:<28} {}", c.result.label(), c.case_id, c.category));
            match &c.result {
                EvalResult::Passed(stats) => {
                    s.push_str(&format!(
                        " ({:.1}s, {} tools, ~{} tokens)\n",
                        stats.duration_ms as f64 / 1000.0,
                        stats.tool_calls.len(),
                        stats.estimated_tokens
                    ));
                }
                EvalResult::Failed { stats, failures } => {
                    s.push_str(&format!(
                        " ({:.1}s, {} tools)\n",
                        stats.duration_ms as f64 / 1000.0,
                        stats.tool_calls.len()
                    ));
                    for f in failures {
                        s.push_str(&format!("       - {f}\n"));
                    }
                }
                EvalResult::Skipped(r) => {
                    s.push_str(&format!(" ({r})\n"));
                }
                EvalResult::Error(e) => {
                    s.push_str(&format!(" (ERROR: {e})\n"));
                }
            }
        }
        s.push_str(&format!(
            "\nPassed {} / Failed {} / Skipped {} / Error {} (total {:.1}s)\n",
            self.passed_count(),
            self.failed_count(),
            self.skipped_count(),
            self.error_count(),
            self.total_duration_ms as f64 / 1000.0
        ));
        s
    }
}

/// Run every case in `cases` against `endpoint` and aggregate results.
///
/// Optimization: we probe the endpoint **once** up front. If it's unreachable,
/// every case becomes a Skipped without further probing -- otherwise a 30-case
/// suite at 2s timeout per probe = 60s of wasted wait.
pub fn run_cases(cases: &[EvalCase], endpoint: &EndpointConfig) -> EvalReport {
    let suite_start = Instant::now();
    let endpoint_alive = runner::probe_endpoint(endpoint);

    let mut report_cases = Vec::with_capacity(cases.len());
    for case in cases {
        let result = if !endpoint_alive {
            EvalResult::Skipped(SkipReason::EndpointUnreachable(endpoint.endpoint.clone()))
        } else {
            match runner::run_case(case, endpoint) {
                Ok(outcome) => match outcome {
                    EvalOutcome::Skipped(reason) => EvalResult::Skipped(reason),
                    EvalOutcome::Ran(stats) => {
                        let failures = check_expectations(&case.expected, &stats);
                        if failures.is_empty() {
                            EvalResult::Passed(stats)
                        } else {
                            EvalResult::Failed { stats, failures }
                        }
                    }
                },
                Err(e) => EvalResult::Error(e),
            }
        };
        report_cases.push(CaseReport {
            case_id: case.id.clone(),
            category: case.category.clone(),
            result,
        });
    }
    EvalReport {
        cases: report_cases,
        total_duration_ms: suite_start.elapsed().as_millis(),
    }
}

/// Convenience: load a directory of YAML cases and run them all.
pub fn run_cases_dir(dir: &Path, endpoint: &EndpointConfig) -> Result<EvalReport, String> {
    let cases = load_cases_dir(dir).map_err(|e| e.to_string())?;
    Ok(run_cases(&cases, endpoint))
}

/// Default location for golden-prompt YAML files within the workspace.
/// Cargo runs unit tests with CWD = crate root (`app/`), so a relative
/// path works during `cargo test --features eval`.
pub fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("tests").join("eval-fixtures")
}

/// Build an [`EndpointConfig`] from environment variables. Useful for
/// `cargo test --features eval -- --nocapture` runs.
///
/// Reads (with defaults):
/// - `NEUROBOOT_EVAL_ENDPOINT` → `http://127.0.0.1:8080`
/// - `NEUROBOOT_EVAL_MODEL`    → `qwen3-4b-instruct`
/// - `NEUROBOOT_EVAL_API_KEY`  → unset
pub fn endpoint_from_env() -> EndpointConfig {
    let endpoint = std::env::var("NEUROBOOT_EVAL_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());
    let model = std::env::var("NEUROBOOT_EVAL_MODEL")
        .unwrap_or_else(|_| "qwen3-4b-instruct".to_owned());
    let api_key = std::env::var("NEUROBOOT_EVAL_API_KEY").ok();
    EndpointConfig {
        endpoint,
        model,
        api_key,
        health_probe_timeout: std::time::Duration::from_secs(3),
    }
}

#[cfg(test)]
mod integration {
    //! Suite-level tests that hit the YAML fixtures.
    //!
    //! `eval_fixtures_all_parse` runs in any environment (no LLM).
    //! `run_golden_prompts` Skips every case if the configured endpoint isn't
    //! reachable. Set `NEUROBOOT_EVAL_STRICT=1` to make behavioral failures
    //! fail the test (default is report-only so the suite stays informational).

    use super::*;

    #[test]
    fn eval_fixtures_all_parse() {
        let dir = fixture_dir();
        let cases = load_cases_dir(&dir).unwrap_or_else(|e| {
            panic!("failed to load eval fixtures from {}: {e}", dir.display())
        });
        assert!(
            cases.len() >= 30,
            "expected >=30 fixtures, got {} (cwd={:?})",
            cases.len(),
            std::env::current_dir().ok()
        );
        for c in &cases {
            assert!(!c.id.is_empty(), "case has empty id");
            assert!(!c.category.is_empty(), "{} has empty category", c.id);
            assert!(!c.input.user.is_empty(), "{} has empty user input", c.id);
        }
        let cats: std::collections::BTreeSet<String> =
            cases.iter().map(|c| c.category.clone()).collect();
        for required in ["skill", "free-form", "multi-step", "dangerous", "edge-case"] {
            assert!(
                cats.contains(required),
                "no fixtures with category `{required}` -- got {cats:?}"
            );
        }
    }

    #[test]
    fn run_golden_prompts() {
        let dir = fixture_dir();
        let endpoint = endpoint_from_env();
        let report = match run_cases_dir(&dir, &endpoint) {
            Ok(r) => r,
            Err(e) => panic!("eval suite errored before any case ran: {e}"),
        };
        println!("{}", report.render());

        let strict = std::env::var("NEUROBOOT_EVAL_STRICT")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if strict {
            assert!(
                report.is_clean(),
                "eval suite had {} failures and {} errors",
                report.failed_count(),
                report.error_count()
            );
        } else if report.error_count() > 0 {
            panic!(
                "{} cases hit infrastructure errors; see report above",
                report.error_count()
            );
        }
    }
}

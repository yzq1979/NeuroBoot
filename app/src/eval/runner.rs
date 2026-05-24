//! Drives the agent loop without a GUI and collects an [`ExecutionStats`]
//! suitable for matching against an [`Expected`](super::spec::Expected).
//!
//! This module is the only part of the eval framework that touches network
//! / the agent worker. Tests that don't need a live LLM should stay in
//! `matchers.rs` / `spec.rs`.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::{
    spawn_agent_request, AgentEvent, AgentJob, ConfirmationResponse, PlanResponse,
};
use crate::hooks::HooksConfig;
use crate::tools::{self, ToolRegistry};
use crate::ui::chat::ChatMessage;

use super::spec::EvalCase;

/// LLM endpoint config the runner targets.
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Health-probe timeout. If `/health` doesn't return 200 within this,
    /// the case is reported as [`SkipReason::EndpointUnreachable`].
    pub health_probe_timeout: Duration,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".to_owned(),
            model: "qwen3-4b-instruct".to_owned(),
            api_key: None,
            health_probe_timeout: Duration::from_secs(3),
        }
    }
}

/// One observed tool_call in the agent's response.
#[derive(Debug, Clone)]
pub struct ToolCallObserved {
    pub name: String,
    /// Raw JSON args string (we don't validate the args here -- expectations
    /// only match against tool *names* in v3.0 W8).
    pub arguments: String,
}

/// Stats produced by one agent run, fed into `check_expectations`.
#[derive(Debug, Clone)]
pub struct ExecutionStats {
    pub tool_calls: Vec<ToolCallObserved>,
    /// Concatenated assistant text (all streaming chunks merged).
    pub final_text: String,
    pub duration_ms: u128,
    /// Cheap heuristic: assume ~4 chars per token for English / ~1.5 chars
    /// for Chinese. We blend them at 2.5. Good enough for trend tracking.
    pub estimated_tokens: usize,
}

/// What happened on a single case.
pub enum EvalOutcome {
    Ran(ExecutionStats),
    Skipped(SkipReason),
}

/// Why a case wasn't executed.
#[derive(Debug, Clone)]
pub enum SkipReason {
    EndpointUnreachable(String),
    CaseDisabled(String),
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::EndpointUnreachable(s) => write!(f, "skip: endpoint unreachable ({s})"),
            SkipReason::CaseDisabled(s) => write!(f, "skip: case disabled ({s})"),
        }
    }
}

/// Run one case against the configured endpoint.
///
/// Returns `Err` only on infrastructure failures (panics, mpsc disconnects);
/// model-level failures (HTTP 5xx, etc.) surface as `Ran` with whatever
/// stats we accumulated so the matcher can decide.
pub fn run_case(case: &EvalCase, endpoint: &EndpointConfig) -> Result<EvalOutcome, String> {
    // Health probe first -- a missing model is a skip, not a failure.
    if !probe_health(endpoint) {
        return Ok(EvalOutcome::Skipped(SkipReason::EndpointUnreachable(
            endpoint.endpoint.clone(),
        )));
    }

    let registry = build_eval_tool_registry();
    let hooks = Arc::new(HooksConfig::default());

    let job = AgentJob {
        endpoint: endpoint.endpoint.clone(),
        model: endpoint.model.clone(),
        api_key: endpoint.api_key.clone(),
        system_prompt: crate::DEFAULT_SYSTEM_PROMPT.to_owned(),
        messages: vec![ChatMessage::user_with_images(case.input.user.clone(), Vec::new())],
        tool_registry: Arc::new(registry),
        cancel: Arc::new(AtomicBool::new(false)),
        hooks_config: hooks,
    };

    let start = Instant::now();
    let rx = spawn_agent_request(job);
    let mut stats = ExecutionStats {
        tool_calls: Vec::new(),
        final_text: String::new(),
        duration_ms: 0,
        estimated_tokens: 0,
    };

    let hard_deadline = start + Duration::from_secs(case.expected.max_response_seconds.max(60) + 30);

    drive_loop(&rx, &mut stats, hard_deadline)?;
    stats.duration_ms = start.elapsed().as_millis();
    stats.estimated_tokens = estimate_tokens(&stats.final_text);

    Ok(EvalOutcome::Ran(stats))
}

/// Public form of [`probe_health`] used by [`super::run_cases`] to fail-fast
/// the whole suite when the endpoint is unreachable.
pub fn probe_endpoint(endpoint: &EndpointConfig) -> bool {
    probe_health(endpoint)
}

/// Cheap blocking probe — issue GET /health, decide unreachable on any
/// failure within `health_probe_timeout`.
fn probe_health(endpoint: &EndpointConfig) -> bool {
    let url = format!(
        "{}/health",
        endpoint.endpoint.trim_end_matches('/')
    );
    let client = match reqwest::blocking::Client::builder()
        .timeout(endpoint.health_probe_timeout)
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.get(&url).send() {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Drain agent events into `stats`. Auto-handles Confirmation / PlanProposal
/// so the loop never blocks waiting for a UI.
fn drive_loop(
    rx: &mpsc::Receiver<AgentEvent>,
    stats: &mut ExecutionStats,
    hard_deadline: Instant,
) -> Result<(), String> {
    loop {
        // Bound each recv so we don't hang forever if the worker died silently.
        let remaining = hard_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("hard deadline exceeded".into());
        }
        let recv_timeout = remaining.min(Duration::from_secs(30));
        match rx.recv_timeout(recv_timeout) {
            Ok(AgentEvent::AssistantStart) => {}
            Ok(AgentEvent::TokenChunk(s)) => stats.final_text.push_str(&s),
            Ok(AgentEvent::AssistantToolCalls(summaries)) => {
                for s in summaries {
                    stats.tool_calls.push(ToolCallObserved {
                        name: s.name,
                        arguments: s.arguments,
                    });
                }
            }
            Ok(AgentEvent::Message(msg)) => {
                use crate::ui::chat::Role;
                if msg.role == Role::Assistant {
                    if !msg.content.is_empty() {
                        if !stats.final_text.is_empty()
                            && !stats.final_text.ends_with('\n')
                        {
                            stats.final_text.push('\n');
                        }
                        stats.final_text.push_str(&msg.content);
                    }
                }
                // Role::Tool / Role::User / Role::System messages are part of the
                // internal loop; matchers don't see them.
            }
            Ok(AgentEvent::Confirmation(req)) => {
                // Eval never auto-executes dangerous tools. Reject with a
                // synthetic "user denied" reply so the agent re-plans or
                // bails out.
                let _ = req.responder.send(ConfirmationResponse::Reject);
            }
            Ok(AgentEvent::PlanProposal(req)) => {
                // Auto-approve so the agent proceeds to attempt the plan steps;
                // any dangerous step will then hit our Confirmation auto-reject.
                let _ = req.responder.send(PlanResponse::Approve);
            }
            Ok(AgentEvent::Done) => return Ok(()),
            Ok(AgentEvent::Error(e)) => {
                // Surface the model's error in the final text so matchers can
                // see what happened, then end the loop cleanly.
                if !stats.final_text.is_empty() {
                    stats.final_text.push('\n');
                }
                stats.final_text.push_str(&format!("[agent error] {e}"));
                return Ok(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Worker quiet too long -- abort.
                return Err(format!(
                    "no agent event for {}s",
                    recv_timeout.as_secs()
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("agent worker disconnected before Done".into());
            }
        }
    }
}

/// Build the same registry the GUI ships (safe + dangerous), so eval cases
/// can test that the agent CHOOSES the right tools for the prompt.
///
/// Dangerous tools stay registered -- eval cases that test the dangerous
/// path want the agent to attempt them, then the auto-reject in [`drive_loop`]
/// stops actual execution. Cases that should NOT trigger dangerous tools
/// just put them in `must_not_call`.
fn build_eval_tool_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    // v1 baseline (safe)
    r.register(Box::new(tools::safe::list_disks::ListDisks));
    r.register(Box::new(tools::safe::read_system_info::ReadSystemInfo));
    r.register(Box::new(tools::safe::read_event_log_errors::ReadEventLogErrors));
    // v2 Stage A safe
    r.register(Box::new(tools::safe::list_partitions::ListPartitions));
    r.register(Box::new(tools::safe::list_volumes::ListVolumes));
    r.register(Box::new(tools::safe::read_ip_config::ReadIpConfig));
    r.register(Box::new(tools::safe::list_network_adapters::ListNetworkAdapters));
    r.register(Box::new(tools::safe::list_processes_top::ListProcessesTop));
    r.register(Box::new(tools::safe::list_services::ListServices));
    r.register(Box::new(tools::safe::list_minidumps::ListMinidumps));
    r.register(Box::new(tools::safe::list_recent_shutdowns::ListRecentShutdowns));
    // v2 Stage 6 / v3 quick wins
    r.register(Box::new(tools::safe::read_smart::ReadSmart));
    r.register(Box::new(tools::safe::extract_archive::ExtractArchive));
    r.register(Box::new(tools::safe::analyze_minidump::AnalyzeMinidump));
    // v3.0 W1.5 + W3-4 + W7 + W6-7
    r.register(Box::new(tools::safe::load_skill::LoadSkill));
    r.register(Box::new(tools::safe::propose_plan::ProposePlan));
    r.register(Box::new(tools::safe::list_winre_status::ListWinreStatus));
    r.register(Box::new(tools::safe::bitlocker_status::BitlockerStatus));
    r.register(Box::new(tools::safe::find_large_files::FindLargeFiles));
    r.register(Box::new(tools::safe::read_recent_installs::ReadRecentInstalls));
    r.register(Box::new(tools::safe::lookup_error_code::LookupErrorCode));
    r.register(Box::new(tools::safe::memory::Memory));
    // Dangerous (eval auto-rejects confirmation, so they never actually run)
    r.register(Box::new(tools::dangerous::delete_path::DeletePath));
    r.register(Box::new(tools::dangerous::run_chkdsk::RunChkdsk));
    r.register(Box::new(tools::dangerous::run_sfc::RunSfcScannow));
    r.register(Box::new(tools::dangerous::run_dism_restorehealth::RunDismRestoreHealth));
    r.register(Box::new(tools::dangerous::defender_offline_scan::DefenderOfflineScan));
    r.register(Box::new(tools::dangerous::bootrec_rebuild_bcd::BootrecRebuildBcd));
    r.register(Box::new(tools::dangerous::reset_local_admin_password::ResetLocalAdminPassword));
    r.register(Box::new(tools::dangerous::testdisk_scan_partition::TestdiskScanPartition));
    r
}

/// Mixed-script token estimator. Counts ASCII chars at 4 per token and
/// non-ASCII (CJK / emoji) at 1.5 per token. Trend tool, not exact.
fn estimate_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut other = 0usize;
    for c in text.chars() {
        if c.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    (ascii / 4) + ((other as f64 / 1.5).ceil() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_handles_mixed_script() {
        let t1 = estimate_tokens("hello world");
        let t2 = estimate_tokens("你好世界");
        // English at 4 chars/tok: "hello world" = 11 chars / 4 ~= 2.
        assert!((1..=4).contains(&t1));
        // CJK at 1.5 chars/tok: 4 chars / 1.5 = 2.67 -> 3.
        assert_eq!(t2, 3);
    }

    #[test]
    fn probe_health_returns_false_for_bad_port() {
        let cfg = EndpointConfig {
            endpoint: "http://127.0.0.1:1".to_owned(),
            health_probe_timeout: Duration::from_millis(300),
            ..Default::default()
        };
        assert!(!probe_health(&cfg));
    }

    #[test]
    fn endpoint_unreachable_yields_skipped() {
        let case = EvalCase {
            id: "x".into(),
            category: "skill".into(),
            description: "".into(),
            input: super::super::spec::Input { user: "hi".into() },
            expected: super::super::spec::Expected::default(),
        };
        let cfg = EndpointConfig {
            endpoint: "http://127.0.0.1:1".to_owned(),
            health_probe_timeout: Duration::from_millis(300),
            ..Default::default()
        };
        let outcome = run_case(&case, &cfg).unwrap();
        assert!(matches!(outcome, EvalOutcome::Skipped(_)));
    }

    #[test]
    fn registry_includes_safe_and_dangerous() {
        let r = build_eval_tool_registry();
        assert!(r.get("list_disks").is_some(), "safe registered");
        assert!(r.get("delete_path").is_some(), "dangerous registered");
        // Total roughly matches the GUI registration (~30 tools).
        assert!(r.len() >= 25, "expected >=25 tools, got {}", r.len());
    }
}

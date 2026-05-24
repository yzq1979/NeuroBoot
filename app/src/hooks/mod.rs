//! Hooks 系统 —— v3.0 W6-7。
//!
//! 灵感来自 Claude Code Hooks（[2026 完整指南](https://ofox.ai/blog/claude-code-hooks-subagents-skills-complete-guide-2026/)）。
//! Hook 是 deterministic 100% 的执行点，比让 LLM "记得" / system prompt 提醒强 ——
//! 因为 hook 由 NeuroBoot 主循环主动跑，跟模型自由意志无关。
//!
//! ## 简化范围（vs 完整 Claude Code）
//!
//! - **4 个 hook 点**：SessionStart / PreToolUse / PostToolUse / Stop
//! - **Handler 只支持 `type: command`**：跑 PS / CMD / .exe 拿 stdout + exit code
//!   - **不**做 HTTP handler（PE 内复杂度高、价值低）
//!   - **不**做 PostToolUse / Stop 之外的 user-prompt-submit-hook 等扩展点
//! - **timeout 默认 10s**：PE 环境快进快出，不允许阻塞太久
//!
//! ## 配置文件位置
//!
//! 跟 `prompts.txt` 同优先级：扫所有非 `X:` 盘符，找第一个：
//! - `<root>\NeuroBoot\hooks.json`
//! - `<root>\NeuroBoot.hooks.json`
//!
//! ## hooks.json schema
//!
//! ```json
//! {
//!   "hooks": {
//!     "SessionStart": [
//!       { "type": "command", "command": "powershell -File X:\\my-init.ps1", "timeout": 5 }
//!     ],
//!     "PreToolUse": [
//!       { "type": "command", "command": "echo blocked", "matcher": "delete_path" }
//!     ],
//!     "PostToolUse": [
//!       { "type": "command", "command": "X:\\log-tool.cmd" }
//!     ],
//!     "Stop": []
//!   }
//! }
//! ```
//!
//! - `matcher`（可选）：仅 PreToolUse / PostToolUse 用 —— 工具 name 完全匹配才触发
//! - `timeout`（可选）：单位秒，默认 10
//!
//! ## 行为
//!
//! - **SessionStart**：返回的 stdout 被 [`run_session_start`] 收集成字符串，
//!   调用方（main.rs）拼到 system prompt 末尾
//! - **PreToolUse**：通过环境变量 `NEUROBOOT_TOOL_NAME` / `NEUROBOOT_TOOL_ARGS` 传上下文。
//!   **非零 exit 代码 = 拒绝该工具调用**（agent loop 看到 [`HookOutcome::Block`] 跳过 execute，
//!   合成 "（hook 拒绝）..." tool result 回灌 LLM）
//! - **PostToolUse**：额外传 `NEUROBOOT_TOOL_SUCCESS` / `NEUROBOOT_TOOL_RESULT`（截断 4 KB）。
//!   exit code 仅记日志，**不**回灌（已经执行完了）
//! - **Stop**：单 turn 结束（Done / Error 前）触发；exit code 不影响流程

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Deserialize;

/// 4 个 hook 触发点 —— 与 Claude Code 模型 1:1 对齐（去掉 PostToolUse 之外的扩展点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    SessionStart,
    PreToolUse,
    PostToolUse,
    Stop,
}

impl HookEvent {
    pub fn as_key(self) -> &'static str {
        match self {
            HookEvent::SessionStart => "SessionStart",
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::Stop => "Stop",
        }
    }
}

/// 单个 hook handler 配置（hooks.json 数组的元素）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HookHandler {
    /// 当前只支持 "command"；其它值 [`HooksConfig::sanitize`] 会过滤掉
    #[serde(rename = "type")]
    pub kind: String,
    /// shell 命令字符串 —— 直接传 PowerShell 的 -Command 跑
    pub command: String,
    /// 工具 name 完全匹配才触发（仅 PreToolUse / PostToolUse 有意义）；
    /// None / 空 = 匹配所有工具
    #[serde(default)]
    pub matcher: Option<String>,
    /// 单位秒；None = 用默认 10s
    #[serde(default)]
    pub timeout: Option<u64>,
}

const DEFAULT_TIMEOUT_SEC: u64 = 10;

/// 完整 hooks.json 反序列化结构。
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: HooksByEvent,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct HooksByEvent {
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<HookHandler>,
    #[serde(rename = "PreToolUse", default)]
    pub pre_tool_use: Vec<HookHandler>,
    #[serde(rename = "PostToolUse", default)]
    pub post_tool_use: Vec<HookHandler>,
    #[serde(rename = "Stop", default)]
    pub stop: Vec<HookHandler>,
}

impl HooksConfig {
    /// 取指定 event 的 handlers 列表。
    pub fn handlers_for(&self, event: HookEvent) -> &[HookHandler] {
        match event {
            HookEvent::SessionStart => &self.hooks.session_start,
            HookEvent::PreToolUse => &self.hooks.pre_tool_use,
            HookEvent::PostToolUse => &self.hooks.post_tool_use,
            HookEvent::Stop => &self.hooks.stop,
        }
    }

    /// 过滤掉非 "command" 类型 handler（后续可能扩展 http 等）。
    pub fn sanitize(mut self) -> Self {
        let keep = |v: &mut Vec<HookHandler>| v.retain(|h| h.kind == "command" && !h.command.trim().is_empty());
        keep(&mut self.hooks.session_start);
        keep(&mut self.hooks.pre_tool_use);
        keep(&mut self.hooks.post_tool_use);
        keep(&mut self.hooks.stop);
        self
    }

    /// 解析 hooks.json 字符串内容。返回 None = 不是合法 JSON 或 schema 不对。
    pub fn parse(content: &str) -> Option<Self> {
        let raw: HooksConfig = serde_json::from_str(content).ok()?;
        Some(raw.sanitize())
    }
}

/// 扫所有非 `X:` 盘根，找第一个 hooks.json。返回 (config, source_path)。
pub fn scan_hooks_config() -> Option<(HooksConfig, PathBuf)> {
    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        for filename in &["NeuroBoot\\hooks.json", "NeuroBoot.hooks.json"] {
            let path = PathBuf::from(format!("{c}:\\{filename}"));
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(cfg) = HooksConfig::parse(&content) {
                    return Some((cfg, path));
                }
            }
        }
    }
    None
}

/// 单个 hook 调用的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookExecution {
    /// 命令字符串（用于日志展示）
    pub command: String,
    /// stdout trim 后的内容
    pub stdout: String,
    /// stderr trim 后的内容（仅记日志用）
    pub stderr: String,
    /// 退出码；超时 = None
    pub exit_code: Option<i32>,
    /// 是否超时
    pub timed_out: bool,
}

impl HookExecution {
    /// 成功 = 退出码为 0 且未超时。
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

/// PreToolUse 的判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// 所有 PreToolUse hook 都成功（或没有 matcher 匹配）—— 允许工具执行
    Allow,
    /// 至少一个 PreToolUse hook 非零退出 —— 拒绝工具，附理由（给 LLM 看的）
    Block { reason: String },
}

/// 拼装 stdout 字符串给 SessionStart 用 —— 把多个 hook 的 stdout 串起来。
pub fn collect_stdout(execs: &[HookExecution]) -> String {
    let mut buf = String::new();
    for e in execs {
        if !e.stdout.is_empty() {
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str(&e.stdout);
        }
    }
    buf
}

/// 启动 powershell 跑一行 command，限 timeout 秒；阻塞返回。
fn run_command(handler: &HookHandler, env: &[(&str, String)]) -> HookExecution {
    let timeout = Duration::from_secs(handler.timeout.unwrap_or(DEFAULT_TIMEOUT_SEC).clamp(1, 60));

    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &handler.command,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return HookExecution {
                command: handler.command.clone(),
                stdout: String::new(),
                stderr: format!("hook spawn 失败：{e}"),
                exit_code: None,
                timed_out: false,
            };
        }
    };

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut s) = child.stdout.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut stdout);
                }
                if let Some(mut s) = child.stderr.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut stderr);
                }
                return HookExecution {
                    command: handler.command.clone(),
                    stdout: stdout.trim().to_owned(),
                    stderr: stderr.trim().to_owned(),
                    exit_code: status.code(),
                    timed_out: false,
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return HookExecution {
                        command: handler.command.clone(),
                        stdout: String::new(),
                        stderr: format!("hook 超时 {}s 被强制终止", timeout.as_secs()),
                        exit_code: None,
                        timed_out: true,
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return HookExecution {
                    command: handler.command.clone(),
                    stdout: String::new(),
                    stderr: format!("等待 hook 失败：{e}"),
                    exit_code: None,
                    timed_out: false,
                };
            }
        }
    }
}

/// 判断某 handler 的 matcher 是否匹配 tool_name（None / 空匹配所有）。
fn matcher_matches(handler: &HookHandler, tool_name: &str) -> bool {
    match &handler.matcher {
        None => true,
        Some(m) if m.trim().is_empty() => true,
        Some(m) => m == tool_name,
    }
}

/// 跑 SessionStart 所有 hook，返回每个 hook 的执行记录（调用方拼 stdout 进 system prompt）。
pub fn run_session_start(config: &HooksConfig) -> Vec<HookExecution> {
    config
        .handlers_for(HookEvent::SessionStart)
        .iter()
        .map(|h| run_command(h, &[]))
        .collect()
}

/// 截断 4 KB —— PostToolUse / 日志用。
fn truncate_for_env(s: &str) -> String {
    const CAP: usize = 4096;
    if s.len() <= CAP {
        s.to_owned()
    } else {
        let mut t = s[..CAP].to_owned();
        t.push_str("...[truncated]");
        t
    }
}

/// 跑 PreToolUse hook —— 任一非零 exit 返回 [`HookOutcome::Block`]。
///
/// `tool_name` / `tool_args` 通过环境变量传给 hook 命令。
pub fn run_pre_tool_use(
    config: &HooksConfig,
    tool_name: &str,
    tool_args: &str,
) -> (HookOutcome, Vec<HookExecution>) {
    let mut execs = Vec::new();
    let env = [
        ("NEUROBOOT_TOOL_NAME", tool_name.to_owned()),
        ("NEUROBOOT_TOOL_ARGS", truncate_for_env(tool_args)),
    ];
    for h in config.handlers_for(HookEvent::PreToolUse) {
        if !matcher_matches(h, tool_name) {
            continue;
        }
        let e = run_command(h, &env);
        let blocked = !e.success();
        let reason_command = e.command.clone();
        let reason_stderr = e.stderr.clone();
        execs.push(e);
        if blocked {
            let reason = if reason_stderr.is_empty() {
                format!("PreToolUse hook 拒绝（command: {reason_command}）")
            } else {
                format!("PreToolUse hook 拒绝：{reason_stderr}")
            };
            return (HookOutcome::Block { reason }, execs);
        }
    }
    (HookOutcome::Allow, execs)
}

/// 跑 PostToolUse hook —— exit code 仅记录，不影响后续。
pub fn run_post_tool_use(
    config: &HooksConfig,
    tool_name: &str,
    tool_args: &str,
    success: bool,
    result: &str,
) -> Vec<HookExecution> {
    let env = [
        ("NEUROBOOT_TOOL_NAME", tool_name.to_owned()),
        ("NEUROBOOT_TOOL_ARGS", truncate_for_env(tool_args)),
        ("NEUROBOOT_TOOL_SUCCESS", if success { "1" } else { "0" }.to_owned()),
        ("NEUROBOOT_TOOL_RESULT", truncate_for_env(result)),
    ];
    config
        .handlers_for(HookEvent::PostToolUse)
        .iter()
        .filter(|h| matcher_matches(h, tool_name))
        .map(|h| run_command(h, &env))
        .collect()
}

/// 跑 Stop hook —— 单 turn 结束触发，exit code 仅记录。
pub fn run_stop(config: &HooksConfig) -> Vec<HookExecution> {
    config
        .handlers_for(HookEvent::Stop)
        .iter()
        .map(|h| run_command(h, &[]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r#"{
            "hooks": {
                "SessionStart": [
                    { "type": "command", "command": "echo hi" }
                ]
            }
        }"#;
        let cfg = HooksConfig::parse(json).expect("must parse");
        assert_eq!(cfg.hooks.session_start.len(), 1);
        assert_eq!(cfg.hooks.session_start[0].command, "echo hi");
        assert!(cfg.hooks.pre_tool_use.is_empty());
    }

    #[test]
    fn sanitize_strips_unknown_types() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    { "type": "http", "command": "https://example.com" },
                    { "type": "command", "command": "echo ok" },
                    { "type": "command", "command": "  " }
                ]
            }
        }"#;
        let cfg = HooksConfig::parse(json).expect("must parse");
        assert_eq!(cfg.hooks.pre_tool_use.len(), 1);
        assert_eq!(cfg.hooks.pre_tool_use[0].command, "echo ok");
    }

    #[test]
    fn parse_handler_with_matcher_and_timeout() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    { "type": "command", "command": "echo block", "matcher": "delete_path", "timeout": 3 }
                ]
            }
        }"#;
        let cfg = HooksConfig::parse(json).expect("must parse");
        let h = &cfg.hooks.pre_tool_use[0];
        assert_eq!(h.matcher.as_deref(), Some("delete_path"));
        assert_eq!(h.timeout, Some(3));
    }

    #[test]
    fn matcher_match_logic() {
        let with_matcher = HookHandler {
            kind: "command".into(),
            command: "x".into(),
            matcher: Some("delete_path".into()),
            timeout: None,
        };
        let no_matcher = HookHandler {
            kind: "command".into(),
            command: "x".into(),
            matcher: None,
            timeout: None,
        };
        assert!(matcher_matches(&with_matcher, "delete_path"));
        assert!(!matcher_matches(&with_matcher, "list_disks"));
        assert!(matcher_matches(&no_matcher, "anything"));
    }

    #[test]
    fn handlers_for_returns_right_slice() {
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "Stop": [
                    { "type": "command", "command": "echo stop" }
                ]
            } }"#,
        )
        .unwrap();
        assert_eq!(cfg.handlers_for(HookEvent::Stop).len(), 1);
        assert_eq!(cfg.handlers_for(HookEvent::SessionStart).len(), 0);
    }

    // ---- Live PowerShell-driven tests（依赖系统有 powershell.exe；Windows 开发机 OK）----

    #[test]
    fn session_start_collects_stdout() {
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "SessionStart": [
                    { "type": "command", "command": "Write-Output 'hello-hook'" }
                ]
            } }"#,
        )
        .unwrap();
        let execs = run_session_start(&cfg);
        assert_eq!(execs.len(), 1);
        assert!(execs[0].success(), "hook should succeed: {:?}", execs[0]);
        assert_eq!(execs[0].stdout, "hello-hook");
        assert_eq!(collect_stdout(&execs), "hello-hook");
    }

    #[test]
    fn pre_tool_use_nonzero_exit_blocks() {
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "PreToolUse": [
                    { "type": "command", "command": "Write-Error 'nope'; exit 2", "matcher": "delete_path" }
                ]
            } }"#,
        )
        .unwrap();
        let (outcome, execs) = run_pre_tool_use(&cfg, "delete_path", "{}");
        assert_eq!(execs.len(), 1);
        match outcome {
            HookOutcome::Block { reason } => assert!(reason.contains("PreToolUse hook 拒绝")),
            HookOutcome::Allow => panic!("should block, got allow"),
        }
    }

    #[test]
    fn pre_tool_use_matcher_skips_other_tools() {
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "PreToolUse": [
                    { "type": "command", "command": "exit 1", "matcher": "delete_path" }
                ]
            } }"#,
        )
        .unwrap();
        let (outcome, execs) = run_pre_tool_use(&cfg, "list_disks", "{}");
        assert_eq!(execs.len(), 0); // matcher 不中，hook 跳过
        assert_eq!(outcome, HookOutcome::Allow);
    }

    #[test]
    fn pre_tool_use_passes_env_vars() {
        // hook 读环境变量并 echo —— 验证 NEUROBOOT_TOOL_NAME 被设
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "PreToolUse": [
                    { "type": "command", "command": "Write-Output $env:NEUROBOOT_TOOL_NAME" }
                ]
            } }"#,
        )
        .unwrap();
        let (outcome, execs) = run_pre_tool_use(&cfg, "list_disks", "{\"x\":1}");
        assert_eq!(outcome, HookOutcome::Allow);
        assert_eq!(execs[0].stdout, "list_disks");
    }

    #[test]
    fn timeout_kills_long_hook() {
        // sleep 5s + timeout 1s => 超时
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "Stop": [
                    { "type": "command", "command": "Start-Sleep -Seconds 5", "timeout": 1 }
                ]
            } }"#,
        )
        .unwrap();
        let execs = run_stop(&cfg);
        assert_eq!(execs.len(), 1);
        assert!(execs[0].timed_out, "should time out: {:?}", execs[0]);
        assert!(!execs[0].success());
    }

    #[test]
    fn post_tool_use_passes_success_and_result_env() {
        let cfg = HooksConfig::parse(
            r#"{ "hooks": {
                "PostToolUse": [
                    { "type": "command", "command": "Write-Output \"$env:NEUROBOOT_TOOL_SUCCESS|$env:NEUROBOOT_TOOL_RESULT\"" }
                ]
            } }"#,
        )
        .unwrap();
        let execs = run_post_tool_use(&cfg, "list_disks", "{}", true, "5 disks found");
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].stdout, "1|5 disks found");
    }
}

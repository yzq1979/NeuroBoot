//! 端点配置 + A/C 双备探测与切换。
//!
//! 方案 A = Remote（云端 / 局域网 OpenAI 兼容），通常更强但需要网络
//! 方案 C = Local（本机 llama-server），离线可用、PE 兼容
//!
//! 配置来源优先级：环境变量 > config.json > 程序默认。
//! 启动时探测 A：可达则 active = A，inactive = C；不可达或未配置则 active = C，inactive = None。
//! UI 顶栏「切换」按钮在 inactive 为 Some 时才显示。
//!
//! 阶段 v1.0.1：加 config.json 加载（PE 内无法注入 env var 的痛点），
//! 探测超时从 3s 改 5s + 优先 HEAD /v1/models（更轻量）。

use std::time::Duration;

use super::config_file::{load_config_file, ConfigFile};

/// 端点配置（OpenAI 兼容）。
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    /// 端点根地址（不含 `/v1/chat/completions` 路径）
    pub endpoint: String,
    /// OpenAI 协议里 model 字段的值
    pub model: String,
    /// 可选 API key（Some 时请求加 `Authorization: Bearer ...` 头）
    pub api_key: Option<String>,
    /// UI 上显示用的标签（如「本地」「云端」）
    pub label: String,
}

/// 探测结果：active + inactive + 当前生效的 ConfigFile（UI 设置面板初始化用）+ 配置来源描述。
pub struct DetectedEndpoints {
    pub active: EndpointConfig,
    pub inactive: Option<EndpointConfig>,
    /// 当前生效的合并后 config（env var 覆盖了 config.json 的部分）；
    /// 若没找到任何 config.json，是纯默认 + env var。
    pub effective: ConfigFile,
    /// 描述用：「config 来自 D:\NeuroBoot.config.json」或「没找到 config 文件，用程序默认」
    pub source_hint: String,
    /// 描述用：「探测云端 200ms / 不可达」
    pub probe_hint: String,
}

/// 启动时根据 env var + config.json 决定 active/inactive endpoint。
///
/// 环境变量（最高优先级，用于开发调试）：
/// - `NEUROBOOT_A_ENDPOINT` / `NEUROBOOT_A_MODEL` / `NEUROBOOT_A_API_KEY`
///
/// 决策流程：
/// 1. 读 config.json（找不到用 default）
/// 2. env var 非空时覆盖 config 中 remote_* 三项
/// 3. 若最终 remote_endpoint 非空且 `prefer_remote=true` 且 探测可达 → active = Remote, inactive = Local
/// 4. 否则 → active = Local；若 remote 配了但探测失败/被禁，仍把它放进 inactive 让用户能手动切
///
/// `default_local_endpoint` / `default_local_model` 当 config 缺该字段时兜底（main.rs 的常量）。
pub fn detect_endpoints(
    default_local_endpoint: &str,
    default_local_model: &str,
) -> DetectedEndpoints {
    // 1. 加载 config.json
    let loaded = load_config_file();
    let (mut cfg, source_hint) = match &loaded {
        Some(l) => (
            l.config.clone(),
            format!("配置来源：{}", l.source_path.display()),
        ),
        None => (
            ConfigFile::default(),
            "未找到 config.json，使用程序默认。可在设置中填入并保存到 U 盘。".to_owned(),
        ),
    };

    // local_endpoint / local_model 缺省时用 main.rs 传进来的默认
    if cfg.local_endpoint.is_empty() {
        cfg.local_endpoint = default_local_endpoint.to_owned();
    }
    if cfg.local_model.is_empty() {
        cfg.local_model = default_local_model.to_owned();
    }

    // 2. env var 覆盖 remote 三项（开发调试场景）
    if let Some(url) = std::env::var("NEUROBOOT_A_ENDPOINT").ok().filter(|s| !s.is_empty()) {
        cfg.remote_endpoint = url;
        if let Some(m) = std::env::var("NEUROBOOT_A_MODEL").ok().filter(|s| !s.is_empty()) {
            cfg.remote_model = m;
        }
        if let Some(k) = std::env::var("NEUROBOOT_A_API_KEY").ok().filter(|s| !s.is_empty()) {
            cfg.remote_api_key = k;
        }
        cfg.remote_label = "云端(env)".to_owned();
    }

    let local = EndpointConfig {
        endpoint: cfg.local_endpoint.clone(),
        model: cfg.local_model.clone(),
        api_key: None,
        label: "本地".to_owned(),
    };

    // 3. 没配 remote 或被禁 prefer_remote
    if !cfg.has_remote() {
        return DetectedEndpoints {
            active: local,
            inactive: None,
            effective: cfg,
            source_hint,
            probe_hint: "未配置在线端点（点齿轮按钮设置）".to_owned(),
        };
    }

    let remote = EndpointConfig {
        endpoint: cfg.remote_endpoint.clone(),
        model: if cfg.remote_model.is_empty() {
            "default".to_owned()
        } else {
            cfg.remote_model.clone()
        },
        api_key: if cfg.remote_api_key.is_empty() {
            None
        } else {
            Some(cfg.remote_api_key.clone())
        },
        label: cfg.remote_label.clone(),
    };

    if !cfg.prefer_remote {
        // 用户禁了优先 remote，但仍允许 UI 切换
        return DetectedEndpoints {
            active: local,
            inactive: Some(remote),
            effective: cfg,
            source_hint,
            probe_hint: "已禁用「优先在线」（手动切换）".to_owned(),
        };
    }

    // 4. 探测 remote 可达性
    let probe_start = std::time::Instant::now();
    let reachable = probe_endpoint(&remote.endpoint, Duration::from_secs(5));
    let elapsed_ms = probe_start.elapsed().as_millis();

    if reachable {
        DetectedEndpoints {
            active: remote,
            inactive: Some(local),
            effective: cfg,
            source_hint,
            probe_hint: format!("在线端点可达 ({} ms)", elapsed_ms),
        }
    } else {
        // 探测失败 —— 用本地，但仍把 remote 放进 inactive 让用户手动切（可能只是 GET 路径不对）
        DetectedEndpoints {
            active: local,
            inactive: Some(remote),
            effective: cfg,
            source_hint,
            probe_hint: format!("在线端点不可达 ({} ms)，自动回退本地", elapsed_ms),
        }
    }
}

/// 探测一个 endpoint 是否可达。
///
/// 优先用 `HEAD /v1/models`（OpenAI 兼容端点通常都有，比 GET 根路径更准）；
/// HEAD 失败时退化用 GET 根路径。
/// 任何 HTTP 响应都算可达（包括 401/404 —— 服务器有响应说明 TCP/DNS/TLS 都通）；
/// 只有 timeout / 连接拒绝 / DNS 失败才算不可达。
fn probe_endpoint(endpoint: &str, timeout: Duration) -> bool {
    let client = match reqwest::blocking::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let trimmed = endpoint.trim_end_matches('/');
    let models_url = format!("{trimmed}/v1/models");
    if client.head(&models_url).send().is_ok() {
        return true;
    }
    client.get(endpoint).send().is_ok()
}

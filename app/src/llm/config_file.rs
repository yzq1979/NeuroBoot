//! 用户配置文件（`config.json`）加载与持久化 —— 端点 / 模型 / API key / 行为开关。
//!
//! 阶段 v1.0.1 起新增：U 盘真测发现 PE 里无法注入环境变量，必须支持配置文件方案。
//!
//! 加载策略（按顺序）：
//! 1. 扫所有非 `X:` 盘符根目录找 `NeuroBoot.config.json` 或 `NeuroBoot\config.json`
//!    —— 用户在主系统编辑后丢到 U 盘 / Ventoy 数据分区根
//! 2. 退化到 `X:\NeuroBoot\config.json`（ISO 内置默认）
//!
//! 优先级：环境变量 > config.json > 程序默认。
//! - 调试场景：开发者用 env var override config，方便切端点测试
//! - 终端用户场景：在 UI 设置面板填 → 写入 U 盘 config.json → 下次启动生效

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 用户配置文件 schema。
///
/// 字段语义：
/// - `remote_endpoint` / `remote_model` / `remote_api_key` / `remote_label`
///   —— 在线 AI 端点（OpenAI / DeepSeek / 通义千问 / 智谱 等 OpenAI 兼容）
/// - `prefer_remote` —— 启动时优先探测 remote；false 则直接用 local 不探测
/// - `local_endpoint` / `local_model` —— 本地 llama-server（通常不改）
/// - `system_prompt_override` —— 非空时覆盖内置 system prompt
///
/// 所有字段都有 `#[serde(default)]`，缺字段时落默认值，向前向后兼容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigFile {
    #[serde(default)]
    pub remote_endpoint: String,
    #[serde(default)]
    pub remote_model: String,
    #[serde(default)]
    pub remote_api_key: String,
    #[serde(default = "default_remote_label")]
    pub remote_label: String,
    #[serde(default = "default_prefer_remote")]
    pub prefer_remote: bool,
    #[serde(default = "default_local_endpoint")]
    pub local_endpoint: String,
    #[serde(default = "default_local_model")]
    pub local_model: String,
    #[serde(default)]
    pub system_prompt_override: Option<String>,
}

fn default_remote_label() -> String {
    "云端".to_owned()
}
fn default_prefer_remote() -> bool {
    true
}
fn default_local_endpoint() -> String {
    "http://127.0.0.1:8080".to_owned()
}
fn default_local_model() -> String {
    "qwen3-4b-instruct".to_owned()
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            remote_endpoint: String::new(),
            remote_model: String::new(),
            remote_api_key: String::new(),
            remote_label: default_remote_label(),
            prefer_remote: default_prefer_remote(),
            local_endpoint: default_local_endpoint(),
            local_model: default_local_model(),
            system_prompt_override: None,
        }
    }
}

impl ConfigFile {
    /// remote_endpoint 是否填了 —— 用来判断是否构造 remote EndpointConfig。
    pub fn has_remote(&self) -> bool {
        !self.remote_endpoint.trim().is_empty()
    }
}

/// 启发式判断一个 model 名是否支持 vision（图片输入）。
///
/// 用关键词匹配 —— OpenAI 兼容生态没有标准的「is_vl」元数据接口，只能从 model name 猜。
/// 假阳性宁多勿少：如果模型名含「vl」「vision」「omni」「gpt-4o/5」「claude-3」「gemini」
/// 「-vl-」「intern」等都视为 VL；用户实测发现不支持时会从 API 报错，不至于严重误事。
///
/// 用于：UI 决定「+ 图片」按钮是否启用 + 切端点时友好提示。
pub fn is_vl_model(model: &str) -> bool {
    let m = model.to_lowercase();
    // 通用关键词
    let keywords = [
        "vl",           // qwen-vl, qwen2-vl, qwen2.5-vl, llava
        "vision",       // gpt-4-vision, claude-3-vision
        "omni",         // gpt-4o, gpt-5 omni
        "multimodal",
        "minicpm-v",
        "intern",       // InternVL, InternLM-XComposer
        "llava",
        "moondream",
        "cogvlm",
        "deepseek-vl",
        "glm-4v",       // 智谱 GLM-4V
        "glm-4.5v",
        "yi-vl",
        "phi-3-vision",
        "phi-4-multimodal",
    ];
    if keywords.iter().any(|k| m.contains(k)) {
        return true;
    }
    // 已知默认含视觉的模型族
    let visual_families = [
        "gpt-4o",       // gpt-4o, gpt-4o-mini
        "gpt-5",        // gpt-5 系列
        "claude-3",     // claude-3-opus/sonnet/haiku 全支持 vision
        "claude-4",
        "claude-opus-4",
        "claude-sonnet-4",
        "claude-haiku-4",
        "gemini-1.5",
        "gemini-2",
        "gemini-pro-vision",
        "pixtral",      // Mistral Pixtral
        "qwen-plus",    // 通义千问 plus 多模态版（部分）
        "qwen-max",
        "qwen-turbo",
    ];
    visual_families.iter().any(|f| m.contains(f))
}

/// 找到 config.json 文件后返回其来源路径（用于 UI 提示「正在用 X:\... 的配置」）。
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: ConfigFile,
    pub source_path: PathBuf,
}

/// 加载 config.json。
///
/// 加载顺序：所有非 X: 盘的 `<root>\NeuroBoot.config.json` →
/// `<root>\NeuroBoot\config.json` → `X:\NeuroBoot\config.json`。
/// 找到第一个 parse 成功的就返回；全部失败返回 None。
pub fn load_config_file() -> Option<LoadedConfig> {
    for path in candidate_paths() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ConfigFile>(&content) {
                Ok(cfg) => {
                    return Some(LoadedConfig {
                        config: cfg,
                        source_path: path,
                    });
                }
                Err(_) => {
                    // 文件存在但 JSON 损坏：跳过继续找；不打 panic
                    continue;
                }
            },
            Err(_) => continue, // 文件不存在或读不到
        }
    }
    None
}

/// 候选 config 路径清单（按优先级排序）。
///
/// 公开是为了 UI 调试时显示「我会在以下位置找 config」。
pub fn candidate_paths() -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(64);
    // 非 X: 盘符：A~Z 跳过 X
    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        out.push(PathBuf::from(format!("{c}:\\NeuroBoot.config.json")));
        out.push(PathBuf::from(format!("{c}:\\NeuroBoot\\config.json")));
    }
    // PE ramdisk 兜底
    out.push(PathBuf::from("X:\\NeuroBoot\\config.json"));
    out
}

/// 把 config 写到第一个可写的非 X: 盘根（`<X>:\NeuroBoot.config.json`）。
///
/// 用于 UI 设置面板的「保存到 U 盘」按钮。返回成功写入的路径，或所有盘都失败时的错误。
pub fn save_to_first_writable_drive(cfg: &ConfigFile) -> Result<PathBuf, String> {
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("序列化 config 失败：{e}"))?;

    for letter in b'A'..=b'Z' {
        if letter == b'X' {
            continue;
        }
        let c = letter as char;
        let drive_root = PathBuf::from(format!("{c}:\\"));
        // 跳过不存在 / 不可访问的盘
        if !drive_root.exists() {
            continue;
        }
        let path = PathBuf::from(format!("{c}:\\NeuroBoot.config.json"));
        match std::fs::write(&path, &json) {
            Ok(_) => return Ok(path),
            Err(_) => continue, // 该盘只读 / 权限不够 → 试下一个
        }
    }
    Err("找不到可写的非 X: 分区。Ventoy 通常会创建一个 exFAT 数据分区，请确认 U 盘是 Ventoy 模式而非纯 ISO 直写。".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trip() {
        let cfg = ConfigFile::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: ConfigFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn missing_fields_get_defaults() {
        // 只填 remote_endpoint，其他全用默认
        let json = r#"{"remote_endpoint":"https://api.deepseek.com"}"#;
        let parsed: ConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.remote_endpoint, "https://api.deepseek.com");
        assert!(parsed.prefer_remote, "prefer_remote 默认应为 true");
        assert_eq!(parsed.local_endpoint, "http://127.0.0.1:8080");
        assert_eq!(parsed.remote_label, "云端");
    }

    #[test]
    fn has_remote_detects_empty_endpoint() {
        let cfg = ConfigFile::default();
        assert!(!cfg.has_remote());

        let cfg = ConfigFile {
            remote_endpoint: "  ".to_owned(),
            ..Default::default()
        };
        assert!(!cfg.has_remote(), "空白字符也算未配置");

        let cfg = ConfigFile {
            remote_endpoint: "https://api.example.com".to_owned(),
            ..Default::default()
        };
        assert!(cfg.has_remote());
    }

    #[test]
    fn vl_detection_known_models() {
        // VL models - 应为 true
        for m in [
            "qwen2.5-vl-7b-instruct",
            "Qwen-VL-Plus",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-5-mini",
            "claude-3-5-sonnet",
            "claude-opus-4-7",
            "gemini-1.5-pro",
            "deepseek-vl2-tiny",
            "glm-4v-flash",
            "InternVL2-8B",
            "llava-1.5-7b",
            "pixtral-12b",
        ] {
            assert!(is_vl_model(m), "expected VL for {}", m);
        }
        // 纯文本模型 - 应为 false
        for m in [
            "qwen3-4b-instruct",
            "qwen3-coder-30b",
            "deepseek-chat",
            "deepseek-r1",
            "gpt-3.5-turbo",
            "llama-3.1-8b",
            "mistral-7b",
        ] {
            assert!(!is_vl_model(m), "expected non-VL for {}", m);
        }
    }

    #[test]
    fn candidate_paths_excludes_x() {
        let paths = candidate_paths();
        // 不应该有 X:\NeuroBoot.config.json（X: 是 PE ramdisk）
        let has_x_root = paths.iter().any(|p| {
            let s = p.to_string_lossy();
            s.starts_with("X:\\NeuroBoot.config")
        });
        assert!(!has_x_root, "X: 根目录不应在候选列表");
        // 但应该有 X:\NeuroBoot\config.json 作为兜底
        let has_x_subdir = paths
            .iter()
            .any(|p| p.to_string_lossy().to_uppercase() == "X:\\NEUROBOOT\\CONFIG.JSON");
        assert!(has_x_subdir, "X: 子目录兜底应在候选列表");
    }
}

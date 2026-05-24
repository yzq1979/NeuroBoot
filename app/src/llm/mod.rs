//! LLM 子模块：OpenAI 兼容数据结构 + 同步 HTTP helper。
//!
//! 阶段 3.2 起 agent loop 接管 worker 线程管理；
//! 这里只暴露 `blocking_chat_completion` 作为 agent 每一步的底层调用。
//! 阶段 4 起会扩展：A/C 端点路由、流式输出。

pub mod client;
pub mod config_file;
pub mod endpoint;
pub mod openai;

//! Dangerous 类工具：有副作用（写盘、删文件、改引导等），
//! Agent 调用前会被 agent loop 拦截走人工确认弹窗。
//!
//! 阶段 4.5：delete_path（删任意路径，含黑名单防整盘）

pub mod delete_path;

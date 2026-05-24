//! Safe 类工具：只读诊断，Agent 可自动执行（无副作用）。
//!
//! 阶段 3.3：list_disks
//! 阶段 3.4：read_system_info + read_event_log_errors

pub mod list_disks;
pub mod read_event_log_errors;
pub mod read_system_info;

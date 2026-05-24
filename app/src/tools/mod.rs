//! 工具子模块：Agent 可调用的诊断/修复工具，按安全等级分类。
//!
//! 阶段 3.1 只搭框架（Tool trait + Registry）；阶段 3.3 起注册具体工具：
//! - safe/list_disks：硬盘清单
//! - safe/read_system_info：系统/硬件摘要
//! - safe/read_event_log_errors：系统日志最近 errors
//!
//! 阶段 4 起加 dangerous/ 子目录（修复/破坏性工具，执行前必须人工确认）。

pub mod audit_log;
pub mod dangerous;
pub mod ps_helper;
pub mod registry;
pub mod safe;

#[allow(unused_imports)] // agent loop 接入后会用上；main.rs 也通过 full path 用
pub use registry::{SafetyClass, Tool, ToolError, ToolOutput, ToolRegistry};

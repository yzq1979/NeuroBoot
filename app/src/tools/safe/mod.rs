//! Safe 类工具：只读诊断，Agent 可自动执行（无副作用）。
//!
//! 阶段 3.3：list_disks
//! 阶段 3.4：read_system_info + read_event_log_errors
//! 阶段 v2 P0：list_partitions / list_volumes / read_ip_config / list_network_adapters /
//!             list_processes_top / list_services / list_minidumps / list_recent_shutdowns

pub mod analyze_minidump;
pub mod extract_archive;
pub mod list_disks;
pub mod list_minidumps;
pub mod list_network_adapters;
pub mod list_partitions;
pub mod list_processes_top;
pub mod list_recent_shutdowns;
pub mod list_services;
pub mod list_volumes;
pub mod load_skill;
pub mod propose_plan;
pub mod read_event_log_errors;
pub mod read_ip_config;
pub mod read_smart;
pub mod read_system_info;

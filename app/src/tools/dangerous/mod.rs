//! Dangerous 类工具：有副作用（写盘、删文件、改引导等），
//! Agent 调用前会被 agent loop 拦截走人工确认弹窗。
//!
//! v1：delete_path（删任意路径，含黑名单防整盘）
//! v2 Stage 4.1：run_chkdsk / run_sfc_scannow / run_dism_restorehealth /
//!               defender_offline_scan / bootrec_rebuild_bcd

pub mod bootrec_rebuild_bcd;
pub mod defender_offline_scan;
pub mod delete_path;
pub mod reset_local_admin_password;
pub mod run_chkdsk;
pub mod run_dism_restorehealth;
pub mod run_sfc;
pub mod testdisk_scan_partition;

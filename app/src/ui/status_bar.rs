//! 状态栏：本地时钟 + 内存占用 + 本地 IP —— 给 PE 真测时常用诊断信息一个一眼可见的入口。
//!
//! 阶段 v1.0.1+ 新增（用户提出「传统 PE 桌面通常有时钟/内存/IP，NeuroBoot 没有」的反馈）。
//!
//! 设计：
//! - **时钟**：每帧重读 `GetLocalTime` 系统调用（轻量到忽略）—— PE RTC 时间可能不准但用户能看
//! - **内存**：`GlobalMemoryStatusEx` Win32 API，缓存 5s 刷新
//! - **本地 IP**：UDP socket trick（connect 8.8.8.8 不真发包，看路由选哪个 src IP），缓存 5s
//! - 全部不引入新依赖（chrono / sysinfo / ipconfig 都避开），保持 PE 兼容 + binary size 小

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use eframe::egui;

/// 本地时钟（年月日时分秒，本地时区）—— Win32 GetLocalTime 的 Rust 镜像。
#[derive(Debug, Clone, Copy)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

impl LocalTime {
    /// 调 GetLocalTime 取当前本地时间。
    pub fn now() -> Self {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLocalTime(lpSystemTime: *mut SystemTime);
        }

        let mut st = SystemTime::default();
        unsafe {
            GetLocalTime(&mut st);
        }
        Self {
            year: st.year,
            month: st.month,
            day: st.day,
            hour: st.hour,
            minute: st.minute,
            second: st.second,
        }
    }

    pub fn format_hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// 物理内存使用 —— GlobalMemoryStatusEx 的 Rust 镜像。
#[derive(Debug, Clone, Copy)]
pub struct MemoryInfo {
    pub used_mb: u64,
    pub total_mb: u64,
    /// 0~100
    pub load_percent: u32,
}

impl MemoryInfo {
    /// 调 GlobalMemoryStatusEx 取内存信息；失败返回 None（不 panic，PE 容错优先）。
    pub fn read() -> Option<Self> {
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            memory_load: u32,
            total_phys: u64,
            avail_phys: u64,
            total_page_file: u64,
            avail_page_file: u64,
            total_virtual: u64,
            avail_virtual: u64,
            avail_extended_virtual: u64,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalMemoryStatusEx(lpBuffer: *mut MemoryStatusEx) -> i32;
        }

        let mut ms = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        let ok = unsafe { GlobalMemoryStatusEx(&mut ms) };
        if ok == 0 {
            return None;
        }
        let total_mb = ms.total_phys / 1024 / 1024;
        let used_mb = (ms.total_phys.saturating_sub(ms.avail_phys)) / 1024 / 1024;
        Some(Self {
            used_mb,
            total_mb,
            load_percent: ms.memory_load,
        })
    }
}

/// 本地 IP —— 用 UDP "connect" 8.8.8.8 看本机用哪个 src IP（无真包发出，路由查询而已）。
///
/// 没有网卡 / 不通网时返回 None；PE 里 wpeinit 之前调可能拿不到 IP（接受）。
pub fn read_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

/// 状态栏缓存 —— 避免每帧调系统调用。
pub struct StatusBarState {
    /// 上次刷新内存/IP 的时间点
    last_refresh: Option<Instant>,
    cached_memory: Option<MemoryInfo>,
    cached_ip: Option<String>,
    refresh_interval: Duration,
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self {
            last_refresh: None,
            cached_memory: None,
            cached_ip: None,
            refresh_interval: Duration::from_secs(5),
        }
    }
}

impl StatusBarState {
    /// 强制刷新一次缓存（启动时调一次让首帧就显示数据）。
    pub fn refresh_now(&mut self) {
        self.cached_memory = MemoryInfo::read();
        self.cached_ip = read_local_ip();
        self.last_refresh = Some(Instant::now());
    }

    fn refresh_if_stale(&mut self) {
        let stale = match self.last_refresh {
            None => true,
            Some(t) => t.elapsed() >= self.refresh_interval,
        };
        if stale {
            self.refresh_now();
        }
    }

    /// 在 ui 上画一行：时钟 · 内存 · IP。
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        self.refresh_if_stale();

        ui.horizontal(|ui| {
            let now = LocalTime::now();
            ui.weak(format!(
                "🕐 {}-{:02}-{:02} {}",
                now.year,
                now.month,
                now.day,
                now.format_hms()
            ));
            ui.weak("·");
            if let Some(mem) = &self.cached_memory {
                ui.weak(format!(
                    "内存 {}/{} MB ({}%)",
                    mem.used_mb, mem.total_mb, mem.load_percent
                ));
            } else {
                ui.weak("内存 ?");
            }
            ui.weak("·");
            match &self.cached_ip {
                Some(ip) if ip != "0.0.0.0" => {
                    ui.weak(format!("本地 IP {ip}"));
                }
                _ => {
                    ui.weak("无网络 / 未配 IP");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_returns_sane_values() {
        let t = LocalTime::now();
        assert!(t.year >= 2024 && t.year <= 2100, "year out of range: {}", t.year);
        assert!(t.month >= 1 && t.month <= 12);
        assert!(t.day >= 1 && t.day <= 31);
        assert!(t.hour <= 23);
        assert!(t.minute <= 59);
        assert!(t.second <= 59);
    }

    #[test]
    fn format_hms_pads_zeros() {
        let t = LocalTime {
            year: 2026,
            month: 5,
            day: 24,
            hour: 8,
            minute: 5,
            second: 3,
        };
        assert_eq!(t.format_hms(), "08:05:03");
    }

    #[test]
    fn memory_info_reads_something() {
        // 主系统一定有内存；测试机不应该出现 read() 失败
        let m = MemoryInfo::read().expect("GlobalMemoryStatusEx 失败");
        assert!(m.total_mb > 0, "total memory should be > 0");
        assert!(m.used_mb <= m.total_mb);
        assert!(m.load_percent <= 100);
    }

    #[test]
    fn refresh_state_caches_within_interval() {
        let mut s = StatusBarState::default();
        s.refresh_now();
        let first_ip = s.cached_ip.clone();
        // 立刻再 refresh_if_stale 不应该真的重新读
        let before = s.last_refresh;
        s.refresh_if_stale();
        assert_eq!(s.last_refresh, before, "应该在 interval 内不刷新");
        assert_eq!(s.cached_ip, first_ip);
    }
}

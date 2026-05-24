//! 工具：list_network_adapters —— 列出所有网卡（含禁用/未连接的）。
//!
//! `Get-NetAdapter -IncludeHidden` 给所有物理 + 虚拟网卡，含状态、MAC、连接速率、驱动型号。
//! 跟 read_ip_config 互补：本工具看「网卡硬件层」，read_ip_config 看「TCP/IP 层」。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ListNetworkAdapters;

impl Tool for ListNetworkAdapters {
    fn name(&self) -> &str {
        "list_network_adapters"
    }

    fn description(&self) -> &str {
        "列所有网络适配器（物理 + 虚拟 + 隐藏 + 禁用）—— 看网卡硬件层。\n\
         \n\
         **When to use**: 用户说「网卡不见了」「Wi-Fi 图标灰显」「装了 VPN 后多了奇怪网卡」「网速很慢」时；\
         判断是网卡硬件 / 驱动问题（Status=Disabled / DriverVersion 异常）还是 IP 配置问题（用 read_ip_config）；\
         检查是否被恶意软件添加了虚拟网卡（用 InterfaceDescription 筛 'TAP'、'VPN' 等关键词）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON 数组，每适配器含：\n\
         - `Name`: 显示名（中文 Windows 是「以太网」「WLAN」等）\n\
         - `Status`: Up / Disconnected / Disabled / NotPresent\n\
         - `MacAddress`: 物理地址（识别厂商 / 重复 MAC 故障用）\n\
         - `LinkSpeed`: 协商速率（如 1 Gbps，**比 hub/线缆理论值低**的话有问题）\n\
         - `InterfaceDescription`: 驱动名（识别 OEM / 虚拟网卡）\n\
         - `MediaType`: 802.3 / Native802.11 / Wireless80211 / 等\n\
         - `DriverVersion`: 驱动版本号（驱动新旧对比用）\n\
         \n\
         **Notes**: 含 `-IncludeHidden`，会看到 VPN / WSL2 / Hyper-V 虚拟网卡。\
         跟 read_ip_config 互补：本工具看「硬件状态」，read_ip_config 看「TCP/IP 配置」。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: &Value) -> ToolOutput {
        let script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
ConvertTo-Json @(Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Select-Object Name, @{N='Status';E={$_.Status.ToString()}}, MacAddress, LinkSpeed, InterfaceDescription, @{N='MediaType';E={$_.MediaType.ToString()}}, DriverVersion) -Depth 3 -Compress"#;

        run_ps_json_array(script)
    }
}

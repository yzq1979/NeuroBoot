//! 工具：read_ip_config —— 列出所有网卡的 IP / MAC / DHCP / DNS 配置。
//!
//! `Get-NetIPConfiguration` 给每个有 IP 的接口一行：InterfaceAlias / InterfaceDescription /
//! IPv4Address / IPv4DefaultGateway / DNSServer / NetAdapter.Status。
//! 是网络故障诊断的第一手数据。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct ReadIpConfig;

impl Tool for ReadIpConfig {
    fn name(&self) -> &str {
        "read_ip_config"
    }

    fn description(&self) -> &str {
        "读所有网卡的 IP / 子网 / 网关 / DNS（ipconfig /all 的结构化版）。\n\
         \n\
         **When to use**: 用户说「连不上网」「Wi-Fi 显示连了但打不开网页」「DNS 错误」「IP 冲突」时；\
         判断网卡有没有拿到 IP（IPv4Address 为空 = DHCP 失败 / 网卡禁用 / 物理断开）；\
         看 DNS 是不是被改成可疑服务器（恶意软件常用招）；\
         判断网关是否正常（在同一子网内）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON 数组，每接口含：\n\
         - `InterfaceAlias`: 用户看到的网卡名（如「以太网」「WLAN」「Realtek PCIe GBE」）\n\
         - `InterfaceDescription`: 驱动层名（更技术化）\n\
         - `IPv4Address`: IP 地址（多 IP 用 ', ' 分隔；空 = 没拿到 IP）\n\
         - `IPv4Mask`: 子网掩码长度（如 24 = /24 = 255.255.255.0）\n\
         - `IPv4Gateway`: 默认网关（空 = 无网关，只能本地通信）\n\
         - `DNSServers`: DNS 服务器列表（**关键安全检查项**：常见污染 = 改成 8.8.8.8 之外的可疑 IP）\n\
         - `Status`: Up / Disconnected / Disabled / Unknown\n\
         \n\
         **Notes**: 只看 IPv4；要看 IPv6 / Wi-Fi 信号强度等，要扩展工具。\
         空数组 = 所有网卡都没启用（PE 里 wpeinit 失败时可能这样）。"
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
ConvertTo-Json @(Get-NetIPConfiguration -ErrorAction SilentlyContinue | ForEach-Object {
    [PSCustomObject]@{
        InterfaceAlias = $_.InterfaceAlias
        InterfaceDescription = $_.InterfaceDescription
        IPv4Address = ($_.IPv4Address | ForEach-Object { $_.IPAddress }) -join ', '
        IPv4Mask = ($_.IPv4Address | ForEach-Object { $_.PrefixLength.ToString() }) -join ', '
        IPv4Gateway = ($_.IPv4DefaultGateway | ForEach-Object { $_.NextHop }) -join ', '
        DNSServers = ($_.DNSServer | Where-Object { $_.AddressFamily -eq 2 } | ForEach-Object { $_.ServerAddresses -join ', ' }) -join '; '
        Status = if ($_.NetAdapter) { $_.NetAdapter.Status.ToString() } else { 'Unknown' }
    }
}) -Depth 4 -Compress"#;

        run_ps_json_array(script)
    }
}

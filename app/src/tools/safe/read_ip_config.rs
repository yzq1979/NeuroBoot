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
        "读取本机所有网卡的 IP / 网关 / DNS 配置（ipconfig /all 的结构化版）。\
         无参数。返回 JSON 数组，每条接口含 InterfaceAlias / InterfaceDescription / \
         IPv4Address / IPv4Mask / IPv4Gateway / DNSServers / Status 字段。\
         适合诊断「连不上网」「DNS 错」「拿不到 IP」类问题。"
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

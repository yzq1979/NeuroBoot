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
        "列出本机所有网络适配器（物理网卡 + 虚拟网卡 + 蓝牙/Wi-Fi/有线/无线），\
         含已禁用的。返回 JSON 数组，每条含 Name/Status (Up/Disconnected/Disabled)/\
         MacAddress/LinkSpeed/InterfaceDescription/MediaType/DriverVersion 字段。\
         适合诊断「网卡不见了」「Wi-Fi 灰显」「驱动有问题」类。"
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

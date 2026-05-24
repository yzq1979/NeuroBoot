//! 工具：read_system_info —— 读取系统/硬件摘要。
//!
//! 用 WMI（CimInstance）查询 OS / ComputerSystem / Processor / BIOS；
//! 返回 JSON object 含主要诊断字段。比 `Get-ComputerInfo` 快很多（毫秒级），
//! PE 里 WMI 服务通常可用，跨主系统/PE 一致。

use std::process::Command;

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolOutput};

pub struct ReadSystemInfo;

impl Tool for ReadSystemInfo {
    fn name(&self) -> &str {
        "read_system_info"
    }

    fn description(&self) -> &str {
        "读取本机的系统与硬件摘要信息（OS 版本、CPU、RAM、主板、BIOS、最后启动时间等）。\
         无参数。返回 JSON object，含 OsName / OsVersion / OsBuild / OsArchitecture / \
         OsInstallDate / LastBoot / Manufacturer / Model / TotalMemoryGB / CpuName / \
         CpuCores / CpuLogicalProcessors / BiosVendor / BiosVersion 字段。"
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
        // [ordered]@{} 保证 JSON 字段顺序稳定（不加默认按 hashtable 内部排序，飘）
        let ps_script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$cs = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue
$cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
$bios = Get-CimInstance Win32_BIOS -ErrorAction SilentlyContinue
$info = [ordered]@{
    OsName = $os.Caption
    OsVersion = $os.Version
    OsBuild = $os.BuildNumber
    OsArchitecture = $os.OSArchitecture
    OsInstallDate = $os.InstallDate.ToString('yyyy-MM-dd')
    LastBoot = $os.LastBootUpTime.ToString('yyyy-MM-dd HH:mm:ss')
    Manufacturer = $cs.Manufacturer
    Model = $cs.Model
    TotalMemoryGB = [math]::Round($cs.TotalPhysicalMemory/1GB, 1)
    CpuName = $cpu.Name
    CpuCores = $cpu.NumberOfCores
    CpuLogicalProcessors = $cpu.NumberOfLogicalProcessors
    BiosVendor = $bios.Manufacturer
    BiosVersion = $bios.SMBIOSBIOSVersion
}
ConvertTo-Json $info -Depth 3 -Compress"#;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                ps_script,
            ])
            .output()
            .map_err(|e| ToolError::new(format!("启动 powershell.exe 失败：{e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::new(format!(
                "Get-CimInstance 失败 (exit {}):\n{}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            return Ok("{}".to_owned());
        }
        Ok(stdout)
    }
}

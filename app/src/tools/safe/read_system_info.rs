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
        "读取本机系统与硬件摘要：OS 版本 / CPU / RAM / 主板 / BIOS / 最后启动时间。\n\
         \n\
         **When to use**: 用户问「我的电脑是什么配置」「Windows 是什么版本」「最后什么时候开机的」时；\
         诊断兼容性问题（驱动 / 软件 / Windows 功能）要先知道 OS 版本和架构；\
         判断硬件老旧程度（看 BIOS 年份 + CPU 型号）。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: JSON object，字段顺序稳定：\n\
         - `OsName` / `OsVersion` / `OsBuild` / `OsArchitecture` / `OsInstallDate`: Windows 元数据\n\
         - `LastBoot`: 最后启动时间（异常关机后这个时间会跳变，可对比 list_recent_shutdowns）\n\
         - `Manufacturer` / `Model`: 整机品牌 + 型号（如 LENOVO / ThinkPad X1 Carbon Gen 9）\n\
         - `TotalMemoryGB`: 物理内存总量\n\
         - `CpuName` / `CpuCores` / `CpuLogicalProcessors`\n\
         - `BiosVendor` / `BiosVersion`: BIOS 厂商和版本\n\
         \n\
         **Example output**: `{\"OsName\":\"Microsoft Windows 11 Pro\",\"OsVersion\":\"10.0.26200\",\
         \"OsBuild\":\"26200\",\"OsArchitecture\":\"64-bit\",\"OsInstallDate\":\"2026-03-15\",\
         \"LastBoot\":\"2026-05-24 08:00:01\",\"Manufacturer\":\"LENOVO\",\"Model\":\"ThinkPad X1 Carbon Gen 9\",\
         \"TotalMemoryGB\":32.0,\"CpuName\":\"Intel(R) Core(TM) Ultra 7 255H\",\"CpuCores\":16,\
         \"CpuLogicalProcessors\":22,\"BiosVendor\":\"LENOVO\",\"BiosVersion\":\"N40ET12W\"}`\n\
         \n\
         **Notes**: PE 里 WMI 服务通常可用，跨主系统/PE 一致；查询是毫秒级。\
         返回 `{{}}` 表示 WMI 失败（极少见）。"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ReadSystemInfo);
    }
}

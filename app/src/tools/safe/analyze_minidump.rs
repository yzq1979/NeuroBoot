//! [Safe] analyze_minidump —— 通过 NirSoft BlueScreenView 解析 BSOD dump 文件。
//!
//! v3 Quick Win 3。现有 `list_minidumps` 只列文件名；用户最痛的是「告诉我**为什么**蓝屏」。
//! BlueScreenView (~83 KB portable, freeware) 用 `/scomma` CLI 输出 CSV，含：
//! - dump filename / crash time
//! - bug check string + code（如 "DRIVER_IRQL_NOT_LESS_OR_EQUAL" 0x000000D1）
//! - parameter 1~4
//! - **caused by driver / caused by address**（关键诊断字段）
//! - file description / product name / company / file version

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct AnalyzeMinidump;

fn find_bluescreenview() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"X:\NeuroBoot\tools\BlueScreenView\BlueScreenView.exe"),
        PathBuf::from(r"C:\NeuroBoot\tools\BlueScreenView\BlueScreenView.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

impl Tool for AnalyzeMinidump {
    fn name(&self) -> &str {
        "analyze_minidump"
    }

    fn description(&self) -> &str {
        "**[高价值]** 解析 BSOD minidump 文件，提取真正的崩溃原因（driver / bug check code / 参数）—— 比 list_minidumps 进一步。\n\
         \n\
         **When to use**: 用户说「最近频繁蓝屏」「想知道是哪个驱动导致的」；\
         list_minidumps 看到有 dump 后的**下一步**自然调用；BSOD 诊断的核心。\n\
         \n\
         **Parameters**:\n\
         - `dump_dir` (string, optional): minidump 目录。默认 `C:\\Windows\\Minidump`\n\
         - `max_dumps` (integer, optional, 默认 5): 最多解析几个 dump（每个 ~100 ms）\n\
         \n\
         **Returns**: JSON 数组（按时间从新到旧），每个 dump 含：\n\
         - `DumpFile`: 文件名\n\
         - `CrashTime`: 崩溃时间\n\
         - `BugCheckString`: 助记符（如 `DRIVER_IRQL_NOT_LESS_OR_EQUAL`）—— **核心诊断字段**\n\
         - `BugCheckCode`: 16 进制码（如 `0x000000D1`）\n\
         - `Parameter1` ~ `Parameter4`: 调试参数\n\
         - `CausedByDriver`: 罪魁驱动文件（如 `nvlddmkm.sys`）—— **第一手线索**\n\
         - `CausedByAddress`: 崩溃地址\n\
         - `FileDescription` / `Company`: 驱动厂商\n\
         \n\
         **Example output**（截选 8 个核心字段）: `[{\"DumpFile\":\"052426-12345-01.dmp\",\
         \"CrashTime\":\"5/24/2026 2:32:11 PM\",\"BugCheckString\":\"DRIVER_IRQL_NOT_LESS_OR_EQUAL\",\
         \"BugCheckCode\":\"0x000000d1\",\"CausedByDriver\":\"nvlddmkm.sys\",\
         \"FileDescription\":\"NVIDIA Windows Kernel Mode Driver\",\"Company\":\"NVIDIA Corporation\",\
         \"FileVersion\":\"32.0.15.6094\"}]`\n\
         \n\
         **Notes**: BlueScreenView.exe 默认不在 NeuroBoot ISO；按 docs/BUILD.md 「v3 Quick Win 工具下载」节\
         下载（~83 KB freeware，非商用 / 公开分发要注意 license）放到 X:\\NeuroBoot\\tools\\BlueScreenView\\；\
         **关键关联**：BugCheckString = `DRIVER_*` 时看 `CausedByDriver`（驱动问题）；\
         `IRQL_NOT_LESS_OR_EQUAL` + 厂商显卡驱动 = 显卡驱动崩溃；\
         `MEMORY_MANAGEMENT` = 内存问题；`PAGE_FAULT_IN_NONPAGED_AREA` = 内存或驱动。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "dump_dir": {
                    "type": "string",
                    "description": "minidump 目录，默认 C:\\Windows\\Minidump"
                },
                "max_dumps": {
                    "type": "integer",
                    "description": "最多解析几个 dump（每个 ~100 ms）",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let dump_dir = args
            .get("dump_dir")
            .and_then(Value::as_str)
            .unwrap_or(r"C:\Windows\Minidump")
            .to_owned();
        let max_dumps = args
            .get("max_dumps")
            .and_then(Value::as_i64)
            .unwrap_or(5)
            .clamp(1, 20);

        if !Path::new(&dump_dir).exists() {
            return Ok(format!(
                "(dump 目录不存在) {} —— 可能：① 该机器没崩过 ② Minidump 写入被禁用（控制面板「启动和故障恢复」可查）",
                dump_dir
            ));
        }

        let exe = find_bluescreenview().ok_or_else(|| {
            ToolError::with_kind(
                ToolErrorKind::NotFound,
                "BlueScreenView.exe 未找到。NeuroBoot 默认 ISO 不带 BlueScreenView；\
                 按 docs/BUILD.md 「v3 Quick Win 工具下载」节下载（~83 KB freeware）放到 \
                 X:\\NeuroBoot\\tools\\BlueScreenView\\ 后再试。",
            )
        })?;

        // BlueScreenView /scomma <output.csv> /MiniDumpFolder <dump_dir>
        // 写到临时 CSV 然后读回 parse
        let csv_path = std::env::temp_dir().join(format!(
            "neuroboot-bsv-{}.csv",
            std::process::id()
        ));

        let status = Command::new(&exe)
            .args([
                "/scomma",
                csv_path.to_string_lossy().as_ref(),
                "/MiniDumpFolder",
                &dump_dir,
            ])
            .status()
            .map_err(|e| ToolError::new(format!("启动 BlueScreenView 失败：{e}")))?;

        if !status.success() {
            return Err(ToolError::with_kind(
                ToolErrorKind::ExternalCommandFailed,
                format!(
                    "BlueScreenView 退出码 {}",
                    status.code().unwrap_or(-1)
                ),
            ));
        }

        let csv = std::fs::read_to_string(&csv_path).map_err(|e| {
            ToolError::new(format!(
                "BlueScreenView CSV 输出 {} 读不到：{e}",
                csv_path.display()
            ))
        })?;
        let _ = std::fs::remove_file(&csv_path);

        if csv.trim().is_empty() {
            return Ok("[]".to_owned()); // 目录里没 dump 或全 parse 失败
        }

        // 解析 CSV：字段顺序固定（BlueScreenView /scomma spec）
        // DumpFile, CrashTime, BugCheckString, BugCheckCode, Parameter1, Parameter2,
        // Parameter3, Parameter4, CausedByDriver, CausedByAddress, FileDescription,
        // ProductName, Company, FileVersion, Processor, CrashAddress, StackAddress1,
        // StackAddress2, StackAddress3, ComputerName, FullPath, ProcessorsCount,
        // MajorVersion, MinorVersion, DumpFileSize, DumpFileTime
        let mut entries: Vec<Value> = Vec::new();
        for line in csv.lines().take(max_dumps as usize) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 15 {
                continue; // 行损坏
            }
            entries.push(json!({
                "DumpFile": fields[0].trim().trim_matches('"'),
                "CrashTime": fields[1].trim().trim_matches('"'),
                "BugCheckString": fields[2].trim().trim_matches('"'),
                "BugCheckCode": fields[3].trim().trim_matches('"'),
                "Parameter1": fields[4].trim().trim_matches('"'),
                "Parameter2": fields[5].trim().trim_matches('"'),
                "Parameter3": fields[6].trim().trim_matches('"'),
                "Parameter4": fields[7].trim().trim_matches('"'),
                "CausedByDriver": fields[8].trim().trim_matches('"'),
                "CausedByAddress": fields[9].trim().trim_matches('"'),
                "FileDescription": fields[10].trim().trim_matches('"'),
                "ProductName": fields[11].trim().trim_matches('"'),
                "Company": fields[12].trim().trim_matches('"'),
                "FileVersion": fields[13].trim().trim_matches('"'),
                "Processor": fields[14].trim().trim_matches('"'),
            }));
        }

        Ok(serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&AnalyzeMinidump);
    }
}

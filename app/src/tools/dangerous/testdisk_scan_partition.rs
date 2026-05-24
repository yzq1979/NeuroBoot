//! [DANGEROUS] testdisk_scan_partition —— 用 TestDisk 扫坏 / 误删分区表。
//!
//! TestDisk (CGSecurity) 是 PE 救援盘的另一旗舰工具：分区表损坏 / 误删分区后扫描重建。
//! testdisk_win.exe ~3 MB portable，GPL v2+，可商用。
//!
//! 默认不在 NeuroBoot ISO；按 docs/BUILD.md 下载放 `X:\NeuroBoot\tools\testdisk\testdisk_win.exe`。
//!
//! Dangerous 因为：扫描本身 read-only，但 TestDisk 的 UI 进一步可以「重写分区表」，
//! 用户在 TestDisk UI 里点错可能毁分区表。所以走 confirmation。

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct TestdiskScanPartition;

fn find_testdisk() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"X:\NeuroBoot\tools\testdisk\testdisk_win.exe"),
        PathBuf::from(r"C:\NeuroBoot\tools\testdisk\testdisk_win.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

impl Tool for TestdiskScanPartition {
    fn name(&self) -> &str {
        "testdisk_scan_partition"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS, 救援旗舰] 启动 TestDisk 扫坏 / 误删的分区表** —— `testdisk_win.exe`。\n\
         \n\
         **When to use**: 用户报告：\n\
         - 「分区不见了」「D 盘消失」\n\
         - 「装别的系统后旧分区没了」\n\
         - list_partitions 报告分区数比用户记忆少\n\
         - GPT 头损坏 / 备份头不一致\n\
         \n\
         **When NOT to use**: 只是文件丢失（用 winfr / PhotoRec 单文件恢复）；\
         分区一切正常只是想看看（用 list_partitions）。\n\
         \n\
         **Parameters**: 无（TestDisk 启动后自身 UI 让用户选盘 + 操作）。\n\
         \n\
         **Returns**: 启动确认信息。**实际操作在 TestDisk 自身 TUI 里完成**。\n\
         \n\
         **Notes**: TestDisk 启动后进 TUI（text UI），按提示选盘 → 选分区表类型 → Analyse → \
         Deeper Search。**全程只扫**，扫完显示找到的分区供用户选「Write」（这一步会改分区表，不可逆）。\
         **强烈建议**：操作前先备份当前损坏盘的完整镜像（用 ddrescue / wbadmin）。\
         testdisk_win.exe 默认不在 NeuroBoot ISO；按 docs/BUILD.md 下载（~3 MB GPL 可商用）。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Dangerous
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: &Value) -> ToolOutput {
        let exe = find_testdisk().ok_or_else(|| {
            ToolError::with_kind(
                ToolErrorKind::NotFound,
                "testdisk_win.exe 未找到。NeuroBoot 默认 ISO 不带 TestDisk；\
                 请按 docs/BUILD.md 「救援工具下载」节下载（~3 MB GPL 可商用）放到 \
                 X:\\NeuroBoot\\tools\\testdisk\\ 后再试。",
            )
        })?;

        Command::new(&exe)
            .spawn()
            .map_err(|e| ToolError::new(format!("启动 testdisk_win.exe 失败：{e}")))?;

        Ok("已启动 TestDisk TUI 在新窗口。操作指引：\n\
            1. Create new log（默认）\n\
            2. 选要扫的物理硬盘\n\
            3. 选分区表类型（一般 Intel/PC 或 EFI GPT）\n\
            4. Analyse → Quick Search\n\
            5. 找到分区后按 P 看文件确认是对的\n\
            6. 没找到的话 Deeper Search\n\
            7. 确认无误后 Write（不可逆，**强烈建议先 ddrescue 备份**）"
            .to_owned())
    }
}

//! [DANGEROUS, 低风险] defender_offline_scan —— Microsoft Defender 离线全盘扫。
//!
//! `MpCmdRun.exe -Scan -ScanType 2 -BootSectorScan` —— 全盘 + 引导扇区扫；
//! 在 PE 里跑相比在线扫的优势：① 不会被 user-mode rootkit 干扰；② 能扫到锁定文件。
//!
//! 填补 ESET/Norton/Bitdefender 救援盘 EOL 后的空白（见 docs/RESEARCH-2026-05.md 第六节）。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct DefenderOfflineScan;

impl Tool for DefenderOfflineScan {
    fn name(&self) -> &str {
        "defender_offline_scan"
    }

    fn description(&self) -> &str {
        "**[DANGEROUS, 低风险] Microsoft Defender 全盘 + 引导扇区扫** —— `MpCmdRun.exe -Scan -ScanType 2 -BootSectorScan`。\n\
         \n\
         **When to use**: 用户怀疑中毒（卡顿 + 风扇高速转 + 进程异常）；安装可疑软件后；\
         访问可疑网站 / 邮件附件后；从在线扫不到但仍可疑时（PE 里离线扫能绕过 rootkit）。\n\
         \n\
         **Why PE-friendly**: 离线扫 vs 在线扫的关键优势 —— rootkit 在线时 hook 系统隐藏自己，\
         PE 启动时 rootkit 进程不在运行，因此 Defender 能看到真实文件；锁定文件也能扫。\n\
         \n\
         **Parameters**: 无。\n\
         \n\
         **Returns**: MpCmdRun.exe stdout（扫描进度 + 命中列表 + 处理动作）。\n\
         \n\
         **Notes**: 全盘扫耗时几十分钟到几小时；扫到威胁会自动隔离（默认行为）；\
         若 `MpCmdRun.exe` 不在 PATH（PE 里 Defender 没启用时），会返回 NotFound 错误；\
         注意 Defender 签名库可能不是最新（PE 没自动更新），可先调 `defender_update_signatures`（v2.x 加）。"
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
        // 找 MpCmdRun.exe 路径（不在默认 PATH）
        let probe = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$mpCmd = Get-ChildItem 'C:\Program Files\Windows Defender' -Filter 'MpCmdRun.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $mpCmd) {
    $mpCmd = Get-ChildItem 'C:\ProgramData\Microsoft\Windows Defender\Platform' -Filter 'MpCmdRun.exe' -Recurse -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if ($null -eq $mpCmd) { throw 'MpCmdRun.exe not found' }
& $mpCmd.FullName -Scan -ScanType 2 -BootSectorScan"#;
        run_ps(probe).map_err(|e| {
            if e.message.contains("not found") {
                ToolError::with_kind(
                    ToolErrorKind::NotFound,
                    "MpCmdRun.exe 未找到。可能：(1) PE 里 Defender 没装；(2) Defender 被卸了。\
                     这种情况 NeuroBoot 不能直接调离线扫；建议用户从主系统跑或换 Kaspersky Rescue Disk。"
                        .to_owned(),
                )
            } else {
                e
            }
        })
    }
}

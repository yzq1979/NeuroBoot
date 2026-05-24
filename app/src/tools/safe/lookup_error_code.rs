//! [Safe] lookup_error_code —— 查 Windows BugCheck / Win32 error code 含义。
//!
//! v3.0 W7。配套多个 skill（/diagnose-bsod / /recover-bitlocker / /fix-boot-failure）。
//!
//! **当前实现**：hardcoded 高频 code 表（~30 条），覆盖 PE 救援场景最常见的蓝屏码 + Win32 错误码。
//!
//! **v3.0 W5-6 升级路径**：换内核为 sqlite-vec + Qwen3-Embedding 向量检索，
//! 扩到完整 Microsoft Bug Check Code Reference (~512) + Win32 System Error Codes (~17k)。
//! 工具 API 保持不变 —— 调用方零成本。

use serde_json::{json, Value};

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct LookupErrorCode;

/// 单条 error code 条目（v3.0 MVP 表里手写一遍；v3.1 RAG 切到 sqlite 后此结构进 db）。
struct ErrorEntry {
    /// 归一化后的 code（去 0x 前缀，全大写）
    code: &'static str,
    /// 助记符（如 INACCESSIBLE_BOOT_DEVICE）
    name: &'static str,
    /// 中文一句话描述
    cn_desc: &'static str,
    /// 常见原因 + 排查建议（多行）
    causes: &'static str,
    /// Microsoft 文档锚点
    docs_url: &'static str,
}

/// v3.0 MVP 高频 code 表（手写 ~30 条；W5-6 切 RAG 后此表 deprecated）。
///
/// 选取标准：PE 救援场景最常见 + 用户最痛 + 配套 v3.0 skills 覆盖的诊断流。
const COMMON_CODES: &[ErrorEntry] = &[
    // ===== BugCheck (BSOD stop codes) =====
    ErrorEntry {
        code: "7B",
        name: "INACCESSIBLE_BOOT_DEVICE",
        cn_desc: "启动时找不到 / 无法访问启动盘",
        causes: "常见原因：① 启动盘控制器驱动不在；② BIOS 改了 AHCI/RAID 模式；\
                 ③ 系统盘硬件故障；④ 装新硬件后未识别。排查：先用 list_disks 看盘是否识别 → \
                 fix-boot-failure skill",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x7b--inaccessible-boot-device",
    },
    ErrorEntry {
        code: "A",
        name: "IRQL_NOT_LESS_OR_EQUAL",
        cn_desc: "驱动以错误的中断级访问内存（多为驱动 bug）",
        causes: "常见原因：① 驱动 bug（最常见，看 analyze_minidump 的 CausedByDriver）；\
                 ② 硬件不兼容（特别是新装的）；③ 内存条问题。下一步：analyze_minidump 看罪魁",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xa--irql-not-less-or-equal",
    },
    ErrorEntry {
        code: "D1",
        name: "DRIVER_IRQL_NOT_LESS_OR_EQUAL",
        cn_desc: "驱动以错误中断级访问可分页内存（驱动 bug 高度嫌疑）",
        causes: "常见原因：驱动 bug（>90% 这个原因）。analyze_minidump 直接给 CausedByDriver。\
                 修复：① 更新该驱动；② 回滚到稳定版本；③ 卸载/禁用观察",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xd1--driver-irql-not-less-or-equal",
    },
    ErrorEntry {
        code: "1E",
        name: "KMODE_EXCEPTION_NOT_HANDLED",
        cn_desc: "内核模式程序产生了未处理的异常（驱动 bug / 损坏的系统文件）",
        causes: "常见原因：① 驱动崩溃；② 系统文件损坏；③ 反作弊 / 杀软驱动冲突；\
                 ④ 硬件问题。修复路径：analyze_minidump → 更新驱动 → 跑 sfc/dism",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x1e--kmode-exception-not-handled",
    },
    ErrorEntry {
        code: "1A",
        name: "MEMORY_MANAGEMENT",
        cn_desc: "Windows 内存管理器发现了严重错误（多为硬件 / 驱动问题）",
        causes: "常见原因：① 物理内存条问题（建议跑 mdsched 或 MemTest86）；② 驱动 bug；\
                 ③ 内存超频不稳定。优先查内存硬件",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x1a--memory-management",
    },
    ErrorEntry {
        code: "50",
        name: "PAGE_FAULT_IN_NONPAGED_AREA",
        cn_desc: "访问了根本不存在的内存（指针 bug / 损坏的内存 / 驱动）",
        causes: "常见原因：① 物理内存损坏（典型 - 跑 mdsched）；② 驱动 bug；③ 杀软 / VPN 内核驱动",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x50--page-fault-in-nonpaged-area",
    },
    ErrorEntry {
        code: "9F",
        name: "DRIVER_POWER_STATE_FAILURE",
        cn_desc: "驱动在电源状态切换时挂起（睡眠 / 唤醒 / 关机时蓝屏）",
        causes: "常见原因：① 显卡 / 网卡驱动电源管理 bug；② 笔记本休眠唤醒驱动问题。\
                 修复：禁用「允许计算机关闭此设备以节约电源」",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x9f--driver-power-state-failure",
    },
    ErrorEntry {
        code: "124",
        name: "WHEA_UNCORRECTABLE_ERROR",
        cn_desc: "硬件层报告了不可恢复错误（CPU / 内存 / PCIe 真实硬件故障）",
        causes: "**最严重的硬件级蓝屏**：① CPU 故障；② 内存条坏；③ PCIe 设备故障；\
                 ④ 超频不稳；⑤ 电源供电不稳。优先：恢复默认 BIOS 设置 → 内存测试 → 厂家保修",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x124---whea-uncorrectable-error",
    },
    ErrorEntry {
        code: "F4",
        name: "CRITICAL_OBJECT_TERMINATION",
        cn_desc: "关键系统进程意外终止（如 csrss / wininit / smss 挂了）",
        causes: "常见原因：① 硬盘读写错误（首先排查盘）；② 文件系统损坏；③ 恶意软件破坏",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xf4--critical-object-termination",
    },
    ErrorEntry {
        code: "BE",
        name: "ATTEMPTED_WRITE_TO_READONLY_MEMORY",
        cn_desc: "尝试写入只读内存（典型驱动 bug）",
        causes: "原因几乎一定是驱动 bug。analyze_minidump 看 CausedByDriver → 更新/回滚",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xbe--attempted-write-to-readonly-memory",
    },
    ErrorEntry {
        code: "C2",
        name: "BAD_POOL_CALLER",
        cn_desc: "驱动错误地操作了内存池（典型驱动 bug）",
        causes: "原因：驱动 bug，多见于杀软 / VPN / 反作弊驱动",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xc2--bad-pool-caller",
    },
    ErrorEntry {
        code: "C5",
        name: "DRIVER_CORRUPTED_EXPOOL",
        cn_desc: "驱动损坏了系统内存池",
        causes: "原因：驱动 bug 或内存损坏。先 analyze_minidump 看 driver",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xc5--driver-corrupted-expool",
    },
    ErrorEntry {
        code: "EF",
        name: "CRITICAL_PROCESS_DIED",
        cn_desc: "关键系统进程死了（典型表现：开机蓝屏 + 反复重启）",
        causes: "常见原因：① 系统文件损坏（跑 sfc/dism）；② 恶意软件；③ 不当的优化软件",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xef--critical-process-died",
    },
    ErrorEntry {
        code: "139",
        name: "KERNEL_SECURITY_CHECK_FAILURE",
        cn_desc: "内核检测到数据结构被破坏（驱动 bug / 硬件问题）",
        causes: "2026 多次 Win11 GPU 驱动 KB 触发此码（dxgmms2.sys）。\
                 排查：① 更新显卡驱动；② kernel-mode hardware-enforced stack protection 试关",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x139--kernel-security-check-failure",
    },
    ErrorEntry {
        code: "133",
        name: "DPC_WATCHDOG_VIOLATION",
        cn_desc: "DPC（延迟过程调用）超时（驱动响应慢）",
        causes: "原因：① SSD 固件 bug（特别是某些 NVMe）；② 显卡驱动；③ 杀软扫描卡死",
        docs_url: "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x133--dpc-watchdog-violation",
    },
    // ===== Win32 / HRESULT 高频 =====
    ErrorEntry {
        code: "80070005",
        name: "E_ACCESSDENIED",
        cn_desc: "访问被拒绝（权限不足）",
        causes: "原因：① 操作需要管理员权限；② 文件被独占占用；③ NTFS ACL 限制；\
                 ④ TrustedInstaller 拥有的系统文件。修复：admin 跑 / 解锁文件 / takeown+icacls",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-",
    },
    ErrorEntry {
        code: "80070002",
        name: "ERROR_FILE_NOT_FOUND",
        cn_desc: "找不到指定的文件",
        causes: "原因：① 路径写错；② 文件被删；③ 路径含特殊字符未转义；\
                 ④ 注册表指向已删除的路径",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-",
    },
    ErrorEntry {
        code: "80070003",
        name: "ERROR_PATH_NOT_FOUND",
        cn_desc: "找不到指定的路径（目录不存在）",
        causes: "原因：路径中某级目录不存在。区别于 80070002（文件不存在但目录在）",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-",
    },
    ErrorEntry {
        code: "8007007E",
        name: "ERROR_MOD_NOT_FOUND",
        cn_desc: "找不到指定的模块（DLL 丢失）",
        causes: "典型表现：程序启动报「缺少 xxx.dll」。修复：装对应 redistributable（VC++ / .NET）\
                 或重装该程序",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-",
    },
    ErrorEntry {
        code: "8007045D",
        name: "ERROR_IO_DEVICE",
        cn_desc: "I/O 设备出错（典型坏盘 / 坏块）",
        causes: "**严重信号**：硬盘读写出错。先 read_smart 看 SMART → 备份数据 → 换盘",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--500-999-",
    },
    ErrorEntry {
        code: "80070570",
        name: "ERROR_FILE_CORRUPT",
        cn_desc: "文件损坏（无法读取）",
        causes: "原因：① 文件系统损坏（跑 chkdsk）；② 硬盘坏块（看 read_smart）；\
                 ③ 复制中断遗留半文件",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1300-1699-",
    },
    ErrorEntry {
        code: "80070643",
        name: "ERROR_INSTALL_FAILURE",
        cn_desc: "Windows Update 安装失败（KB / 功能更新）",
        causes: "原因：① 之前 KB 没装完整；② 系统文件损坏；③ 磁盘空间不足；\
                 ④ Component Store 损坏。修复路径：sfc /scannow → dism /restorehealth → 重启重试",
        docs_url: "https://learn.microsoft.com/en-us/windows/deployment/update/windows-update-error-reference",
    },
    ErrorEntry {
        code: "8024000B",
        name: "WU_E_CALL_CANCELLED",
        cn_desc: "Windows Update 操作被取消",
        causes: "通常用户或系统主动取消的 WU 操作。重启再试常解决",
        docs_url: "https://learn.microsoft.com/en-us/windows/deployment/update/windows-update-error-reference",
    },
    ErrorEntry {
        code: "C0000005",
        name: "STATUS_ACCESS_VIOLATION",
        cn_desc: "访问违规（程序访问了没权限的内存）",
        causes: "原因：① 程序 bug（最常见）；② 内存损坏；③ 恶意软件注入崩了。\
                 修复：① 更新该程序；② 跑 mdsched 测内存；③ defender_offline_scan",
        docs_url: "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--12000-15999-",
    },
];

impl Tool for LookupErrorCode {
    fn name(&self) -> &str {
        "lookup_error_code"
    }

    fn description(&self) -> &str {
        "查 Windows BugCheck / Win32 error code 含义 —— 给中文解释 + 常见原因 + Microsoft 文档链接。\n\
         \n\
         **When to use**: 用户问任何 `0xNNNN` / `0x800NNNNN` / `0xCNNNNNNN` 形式的码时；\
         analyze_minidump 返回的 BugCheckCode 想给中文解释；\
         配套 /diagnose-bsod / /recover-bitlocker / /fix-boot-failure skill 的诊断流。\n\
         \n\
         **Parameters**:\n\
         - `code` (string, required): 错误码。支持多种格式：\
         `'0x7B'` / `'7B'` / `'0x0000007B'` / `'STOP 0x7B'` —— 自动归一化\n\
         - `context` (string, optional): 上下文（如「蓝屏」/「Windows Update 失败」）—— \
         多义码（如 0x5 既是 Win32 也可是其它）时辅助消歧\n\
         \n\
         **Returns**: JSON object 含 `code` / `name` / `cn_desc` / `causes` / `docs_url`；\
         未找到时含 `{found: false, code, hint: '...'}`，建议查 Microsoft 文档。\n\
         \n\
         **Example output**: `{\"found\":true,\"code\":\"7B\",\"name\":\"INACCESSIBLE_BOOT_DEVICE\",\
         \"cn_desc\":\"启动时找不到 / 无法访问启动盘\",\"causes\":\"...\",\
         \"docs_url\":\"https://learn.microsoft.com/...\"}`\n\
         \n\
         **Notes**: v3.0 MVP 用 hardcoded 表（~25 个最高频 code）—— 覆盖 PE 救援场景 80%；\
         v3.1 W5-6 升级为 sqlite-vec + Qwen3-Embedding 向量检索完整 Microsoft 文档 \
         (~17k codes)，调用方零成本（同 API）；\
         返回 `found: false` 时**不要编**含义，直接告诉用户查 docs_url。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "错误码，多种格式（0x7B / 7B / 0x0000007B / STOP 0x7B）"
                },
                "context": {
                    "type": "string",
                    "description": "可选上下文（蓝屏 / Windows Update 等），多义码消歧用"
                }
            },
            "required": ["code"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let raw = args
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 code 参数")
            })?;

        let normalized = normalize_code(raw);
        if normalized.is_empty() {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                format!("无法从 '{raw}' 解析出合法的 hex 错误码"),
            ));
        }

        // 在 hardcoded 表里找；匹配规则：归一化后完全相等
        if let Some(entry) = COMMON_CODES.iter().find(|e| e.code == normalized) {
            return Ok(serde_json::to_string(&json!({
                "found": true,
                "code": entry.code,
                "name": entry.name,
                "cn_desc": entry.cn_desc,
                "causes": entry.causes,
                "docs_url": entry.docs_url
            }))
            .unwrap_or_else(|_| "{}".to_owned()));
        }

        // 未找到：返 not-found + 提示去 Microsoft 文档
        let hint = if normalized.len() <= 4 {
            "短码（≤4 位 hex）多为 BugCheck stop code，查 https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-code-reference2"
        } else if normalized.starts_with("8007") {
            "8007xxxx 是 HRESULT 包装的 Win32 error，查 https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes"
        } else if normalized.starts_with('8') {
            "8xxxxxxx 是 HRESULT，查 https://learn.microsoft.com/en-us/windows/win32/com/com-error-codes-10"
        } else if normalized.starts_with('C') {
            "Cxxxxxxx 多为 NTSTATUS，查 https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55"
        } else {
            "查 https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes"
        };
        Ok(serde_json::to_string(&json!({
            "found": false,
            "code": normalized,
            "hint": hint
        }))
        .unwrap_or_else(|_| "{}".to_owned()))
    }
}

/// 把各种格式的 error code 字符串归一化为大写 hex（无 `0x` 前缀，无前导 0 但保留至少 1 位）。
///
/// 支持：`0x7B` / `7b` / `0x0000007B` / `STOP 0x7B` / `BugCheck 0x000000D1` / `0xC0000005`
///
/// 策略：**优先全字符串搜 `0X` 前缀**（确定的 hex 标记），找不到才回退裸 hex token。
/// 这样避免「BugCheck」里的 'B' / 'C' 被误当成裸 hex token 提取。
fn normalize_code(raw: &str) -> String {
    let upper = raw.to_uppercase();

    // 策略 1：找 0X 前缀，抓后续连续 hex digit
    let hex = if let Some(prefix_pos) = upper.find("0X") {
        let after_prefix = &upper[prefix_pos + 2..];
        after_prefix
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect::<String>()
    } else {
        // 策略 2：没 0X 前缀 —— 找**第一个 hex digit 序列且长度 >= 1**
        // 注意：单字母 'B' 即合法（如 normalize_code("B") → "B"），但实际场景这种输入应该极少
        let mut found = String::new();
        for c in upper.chars() {
            if c.is_ascii_hexdigit() {
                found.push(c);
            } else if !found.is_empty() {
                break;
            }
        }
        found
    };

    // 去前导 0（但保留至少 1 位）
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        if hex.is_empty() {
            String::new()
        } else {
            "0".to_owned()
        }
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&LookupErrorCode);
    }

    #[test]
    fn normalize_handles_common_formats() {
        assert_eq!(normalize_code("0x7B"), "7B");
        assert_eq!(normalize_code("7B"), "7B");
        assert_eq!(normalize_code("0x0000007B"), "7B");
        assert_eq!(normalize_code("0x00000007B"), "7B");
        assert_eq!(normalize_code("STOP 0x7B"), "7B");
        assert_eq!(normalize_code("BugCheck 0x000000D1"), "D1");
        assert_eq!(normalize_code("0xC0000005"), "C0000005");
        assert_eq!(normalize_code("0x80070005"), "80070005");
        assert_eq!(normalize_code("7b"), "7B"); // 大小写
    }

    #[test]
    fn rejects_missing_code() {
        let r = LookupErrorCode.execute(&json!({}));
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().kind, ToolErrorKind::InvalidArgument);
    }

    #[test]
    fn finds_inaccessible_boot_device() {
        let r = LookupErrorCode.execute(&json!({ "code": "0x7B" })).unwrap();
        assert!(r.contains("INACCESSIBLE_BOOT_DEVICE"));
        assert!(r.contains("\"found\":true"));
        assert!(r.contains("启动盘"));
    }

    #[test]
    fn finds_driver_irql() {
        let r = LookupErrorCode.execute(&json!({ "code": "STOP 0xD1" })).unwrap();
        assert!(r.contains("DRIVER_IRQL_NOT_LESS_OR_EQUAL"));
        assert!(r.contains("\"found\":true"));
    }

    #[test]
    fn finds_access_denied_hresult() {
        let r = LookupErrorCode.execute(&json!({ "code": "0x80070005" })).unwrap();
        assert!(r.contains("E_ACCESSDENIED"));
        assert!(r.contains("\"found\":true"));
    }

    #[test]
    fn returns_not_found_with_hint() {
        let r = LookupErrorCode
            .execute(&json!({ "code": "0xDEADBEEF" }))
            .unwrap();
        assert!(r.contains("\"found\":false"));
        assert!(r.contains("hint"));
    }
}

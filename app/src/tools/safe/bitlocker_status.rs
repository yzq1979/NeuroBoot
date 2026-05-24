//! [Safe] bitlocker_status —— BitLocker 加密状态 + Secure Boot 状态。
//!
//! v3.0 W7。配套 `/recover-bitlocker` skill。
//! 组合输出：
//! - `manage-bde -status` —— 所有卷的加密状态、Protector 类型、百分比
//! - `Confirm-SecureBootUEFI` —— Secure Boot 是否启用（PCR7 相关）
//!
//! BitLocker 恢复键循环诊断核心：判断卷加密了没 + Secure Boot/PCR7 配置是否变化。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps;
use crate::tools::registry::{SafetyClass, Tool, ToolOutput};

pub struct BitlockerStatus;

impl Tool for BitlockerStatus {
    fn name(&self) -> &str {
        "bitlocker_status"
    }

    fn description(&self) -> &str {
        "查所有卷的 BitLocker 加密状态 + Secure Boot 状态 —— 配套 /recover-bitlocker skill。\n\
         \n\
         **When to use**: 用户开机被要求输入 BitLocker 恢复键 / 蓝紫色屏幕显示 48 位密钥框 / \
         「BitLocker recovery」/「48 digit key」时；\
         判断哪个卷被加密 + Protector 类型（TPM / TPM+PIN / Password / RecoveryKey）；\
         判断 Secure Boot 状态（KB 更新触发 PCR7 / Secure Boot 变化是 BitLocker 循环最常见原因）。\n\
         \n\
         **Parameters**: 无（管理员权限会看到更全的 Protector 详情；非管理员只能看部分卷）。\n\
         \n\
         **Returns**: JSON object 含：\n\
         - `ManageBdeRaw` (str): `manage-bde -status` 原始文本，含每个卷的：\
         `Conversion Status`（Fully Encrypted / Decrypted / Encryption In Progress）、\
         `Encryption Method`（XTS-AES 128/256）、`Protection Status`（On/Off）、`Key Protectors`\n\
         - `SecureBootEnabled` (bool / 'N/A'): Confirm-SecureBootUEFI 结果。\
         `N/A` 通常是 Legacy BIOS 模式（无 Secure Boot 概念）\n\
         \n\
         **Example output**: `{\"ManageBdeRaw\":\"BitLocker Drive Encryption: Configuration Tool version 10.0.26100\\n... Volume C: [OS]\\n    Size:                 238 GB\\n    BitLocker Version:    2.0\\n    Conversion Status:    Fully Encrypted\\n    Percentage Encrypted: 100%\\n    Encryption Method:    XTS-AES 128\\n    Protection Status:    Protection On\\n    Lock Status:          Unlocked\\n    Identification Field: None\\n    Key Protectors:\\n        TPM\\n        Numerical Password\",\"SecureBootEnabled\":true}`\n\
         \n\
         **Notes**: `Protection Status: On` + `Lock Status: Unlocked` = 正常加密状态；\
         看 `Key Protectors` 含 `Numerical Password` = 用户有恢复键（应该能去 account.microsoft.com 找）；\
         `Identification Field` 是 Active Directory 备份用的，企业 / 域账户场景看此字段定位密钥；\
         **如果只看到 TPM 没看到 Numerical Password = 麻烦**（无独立恢复键，全靠 TPM 解封）。"
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
        // manage-bde 在非管理员下也能跑（只看部分卷）；Secure Boot 命令 Legacy BIOS 会 throw
        let script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$bde = try { (manage-bde -status 2>&1 | Out-String).Trim() } catch { "(manage-bde 调用失败：$_)" }
$sb = try { (Confirm-SecureBootUEFI -ErrorAction Stop).ToString() } catch { "N/A" }
ConvertTo-Json @{
    ManageBdeRaw = $bde
    SecureBootEnabled = $sb
} -Depth 3 -Compress"#;
        run_ps(script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&BitlockerStatus);
    }
}

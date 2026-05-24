//! [Safe] find_large_files —— 找指定路径下的大文件（按大小降序）。
//!
//! v3.0 W7。配套 `/diagnose-slow` skill 的「磁盘空间」嫌疑层。
//! `Get-ChildItem -Recurse -File` + 大小过滤 + 排序，纯只读，零副作用。

use serde_json::{json, Value};

use crate::tools::ps_helper::run_ps_json_array;
use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct FindLargeFiles;

impl Tool for FindLargeFiles {
    fn name(&self) -> &str {
        "find_large_files"
    }

    fn description(&self) -> &str {
        "扫指定路径找大文件（按大小降序，仅文件不含目录）—— 定位磁盘空间被谁占了。\n\
         \n\
         **When to use**: 用户说「C 盘满了 / 电脑慢可能因为盘满」；\
         /diagnose-slow skill 的「磁盘空间」嫌疑层；\
         用户想知道「能删什么腾空间」。**调本工具前先确认用户授权扫描该路径**\
         （AppData / Documents 含敏感数据）。\n\
         \n\
         **Parameters**:\n\
         - `path` (string, required): 要扫的根目录绝对路径（如 'C:\\\\Users\\\\<user>'，\
         '\\\\\\\\?\\\\C:\\\\Windows\\\\Installer'）。**避免**扫整个 'C:\\\\' \
         （太慢 + 系统目录无意义）\n\
         - `min_size_mb` (int, optional, 默认 100): 最小大小阈值 MB，小于此值不返回\n\
         - `count` (int, optional, 默认 20): 最多返回前 N 个\n\
         \n\
         **Returns**: JSON 数组（按大小降序），每条含：\n\
         - `FullName` (str): 文件完整路径\n\
         - `SizeMB` (float): 大小 MB，1 位小数\n\
         - `Modified` (str): 最后修改 yyyy-MM-dd\n\
         \n\
         **Example output**: `[{\"FullName\":\"C:\\\\Users\\\\u\\\\Downloads\\\\setup.iso\",\"SizeMB\":4096.5,\"Modified\":\"2025-12-01\"},\
         {\"FullName\":\"C:\\\\Users\\\\u\\\\Videos\\\\rec.mp4\",\"SizeMB\":1820.3,\"Modified\":\"2026-04-15\"}]`\n\
         \n\
         **Notes**: 大目录（>10000 文件）+ 网络盘扫描会**慢** —— 优先扫 \
         Downloads / Documents / AppData\\Local\\Temp 这类典型嫌疑区；\
         返回空数组 `[]` 表示该路径下没有 > min_size_mb 的文件；\
         **不要**让用户直接删 Windows / Program Files / System Volume Information 里的文件 \
         —— 即使大也通常是系统必需。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要扫的根目录绝对路径"
                },
                "min_size_mb": {
                    "type": "integer",
                    "description": "最小大小阈值 MB",
                    "default": 100,
                    "minimum": 1,
                    "maximum": 100000
                },
                "count": {
                    "type": "integer",
                    "description": "返回前 N 个",
                    "default": 20,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 path 参数")
            })?;
        let min_size_mb = args
            .get("min_size_mb")
            .and_then(Value::as_i64)
            .unwrap_or(100)
            .clamp(1, 100_000);
        let count = args
            .get("count")
            .and_then(Value::as_i64)
            .unwrap_or(20)
            .clamp(1, 100);

        // PS 单引号包路径防转义；反斜杠 OK；嵌入单引号需 ''
        let path_escaped = path.replace('\'', "''");

        let script = format!(
            r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$threshold = {min_size_mb} * 1MB
ConvertTo-Json @(Get-ChildItem -Path '{path_escaped}' -Recurse -File -ErrorAction SilentlyContinue | Where-Object {{ $_.Length -gt $threshold }} | Sort-Object Length -Descending | Select-Object -First {count} @{{N='FullName';E={{$_.FullName}}}}, @{{N='SizeMB';E={{[math]::Round($_.Length/1MB,1)}}}}, @{{N='Modified';E={{$_.LastWriteTime.ToString('yyyy-MM-dd')}}}}) -Depth 3 -Compress"#
        );
        run_ps_json_array(&script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&FindLargeFiles);
    }

    #[test]
    fn rejects_missing_path() {
        let r = FindLargeFiles.execute(&json!({}));
        assert!(r.is_err());
        assert_eq!(r.unwrap_err().kind, ToolErrorKind::InvalidArgument);
    }
}

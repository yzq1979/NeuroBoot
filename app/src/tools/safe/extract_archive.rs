//! [Safe] extract_archive —— 7-Zip 解压（含 .7z / .zip / .rar / .tar.gz 等数十种格式）。
//!
//! v3 Quick Win 2。PE 默认无解压工具；用户从 U 盘里塞驱动包 .7z / 蓝屏 dump .zip 时
//! 必须有这个。7za.exe ~1.5 MB portable，LGPL/BSD3 双协议可商用，无依赖。

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct ExtractArchive;

fn find_7za() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"X:\NeuroBoot\tools\7zip\7za.exe"),
        PathBuf::from(r"C:\NeuroBoot\tools\7zip\7za.exe"),
        PathBuf::from(r"C:\Program Files\7-Zip\7z.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

impl Tool for ExtractArchive {
    fn name(&self) -> &str {
        "extract_archive"
    }

    fn description(&self) -> &str {
        "解压压缩包（.7z / .zip / .rar / .tar / .gz / .bz2 / .xz / .iso / 等数十种）。\n\
         \n\
         **When to use**: 用户从 U 盘 / 网络上拿到压缩包要看内容；蓝屏 dump 是 .zip；\
         驱动包 .7z；恢复出的文件含压缩文件想看里面；调研 ISO 内文件等。\n\
         \n\
         **Parameters**:\n\
         - `archive_path` (string, required): 压缩包绝对路径\n\
         - `dest_dir` (string, optional): 解压目标目录；不传则解压到 `<archive 同目录>\\<archive 名去后缀>\\`\n\
         \n\
         **Returns**: 解压结果摘要（文件数 / 总大小 / 错误如有）。\n\
         \n\
         **Example output**: ```\n\
         解压完成 → X:\\NeuroBoot\\tools\\drivers-extracted\n\
         Everything is Ok\n\
         Folders: 12\n\
         Files: 248\n\
         Size:       154892832\n\
         Compressed: 47821304\n\
         ```\n\
         \n\
         **Notes**: 7za.exe 默认不在 NeuroBoot ISO；按 docs/BUILD.md 「v3 Quick Win 工具下载」节下载\
         （~1.5 MB LGPL/BSD3 可商用）放到 X:\\NeuroBoot\\tools\\7zip\\7za.exe；\
         解压使用 `-y` (假设 yes)，不会卡交互；密码加密包暂不支持（v3.x 加 password 参数）。"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "archive_path": {
                    "type": "string",
                    "description": "压缩包绝对路径"
                },
                "dest_dir": {
                    "type": "string",
                    "description": "解压目标目录；不传则用 archive 同目录"
                }
            },
            "required": ["archive_path"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let archive = args
            .get("archive_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::with_kind(ToolErrorKind::InvalidArgument, "缺少 archive_path 参数")
            })?;

        let archive_path = PathBuf::from(archive);
        if !archive_path.exists() {
            return Err(ToolError::with_kind(
                ToolErrorKind::NotFound,
                format!("压缩包不存在：{archive}"),
            ));
        }

        let dest_dir = args
            .get("dest_dir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let parent = archive_path.parent().unwrap_or(PathBuf::from(".").as_path()).to_path_buf();
                let stem = archive_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "extracted".to_owned());
                parent.join(stem)
            });

        let exe = find_7za().ok_or_else(|| {
            ToolError::with_kind(
                ToolErrorKind::NotFound,
                "7za.exe / 7z.exe 未找到。NeuroBoot 默认 ISO 不带 7-Zip；\
                 按 docs/BUILD.md 「v3 Quick Win 工具下载」节下载（~1.5 MB LGPL/BSD3 可商用）放到 \
                 X:\\NeuroBoot\\tools\\7zip\\ 后再试。",
            )
        })?;

        std::fs::create_dir_all(&dest_dir).map_err(|e| {
            ToolError::new(format!("无法创建目标目录 {}：{e}", dest_dir.display()))
        })?;

        let output = Command::new(&exe)
            .args([
                "x",                                       // extract with full paths
                "-y",                                      // assume yes for all queries
                &format!("-o{}", dest_dir.display()),      // output dir
                archive,
            ])
            .output()
            .map_err(|e| ToolError::new(format!("启动 7za 失败：{e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(ToolError::with_kind(
                ToolErrorKind::ExternalCommandFailed,
                format!(
                    "7za 解压失败 (exit {}): stderr={}\nstdout 末尾: {}",
                    output.status.code().unwrap_or(-1),
                    stderr.trim(),
                    stdout.lines().rev().take(5).collect::<Vec<_>>().join("\n")
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        // 7za 输出末尾通常含 "Everything is Ok" + 文件统计
        Ok(format!(
            "解压完成 → {}\n{}",
            dest_dir.display(),
            stdout.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::assert_v30_description_convention;

    #[test]
    fn meets_v30_convention() {
        assert_v30_description_convention(&ExtractArchive);
    }
}

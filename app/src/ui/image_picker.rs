//! 图片选择 + base64 编码 —— vision 多模态附件的入口。
//!
//! 阶段 v1.0.1+ 新增。流程：
//! 1. `pick_image_files()` 调 rfd Win32 IFileDialog 多选 png/jpg/webp
//! 2. `load_path_as_attached()` 读盘 + base64 + 推断 mime + 大小校验
//! 3. 返回 `AttachedImage` 塞 ChatMessage::images
//!
//! 大小策略（v1）：
//! - ≤ 10 MB：直接接受
//! - 10~20 MB：警告（OpenAI vision 推荐 ≤ 20 MB；超过部分 provider 报 413）
//! - > 20 MB：硬拒（多数 API 直接报错，省得提交后失败）

use std::path::{Path, PathBuf};

use base64::Engine;

use crate::ui::chat::AttachedImage;

/// 图片选择/加载阶段产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError {
    pub path: PathBuf,
    pub message: String,
}

/// 允许的图片 mime —— rfd filter 与 mime 推断都用这个表。
pub const SUPPORTED_EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
];

/// 弹文件选择对话框（多选）。
/// 返回用户选的文件路径列表；用户取消则返回空 Vec。
///
/// 阻塞调用 —— Windows IFileDialog 是模态的，会卡住调用线程直到用户关闭对话框。
/// egui 是 immediate mode 不能让 UI 线程长卡，所以建议从按钮 click 立刻调
/// （用户预期会等几秒），不要在 update() 主循环里调。
pub fn pick_image_files() -> Vec<PathBuf> {
    let extensions: Vec<&str> = SUPPORTED_EXTENSIONS.iter().map(|(e, _)| *e).collect();
    rfd::FileDialog::new()
        .add_filter("图片 (png/jpg/webp/gif/bmp)", &extensions)
        .add_filter("所有文件", &["*"])
        .set_title("选择要上传的图片")
        .pick_files()
        .unwrap_or_default()
}

/// 读一个本地文件、推断 mime、base64 编码、校验大小，构造 AttachedImage。
pub fn load_path_as_attached(path: &Path) -> Result<AttachedImage, ImageError> {
    let bytes = std::fs::read(path).map_err(|e| ImageError {
        path: path.to_path_buf(),
        message: format!("读文件失败：{e}"),
    })?;

    let size = bytes.len() as u64;
    if size > 20 * 1024 * 1024 {
        return Err(ImageError {
            path: path.to_path_buf(),
            message: format!("文件 {} MB 超过 20 MB 上限 —— 多数 vision API 会拒收", size / 1024 / 1024),
        });
    }

    let mime = guess_mime_from_extension(path).ok_or_else(|| ImageError {
        path: path.to_path_buf(),
        message: "无法识别图片类型（仅支持 png/jpg/webp/gif/bmp）".to_owned(),
    })?;

    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let display_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    Ok(AttachedImage {
        mime: mime.to_owned(),
        data_base64,
        display_name,
        size_bytes: size,
    })
}

/// 按后缀名推断 mime。返回 None 表示后缀不在支持表里。
fn guess_mime_from_extension(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())?;
    SUPPORTED_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, mime)| *mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_mime_common_types() {
        assert_eq!(
            guess_mime_from_extension(Path::new("a.PNG")),
            Some("image/png")
        );
        assert_eq!(
            guess_mime_from_extension(Path::new("dir\\foo.jpg")),
            Some("image/jpeg")
        );
        assert_eq!(
            guess_mime_from_extension(Path::new("foo.jpeg")),
            Some("image/jpeg")
        );
        assert_eq!(
            guess_mime_from_extension(Path::new("foo.webp")),
            Some("image/webp")
        );
        assert_eq!(guess_mime_from_extension(Path::new("foo.txt")), None);
        assert_eq!(guess_mime_from_extension(Path::new("noext")), None);
    }

    #[test]
    fn load_oversized_file_rejected() {
        // 写一个 > 20 MB 的临时文件，验证 load 拒绝
        let tmp = std::env::temp_dir().join("neuroboot_image_picker_oversize_test.png");
        let big = vec![0u8; 21 * 1024 * 1024];
        std::fs::write(&tmp, &big).unwrap();

        let err = load_path_as_attached(&tmp).unwrap_err();
        assert!(err.message.contains("超过 20 MB"), "msg: {}", err.message);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn load_small_png_succeeds() {
        // 1×1 透明 PNG 的最小合法字节
        let png_bytes = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let tmp = std::env::temp_dir().join("neuroboot_image_picker_small_test.png");
        std::fs::write(&tmp, &png_bytes).unwrap();

        let img = load_path_as_attached(&tmp).unwrap();
        assert_eq!(img.mime, "image/png");
        assert_eq!(img.size_bytes, png_bytes.len() as u64);
        assert!(!img.data_base64.is_empty());
        assert!(img.display_name.ends_with(".png"));

        let _ = std::fs::remove_file(&tmp);
    }
}

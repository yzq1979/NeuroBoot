//! 中文字体加载：把 Noto Sans SC 注册成 egui 默认 proportional 字体。
//!
//! 字体二进制通过 `include_bytes!` 在编译期嵌入 exe —— 运行时不需要外部文件，
//! 这对 PE 环境（裸盘、字体路径不确定）尤其重要。

use std::sync::Arc;

use eframe::egui;

/// Noto Sans SC Regular 字体二进制（约 2.4 MB，覆盖 GB 2312 一二级常用字）。
const NOTO_SANS_SC: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Regular.ttf");

/// 把 Noto Sans SC 注册成 egui 的默认中文字体。
///
/// 在 `eframe::run_native` 的 `AppCreator` 闭包里调用一次即可，
/// 之后所有 `ui.label("中文")` 等都会用这个字体渲染。
pub fn install_chinese_fonts(ctx: &egui::Context) {
    // 从 egui 默认 FontDefinitions 起步（保留 emoji 等内置族）
    let mut fonts = egui::FontDefinitions::default();

    // 把 Noto Sans SC 注册到名字 "noto_sans_sc"
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        Arc::new(egui::FontData::from_static(NOTO_SANS_SC)),
    );

    // 插到 Proportional（正文）族最前面 —— 优先用中文字体渲染所有正文
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());

    // 加到 Monospace（等宽）末尾 —— 代码块出现中文时也有回退
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("noto_sans_sc".to_owned());

    ctx.set_fonts(fonts);
}

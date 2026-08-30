//! Embedded CJK font for egui. Default fonts lack simplified-Chinese glyphs.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Register Noto Sans SC (SIL OFL 1.1) so UI labels render CJK text.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSansSC-Regular.otf"
        ))
        .into(),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto_sans_sc".to_owned());

    ctx.set_fonts(fonts);
}

//! Waver Session: flat warm-gray workstation tokens.

use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

pub const BACKGROUND: Color32 = Color32::from_rgb(174, 177, 172);
pub const CANVAS: Color32 = Color32::from_rgb(185, 188, 182);
pub const PANEL: Color32 = Color32::from_rgb(213, 215, 208);
pub const SURFACE: Color32 = Color32::from_rgb(230, 231, 224);
pub const BORDER: Color32 = Color32::from_rgb(112, 118, 110);
pub const TEXT: Color32 = Color32::from_rgb(34, 38, 33);
pub const MUTED: Color32 = Color32::from_rgb(78, 85, 76);
pub const ACCENT: Color32 = Color32::from_rgb(247, 164, 75);
pub const INPUT: Color32 = Color32::from_rgb(57, 96, 110);
pub const OUTPUT: Color32 = Color32::from_rgb(123, 72, 27);
pub const CABLE: Color32 = Color32::from_rgb(111, 77, 34);
pub const GOOD: Color32 = Color32::from_rgb(49, 95, 51);
pub const DANGER: Color32 = Color32::from_rgb(153, 45, 45);

pub const GRID: Color32 = Color32::from_rgb(166, 172, 163);
pub const ACTIVE_SURFACE: Color32 = Color32::from_rgb(239, 208, 166);
pub const DANGER_SURFACE: Color32 = Color32::from_rgb(233, 195, 187);

pub fn setup(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.slider_trailing_fill = true;
    style.visuals.selection.stroke = Stroke::new(1.0, TEXT);
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(2);
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    style.visuals.widgets.hovered.weak_bg_fill = ACTIVE_SURFACE;
    style.visuals.widgets.active.weak_bg_fill = ACTIVE_SURFACE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    style.spacing.interact_size.y = 24.0;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, style);
}

pub fn panel_frame(margin: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .inner_margin(margin)
        .stroke(Stroke::new(1.0, GRID))
}

pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(MUTED));
}

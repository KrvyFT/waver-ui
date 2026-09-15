//! Design tokens from waver-product-design.svg.

use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

pub const BACKGROUND: Color32 = Color32::from_rgb(17, 19, 24);
pub const CANVAS: Color32 = Color32::from_rgb(28, 30, 36);
pub const PANEL: Color32 = Color32::from_rgb(32, 34, 42);
pub const SURFACE: Color32 = Color32::from_rgb(38, 40, 48);
pub const BORDER: Color32 = Color32::from_rgb(80, 84, 94);
pub const TEXT: Color32 = Color32::from_rgb(206, 210, 218);
pub const MUTED: Color32 = Color32::from_rgb(150, 155, 165);
pub const ACCENT: Color32 = Color32::from_rgb(100, 180, 255);
pub const INPUT: Color32 = Color32::from_rgb(80, 160, 220);
pub const OUTPUT: Color32 = Color32::from_rgb(220, 160, 60);
pub const CABLE: Color32 = Color32::from_rgb(200, 200, 80);
pub const GOOD: Color32 = Color32::from_rgb(92, 200, 138);
pub const DANGER: Color32 = Color32::from_rgb(220, 80, 80);

pub fn setup(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.slider_trailing_fill = true;
    style.visuals.selection.stroke = Stroke::new(1.0, BACKGROUND);
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.bg_fill = SURFACE;
        widget.weak_bg_fill = SURFACE;
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 32.0;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(20.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub fn panel_frame(margin: i8) -> egui::Frame {
    egui::Frame::new().fill(PANEL).inner_margin(margin)
}

pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(MUTED));
}

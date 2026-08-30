//! Rotary knob widgets for modular synth parameter control.

use eframe::egui;

const KNOB_RADIUS: f32 = 18.0;
const DRAG_SENSITIVITY: f32 = 0.008;

/// Mapping curve for a knob.
#[derive(Clone, Copy, Debug)]
pub enum KnobScale {
    Linear,
    Logarithmic,
}

/// Draw and interact with a rotary knob.
///
/// Mutates `value` while dragging / double-click reset, and marks the response
/// changed so callers can sync into [`waver_core::ParamCell`].
pub fn rotary_knob(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    scale: KnobScale,
) -> egui::Response {
    ui.push_id(id, |ui| {
        let (rect, mut response) = ui.allocate_exact_size(
            egui::vec2(KNOB_RADIUS * 2.0 + 8.0, KNOB_RADIUS * 2.0 + 22.0),
            egui::Sense::click_and_drag(),
        );

        if response.double_clicked() {
            *value = default_for_range(&range, scale);
            response.mark_changed();
        }

        if response.dragged() {
            let delta = -response.drag_delta().y * DRAG_SENSITIVITY;
            let next = match scale {
                KnobScale::Linear => {
                    let span = *range.end() - *range.start();
                    (*value + delta * span).clamp(*range.start(), *range.end())
                }
                KnobScale::Logarithmic => {
                    let log_min = range.start().max(1e-6).ln();
                    let log_max = range.end().max(1e-6).ln();
                    let log_v = value.max(1e-6).ln().clamp(log_min, log_max);
                    (log_v + delta * (log_max - log_min))
                        .exp()
                        .clamp(*range.start(), *range.end())
                }
            };
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                response.mark_changed();
            }
        }

        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let center = egui::pos2(rect.center().x, rect.top() + KNOB_RADIUS + 2.0);
        let painter = ui.painter_at(rect);
        let stroke_color = if response.hovered() || response.dragged() {
            egui::Color32::from_rgb(220, 180, 90)
        } else {
            egui::Color32::from_gray(120)
        };

        painter.circle_filled(center, KNOB_RADIUS, egui::Color32::from_rgb(32, 34, 42));
        painter.circle_stroke(
            center,
            KNOB_RADIUS,
            egui::Stroke::new(1.5, stroke_color),
        );

        let t = normalized_value(*value, &range, scale);
        let angle = egui::remap(t, 0.0..=1.0, (-2.4)..=2.4);
        let tip = center + egui::vec2(angle.sin(), -angle.cos()) * (KNOB_RADIUS - 4.0);
        painter.line_segment(
            [center, tip],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(240, 210, 120)),
        );

        let arc_steps = 24;
        for i in 0..arc_steps {
            let f0 = i as f32 / arc_steps as f32;
            let f1 = (i + 1) as f32 / arc_steps as f32;
            if f1 > t {
                break;
            }
            let a0 = egui::remap(f0, 0.0..=1.0, (-2.4)..=2.4);
            let a1 = egui::remap(f1, 0.0..=1.0, (-2.4)..=2.4);
            let p0 = center + egui::vec2(a0.sin(), -a0.cos()) * (KNOB_RADIUS + 2.0);
            let p1 = center + egui::vec2(a1.sin(), -a1.cos()) * (KNOB_RADIUS + 2.0);
            painter.line_segment(
                [p0, p1],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 220)),
            );
        }

        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            format_value(label, *value, scale),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(190),
        );

        painter.text(
            egui::pos2(rect.center().x, rect.top()),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(150),
        );

        response
    })
    .inner
}

/// Four-position wave selector. Returns true when the selection changed.
pub fn wave_selector(ui: &mut egui::Ui, value: &mut f32) -> bool {
    let before = value.round() as i32;
    let mut wave = before;
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new("波形")
                .size(10.0)
                .color(egui::Color32::from_gray(150)),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (name, idx) in [("~", 0), ("/|", 1), ("⊓", 2), ("△", 3)] {
                let selected = wave == idx;
                let fill = if selected {
                    egui::Color32::from_rgb(100, 160, 220)
                } else {
                    egui::Color32::from_rgb(45, 48, 58)
                };
                if ui
                    .add(
                        egui::Button::new(name)
                            .fill(fill)
                            .min_size(egui::vec2(28.0, 22.0)),
                    )
                    .clicked()
                {
                    wave = idx;
                }
            }
        });
    });
    *value = wave as f32;
    wave != before
}

fn normalized_value(value: f32, range: &std::ops::RangeInclusive<f32>, scale: KnobScale) -> f32 {
    match scale {
        KnobScale::Linear => {
            let span = *range.end() - *range.start();
            if span <= f32::EPSILON {
                0.0
            } else {
                ((value - *range.start()) / span).clamp(0.0, 1.0)
            }
        }
        KnobScale::Logarithmic => {
            let log_min = range.start().max(1e-6).ln();
            let log_max = range.end().max(1e-6).ln();
            let span = log_max - log_min;
            if span <= f32::EPSILON {
                0.0
            } else {
                ((value.max(1e-6).ln() - log_min) / span).clamp(0.0, 1.0)
            }
        }
    }
}

fn default_for_range(range: &std::ops::RangeInclusive<f32>, scale: KnobScale) -> f32 {
    match scale {
        KnobScale::Linear => (*range.start() + *range.end()) * 0.5,
        KnobScale::Logarithmic => {
            let log_min = range.start().max(1e-6).ln();
            let log_max = range.end().max(1e-6).ln();
            ((log_min + log_max) * 0.5).exp()
        }
    }
}

fn format_value(label: &str, value: f32, scale: KnobScale) -> String {
    if label.contains("Hz") || label == "FREQ" || label == "频率" {
        if value >= 1000.0 {
            format!("{:.1}k", value / 1000.0)
        } else {
            format!("{:.0}", value)
        }
    } else if label == "AMP" || label == "振幅" {
        format!("{:.0}%", value * 100.0)
    } else {
        match scale {
            KnobScale::Linear => format!("{:.2}", value),
            KnobScale::Logarithmic => format!("{:.1}", value),
        }
    }
}

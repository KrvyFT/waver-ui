//! Painter-only node visuals + manual hit regions.

use eframe::egui;
use waver_core::{Node, NodeKind, ParamId, ParamRegistry, PortId, PortRef};

use super::cable::{JackPos, JACK_HIT_RADIUS};

pub const VCO_SIZE: egui::Vec2 = egui::vec2(220.0, 128.0);
pub const NODE_HEADER_H: f32 = 26.0;

#[derive(Clone, Copy, Debug)]
pub enum NodeHit {
    Header,
    Body,
    Knob { param: u32 },
    Wave { index: u32 },
}

/// Draw a node and return jack positions (screen coords).
pub fn draw_node_visual(
    painter: &egui::Painter,
    node: &Node,
    rect: egui::Rect,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> Vec<JackPos> {
    match node.kind {
        NodeKind::Vco => draw_vco(painter, node, rect, selected, params),
        _ => draw_simple(painter, node, rect, selected),
    }
}

/// Hit radius for on-module rotary knobs (larger than visual radius for easier grabs).
pub const KNOB_HIT_RADIUS: f32 = 28.0;

/// Hit-test pointer against a node's interactive regions.
///
/// Knob / wave targets are tested before the header so a slightly-high amp grab
/// is not stolen by the title bar, and Body never starts a module drag.
pub fn hit_test_node(kind: NodeKind, rect: egui::Rect, pointer: egui::Pos2) -> Option<NodeHit> {
    if !rect.contains(pointer) {
        return None;
    }
    if kind == NodeKind::Vco {
        let freq_c = knob_center(rect, 0);
        let amp_c = knob_center(rect, 1);
        // Prefer the nearer knob when both circles overlap the pointer.
        let d_freq = pointer.distance(freq_c);
        let d_amp = pointer.distance(amp_c);
        if d_freq <= KNOB_HIT_RADIUS || d_amp <= KNOB_HIT_RADIUS {
            if d_freq <= d_amp {
                return Some(NodeHit::Knob { param: 0 });
            }
            return Some(NodeHit::Knob { param: 1 });
        }
        for i in 0..4 {
            if wave_button_rect(rect, i).contains(pointer) {
                return Some(NodeHit::Wave { index: i });
            }
        }
    }
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), NODE_HEADER_H));
    if header.contains(pointer) {
        return Some(NodeHit::Header);
    }
    Some(NodeHit::Body)
}

fn draw_vco(
    painter: &egui::Painter,
    node: &Node,
    rect: egui::Rect,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> Vec<JackPos> {
    let stroke = if selected {
        egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(80))
    };
    painter.rect(
        rect,
        4.0,
        egui::Color32::from_rgb(38, 40, 48),
        stroke,
        egui::StrokeKind::Inside,
    );

    // Header
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), NODE_HEADER_H)),
        egui::CornerRadius {
            nw: 4,
            ne: 4,
            sw: 0,
            se: 0,
        },
        egui::Color32::from_rgb(48, 52, 62),
    );
    painter.text(
        rect.left_top() + egui::vec2(8.0, NODE_HEADER_H * 0.5),
        egui::Align2::LEFT_CENTER,
        "VCO",
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    let freq = params
        .and_then(|p| p.get(node.id, ParamId::new(0)))
        .map(|c| c.value())
        .unwrap_or(440.0);
    let amp = params
        .and_then(|p| p.get(node.id, ParamId::new(1)))
        .map(|c| c.value())
        .unwrap_or(0.5);
    let wave = params
        .and_then(|p| p.get(node.id, ParamId::new(2)))
        .map(|c| c.value().round() as u32)
        .unwrap_or(0);

    draw_knob_visual(painter, knob_center(rect, 0), "FREQ", &format_freq(freq), freq_norm(freq));
    draw_knob_visual(
        painter,
        knob_center(rect, 1),
        "AMP",
        &format!("{:.0}%", amp * 100.0),
        amp.clamp(0.0, 1.0),
    );

    for i in 0..4 {
        let r = wave_button_rect(rect, i);
        let on = wave == i;
        painter.rect(
            r,
            3.0,
            if on {
                egui::Color32::from_rgb(100, 160, 220)
            } else {
                egui::Color32::from_rgb(45, 48, 58)
            },
            egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
            egui::StrokeKind::Inside,
        );
        let label = match i {
            0 => "~",
            1 => "/|",
            2 => "⊓",
            _ => "△",
        };
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    // OUT jack
    let jack_c = egui::pos2(rect.right() - 16.0, rect.bottom() - 16.0);
    draw_jack(painter, jack_c, true);
    painter.text(
        jack_c + egui::vec2(-14.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        "OUT",
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(180),
    );

    vec![JackPos {
        port: PortRef {
            node: node.id,
            port: PortId::new(0),
        },
        center: jack_c,
        is_output: true,
    }]
}

fn draw_simple(
    painter: &egui::Painter,
    node: &Node,
    rect: egui::Rect,
    selected: bool,
) -> Vec<JackPos> {
    let stroke = if selected {
        egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(80))
    };
    painter.rect(
        rect,
        4.0,
        egui::Color32::from_rgb(38, 40, 48),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), NODE_HEADER_H)),
        egui::CornerRadius {
            nw: 4,
            ne: 4,
            sw: 0,
            se: 0,
        },
        egui::Color32::from_rgb(48, 52, 62),
    );
    painter.text(
        rect.left_top() + egui::vec2(8.0, NODE_HEADER_H * 0.5),
        egui::Align2::LEFT_CENTER,
        kind_label(node.kind),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    let mut jacks = Vec::new();
    let counts = node.kind.port_counts();
    if node.kind == NodeKind::Output {
        painter.text(
            rect.center() + egui::vec2(0.0, 6.0),
            egui::Align2::CENTER_CENTER,
            "→ 设备",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(140),
        );
    }
    for i in 0..counts.inputs {
        let c = egui::pos2(rect.left() + 16.0, rect.top() + NODE_HEADER_H + 20.0 + i as f32 * 22.0);
        draw_jack(painter, c, false);
        painter.text(
            c + egui::vec2(14.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "IN",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(180),
        );
        jacks.push(JackPos {
            port: PortRef {
                node: node.id,
                port: PortId::new(i),
            },
            center: c,
            is_output: false,
        });
    }
    for i in 0..counts.outputs {
        let c = egui::pos2(rect.right() - 16.0, rect.top() + NODE_HEADER_H + 20.0 + i as f32 * 22.0);
        draw_jack(painter, c, true);
        painter.text(
            c + egui::vec2(-14.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            "OUT",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(180),
        );
        jacks.push(JackPos {
            port: PortRef {
                node: node.id,
                port: PortId::new(i),
            },
            center: c,
            is_output: true,
        });
    }
    jacks
}

fn knob_center(rect: egui::Rect, index: u32) -> egui::Pos2 {
    let x = rect.left() + 36.0 + index as f32 * 56.0;
    let y = rect.top() + NODE_HEADER_H + 38.0;
    egui::pos2(x, y)
}

fn wave_button_rect(rect: egui::Rect, index: u32) -> egui::Rect {
    let x = rect.left() + 130.0 + index as f32 * 22.0;
    let y = rect.top() + NODE_HEADER_H + 28.0;
    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(20.0, 20.0))
}

fn draw_knob_visual(
    painter: &egui::Painter,
    center: egui::Pos2,
    label: &str,
    value_text: &str,
    t: f32,
) {
    let r = 16.0;
    painter.circle_filled(center, r, egui::Color32::from_rgb(32, 34, 42));
    painter.circle_stroke(center, r, egui::Stroke::new(1.5, egui::Color32::from_gray(120)));
    let angle = egui::remap(t.clamp(0.0, 1.0), 0.0..=1.0, (-2.4)..=2.4);
    let tip = center + egui::vec2(angle.sin(), -angle.cos()) * (r - 4.0);
    painter.line_segment(
        [center, tip],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(240, 210, 120)),
    );
    painter.text(
        center + egui::vec2(0.0, -r - 2.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(150),
    );
    painter.text(
        center + egui::vec2(0.0, r + 2.0),
        egui::Align2::CENTER_TOP,
        value_text,
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(190),
    );
}

fn draw_jack(painter: &egui::Painter, center: egui::Pos2, is_output: bool) {
    let fill = if is_output {
        egui::Color32::from_rgb(220, 160, 60)
    } else {
        egui::Color32::from_rgb(80, 160, 220)
    };
    painter.circle_filled(center, JACK_HIT_RADIUS * 0.5, fill);
    painter.circle_stroke(
        center,
        JACK_HIT_RADIUS * 0.5,
        egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
    );
}

fn freq_norm(freq: f32) -> f32 {
    let lo = 20.0f32.ln();
    let hi = 2000.0f32.ln();
    ((freq.max(20.0).ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
}

fn format_freq(freq: f32) -> String {
    if freq >= 1000.0 {
        format!("{:.1}k", freq / 1000.0)
    } else {
        format!("{:.0}", freq)
    }
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Vco => "VCO",
        NodeKind::Output => "Output",
        NodeKind::Vcf => "VCF",
        NodeKind::Vca => "VCA",
        NodeKind::Adsr => "ADSR",
        NodeKind::Lfo => "LFO",
        NodeKind::Mixer => "Mixer",
        NodeKind::Silence => "Silence",
        NodeKind::Delay => "Delay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amp_knob_hit_not_body_or_header() {
        let rect = egui::Rect::from_min_size(egui::pos2(161.3, 224.7), VCO_SIZE);
        let amp = knob_center(rect, 1);
        match hit_test_node(NodeKind::Vco, rect, amp) {
            Some(NodeHit::Knob { param: 1 }) => {}
            other => panic!("expected Amp knob, got {other:?}"),
        }
        // Slightly above amp (toward header) should still prefer knob over header.
        let near_top = egui::pos2(amp.x, amp.y - 20.0);
        match hit_test_node(NodeKind::Vco, rect, near_top) {
            Some(NodeHit::Knob { param: 1 }) => {}
            other => panic!("expected Amp knob near top edge, got {other:?}"),
        }
    }

    #[test]
    fn header_hit_only_on_title_bar() {
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 160.0), VCO_SIZE);
        let header_pt = egui::pos2(rect.center().x, rect.top() + 10.0);
        assert!(matches!(
            hit_test_node(NodeKind::Vco, rect, header_pt),
            Some(NodeHit::Header)
        ));
        let body_pt = egui::pos2(rect.right() - 30.0, rect.top() + 50.0);
        assert!(matches!(
            hit_test_node(NodeKind::Vco, rect, body_pt),
            Some(NodeHit::Body)
        ));
    }
}

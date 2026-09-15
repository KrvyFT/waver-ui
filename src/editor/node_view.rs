//! Painter-only module cards and their matching hit regions.

use super::cable::JackPos;
use crate::theme;
use eframe::egui;
use waver_core::{Node, NodeKind, ParamId, ParamRegistry, PortId, PortRef};

pub const VCO_SIZE: egui::Vec2 = egui::vec2(208.0, 264.0);
pub const NODE_HEADER_H: f32 = 46.0;
pub const KNOB_HIT_RADIUS: f32 = 34.0;

pub fn node_size(kind: NodeKind) -> egui::Vec2 {
    if kind == NodeKind::Vco {
        VCO_SIZE
    } else {
        egui::vec2(180.0, 164.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NodeHit {
    Header,
    Body,
    Knob { param: u32 },
    Wave { index: u32 },
}

pub fn hit_test_node(kind: NodeKind, rect: egui::Rect, pointer: egui::Pos2) -> Option<NodeHit> {
    if !rect.contains(pointer) {
        return None;
    }
    if kind == NodeKind::Vco {
        for param in 0..2 {
            if pointer.distance(knob_center(rect, param)) <= KNOB_HIT_RADIUS {
                return Some(NodeHit::Knob { param });
            }
        }
        for index in 0..4 {
            if wave_button_rect(rect, index).contains(pointer) {
                return Some(NodeHit::Wave { index });
            }
        }
    }
    if pointer.y < rect.top() + NODE_HEADER_H {
        Some(NodeHit::Header)
    } else {
        Some(NodeHit::Body)
    }
}

pub fn draw_node_visual(
    painter: &egui::Painter,
    node: &Node,
    rect: egui::Rect,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> Vec<JackPos> {
    painter.rect_filled(
        rect.translate(egui::vec2(0.0, 8.0)).expand(4.0),
        16.0,
        egui::Color32::from_black_alpha(35),
    );
    painter.rect_filled(rect, 12.0, theme::SURFACE);
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), NODE_HEADER_H)),
        egui::CornerRadius {
            nw: 12,
            ne: 12,
            sw: 0,
            se: 0,
        },
        theme::PANEL,
    );
    painter.rect_stroke(
        rect,
        12.0,
        egui::Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::ACCENT
            } else {
                theme::BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    text(
        painter,
        rect.min + egui::vec2(16.0, 24.0),
        kind_label(node.kind),
        14.0,
        theme::TEXT,
    );

    if node.kind == NodeKind::Vco {
        let value = |id, fallback| {
            params
                .and_then(|p| p.get(node.id, ParamId::new(id)))
                .map(|cell| cell.value())
                .unwrap_or(fallback)
        };
        let freq = value(0, 440.0);
        let amp = value(1, 0.5);
        draw_knob(
            painter,
            knob_center(rect, 0),
            "频率",
            &format!("{freq:.0} Hz"),
            ((freq.max(20.0).ln() - 20.0f32.ln()) / (2000.0f32.ln() - 20.0f32.ln()))
                .clamp(0.0, 1.0),
        );
        draw_knob(
            painter,
            knob_center(rect, 1),
            "振幅",
            &format!("{:.0}%", amp * 100.0),
            amp,
        );
        text(
            painter,
            rect.min + egui::vec2(16.0, 204.0),
            "波形",
            11.0,
            theme::MUTED,
        );
        let wave = value(2, 0.0).round() as u32;
        for index in 0..4 {
            let button = wave_button_rect(rect, index);
            painter.rect_filled(
                button,
                6.0,
                if index == wave {
                    theme::ACCENT
                } else {
                    theme::PANEL
                },
            );
            painter.text(
                button.center(),
                egui::Align2::CENTER_CENTER,
                ["正弦", "锯齿", "方波", "三角"][index as usize],
                egui::FontId::proportional(11.0),
                if index == wave {
                    theme::BACKGROUND
                } else {
                    theme::MUTED
                },
            );
        }
    } else {
            text(
                painter,
                rect.min + egui::vec2(16.0, 132.0),
                node.kind.desc().summary,
                12.0,
                theme::MUTED,
            );
    }

    let mut jacks = Vec::new();
    let counts = node.kind.port_counts();
    for (is_output, count) in [(false, counts.inputs), (true, counts.outputs)] {
        for index in 0..count {
            let center = egui::pos2(
                if is_output {
                    rect.right() - 8.0
                } else {
                    rect.left() + 8.0
                },
                rect.top()
                    + if node.kind == NodeKind::Vco {
                        184.0
                    } else {
                        82.0
                    }
                    + index as f32 * 32.0,
            );
            painter.circle_filled(
                center,
                8.0,
                if is_output {
                    theme::OUTPUT
                } else {
                    theme::INPUT
                },
            );
            painter.text(
                center + egui::vec2(if is_output { -16.0 } else { 16.0 }, 0.0),
                if is_output {
                    egui::Align2::RIGHT_CENTER
                } else {
                    egui::Align2::LEFT_CENTER
                },
                if is_output { "OUT 输出" } else { "IN 输入" },
                egui::FontId::proportional(10.0),
                theme::MUTED,
            );
            jacks.push(JackPos {
                port: PortRef {
                    node: node.id,
                    port: PortId::new(index),
                },
                center,
                is_output,
            });
        }
    }
    jacks
}

fn text(painter: &egui::Painter, pos: egui::Pos2, label: &str, size: f32, color: egui::Color32) {
    painter.text(
        pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(size),
        color,
    );
}

fn knob_center(rect: egui::Rect, index: u32) -> egui::Pos2 {
    rect.min + egui::vec2(54.0 + index as f32 * 100.0, 116.0)
}

fn wave_button_rect(rect: egui::Rect, index: u32) -> egui::Rect {
    egui::Rect::from_min_size(
        rect.min + egui::vec2(16.0 + index as f32 * 45.0, 220.0),
        egui::vec2(41.0, 32.0),
    )
}

fn draw_knob(painter: &egui::Painter, center: egui::Pos2, label: &str, value: &str, t: f32) {
    painter.circle_filled(center, 32.0, theme::CANVAS);
    painter.circle_stroke(center, 32.0, egui::Stroke::new(1.0, theme::BORDER));
    let angle = egui::remap(t.clamp(0.0, 1.0), 0.0..=1.0, -2.4..=2.4);
    painter.line_segment(
        [
            center,
            center + egui::vec2(angle.sin(), -angle.cos()) * 26.0,
        ],
        egui::Stroke::new(4.0, theme::ACCENT),
    );
    for (offset, label, size, color) in [
        (-44.0, label, 11.0, theme::MUTED),
        (44.0, value, 16.0, egui::Color32::WHITE),
    ] {
        painter.text(
            center + egui::vec2(0.0, offset),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(size),
            color,
        );
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    kind.desc().canvas_label
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

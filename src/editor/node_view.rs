//! Per-module node visuals (VCO, Output).

use eframe::egui;
use waver_core::{Node, NodeKind, PortId, PortRef};

use super::cable::{JackPos, JACK_HIT_RADIUS};

const NODE_WIDTH: f32 = 140.0;
const HEADER_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 22.0;

/// Layout info returned after drawing a node.
pub struct NodeLayout {
    pub rect: egui::Rect,
    pub jacks: Vec<JackPos>,
}

pub fn draw_node(
    ui: &mut egui::Ui,
    node: &Node,
    pos: egui::Pos2,
    selected: bool,
) -> NodeLayout {
    let counts = node.kind.port_counts();
    let body_rows = counts.inputs.max(counts.outputs).max(1);
    let height = HEADER_HEIGHT + body_rows as f32 * ROW_HEIGHT + 8.0;
    let rect = egui::Rect::from_min_size(pos, egui::vec2(NODE_WIDTH, height));

    let painter = ui.painter();
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

    let title = kind_label(node.kind);
    painter.text(
        rect.left_top() + egui::vec2(8.0, 4.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    let mut jacks = Vec::new();

    for i in 0..counts.inputs {
        let y = rect.top() + HEADER_HEIGHT + i as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5;
        let center = egui::pos2(rect.left() + 10.0, y);
        draw_jack(painter, center, false);
        painter.text(
            egui::pos2(rect.left() + 22.0, y),
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
            center,
            is_output: false,
        });
    }

    for i in 0..counts.outputs {
        let y = rect.top() + HEADER_HEIGHT + i as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5;
        let center = egui::pos2(rect.right() - 10.0, y);
        draw_jack(painter, center, true);
        painter.text(
            egui::pos2(rect.right() - 22.0, y),
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
            center,
            is_output: true,
        });
    }

    if node.kind == NodeKind::Output {
        painter.text(
            rect.center() + egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            "→ 设备",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(140),
        );
    }

    NodeLayout { rect, jacks }
}

fn draw_jack(painter: &egui::Painter, center: egui::Pos2, is_output: bool) {
    let fill = if is_output {
        egui::Color32::from_rgb(220, 160, 60)
    } else {
        egui::Color32::from_rgb(80, 160, 220)
    };
    painter.circle_filled(center, JACK_HIT_RADIUS * 0.55, fill);
    painter.circle_stroke(
        center,
        JACK_HIT_RADIUS * 0.55,
        egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
    );
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

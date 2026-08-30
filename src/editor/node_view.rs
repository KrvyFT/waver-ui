//! Per-module node visuals (VCO, Output).

use eframe::egui;
use waver_core::{Node, NodeKind, ParamId, ParamRegistry, PortId, PortRef};

use super::cable::{JackPos, JACK_HIT_RADIUS};
use super::knob::{wave_selector, KnobScale, rotary_knob};

const VCO_WIDTH: f32 = 200.0;
const SIMPLE_WIDTH: f32 = 140.0;
const ROW_HEIGHT: f32 = 22.0;

/// Layout info returned after drawing a node.
pub struct NodeLayout {
    pub rect: egui::Rect,
    pub jacks: Vec<JackPos>,
}

/// Interactive node widget placed at an absolute canvas position.
pub fn show_node(
    ui: &mut egui::Ui,
    node: &Node,
    pos: egui::Pos2,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> NodeLayout {
    match node.kind {
        NodeKind::Vco => show_vco_node(ui, node, pos, selected, params),
        _ => show_simple_node(ui, node, pos, selected),
    }
}

fn show_vco_node(
    ui: &mut egui::Ui,
    node: &Node,
    pos: egui::Pos2,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> NodeLayout {
    let mut jacks = Vec::new();

    let area_response = egui::Area::new(egui::Id::new(("node", node.id.raw())))
        .current_pos(pos)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(egui::Color32::from_rgb(38, 40, 48))
                .stroke(if selected {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255))
                } else {
                    egui::Stroke::new(1.0, egui::Color32::from_gray(80))
                })
                .corner_radius(4.0)
                .inner_margin(6.0);

            frame.show(ui, |ui| {
                ui.set_width(VCO_WIDTH - 12.0);
                ui.label(
                    egui::RichText::new("VCO")
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.add_space(4.0);

                if let Some(registry) = params {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let Some(freq) = registry.get(node.id, ParamId::new(0)) {
                            let mut v = freq.value();
                            if rotary_knob(
                                ui,
                                egui::Id::new(("knob", node.id.raw(), 0u32)),
                                "FREQ",
                                &mut v,
                                20.0..=2000.0,
                                KnobScale::Logarithmic,
                            )
                            .changed()
                            {
                                freq.set(v);
                            }
                        }
                        if let Some(amp) = registry.get(node.id, ParamId::new(1)) {
                            let mut v = amp.value();
                            if rotary_knob(
                                ui,
                                egui::Id::new(("knob", node.id.raw(), 1u32)),
                                "AMP",
                                &mut v,
                                0.0..=1.0,
                                KnobScale::Linear,
                            )
                            .changed()
                            {
                                amp.set(v);
                            }
                        }
                        if let Some(wave) = registry.get(node.id, ParamId::new(2)) {
                            let mut v = wave.value();
                            ui.vertical(|ui| {
                                wave_selector(ui, &mut v);
                            });
                            if (v - wave.value()).abs() > f32::EPSILON {
                                wave.set(v);
                            }
                        }
                    });
                } else {
                    ui.label("无参数");
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (jack_rect, _) = ui.allocate_exact_size(
                            egui::vec2(24.0, 20.0),
                            egui::Sense::hover(),
                        );
                        let center = jack_rect.center();
                        draw_jack(ui.painter(), center, true);
                        ui.label(
                            egui::RichText::new("OUT")
                                .size(11.0)
                                .color(egui::Color32::from_gray(180)),
                        );
                        jacks.push(JackPos {
                            port: PortRef {
                                node: node.id,
                                port: PortId::new(0),
                            },
                            center,
                            is_output: true,
                        });
                    });
                });
            });
        });

    NodeLayout {
        rect: area_response.response.rect,
        jacks,
    }
}

fn show_simple_node(
    ui: &mut egui::Ui,
    node: &Node,
    pos: egui::Pos2,
    selected: bool,
) -> NodeLayout {
    let counts = node.kind.port_counts();
    let body_rows = counts.inputs.max(counts.outputs).max(1);
    let height = 28.0 + body_rows as f32 * ROW_HEIGHT + 8.0;
    let mut jacks = Vec::new();

    let area_response = egui::Area::new(egui::Id::new(("node", node.id.raw())))
        .current_pos(pos)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(egui::Color32::from_rgb(38, 40, 48))
                .stroke(if selected {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255))
                } else {
                    egui::Stroke::new(1.0, egui::Color32::from_gray(80))
                })
                .corner_radius(4.0)
                .inner_margin(6.0);

            frame.show(ui, |ui| {
                ui.set_min_width(SIMPLE_WIDTH - 12.0);
                ui.label(
                    egui::RichText::new(kind_label(node.kind))
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                if node.kind == NodeKind::Output {
                    ui.label(
                        egui::RichText::new("→ 设备")
                            .size(11.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                }

                for i in 0..counts.inputs {
                    ui.horizontal(|ui| {
                        let (jack_rect, _) = ui.allocate_exact_size(
                            egui::vec2(20.0, 20.0),
                            egui::Sense::hover(),
                        );
                        let center = jack_rect.center();
                        draw_jack(ui.painter(), center, false);
                        ui.label("IN");
                        jacks.push(JackPos {
                            port: PortRef {
                                node: node.id,
                                port: PortId::new(i),
                            },
                            center,
                            is_output: false,
                        });
                    });
                }

                for i in 0..counts.outputs {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (jack_rect, _) = ui.allocate_exact_size(
                                egui::vec2(20.0, 20.0),
                                egui::Sense::hover(),
                            );
                            let center = jack_rect.center();
                            draw_jack(ui.painter(), center, true);
                            ui.label("OUT");
                            jacks.push(JackPos {
                                port: PortRef {
                                    node: node.id,
                                    port: PortId::new(i),
                                },
                                center,
                                is_output: true,
                            });
                        });
                    });
                }
            });
        });

    let _ = height;
    NodeLayout {
        rect: area_response.response.rect,
        jacks,
    }
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

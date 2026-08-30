//! Patch canvas: nodes, cables, interaction.

mod cable;
mod knob;
mod node_view;

use std::collections::HashMap;

use eframe::egui;
use rtrb::Producer;
use waver_core::{NodeId, NodeKind, ParamId, RtCommand, param_label};

use crate::patch_state::PatchState;

pub use cable::CableState;

use self::cable::{draw_cable, jack_at, JackPos};
use self::knob::{KnobScale, rotary_knob, wave_selector};
use self::node_view::show_node;

const NODE_HEADER_HEIGHT: f32 = 24.0;

/// Patch editor widget.
pub struct PatchEditor {
    cable: CableState,
    drag_node: Option<NodeId>,
    drag_offset: egui::Vec2,
    jack_cache: HashMap<NodeId, Vec<JackPos>>,
}

impl Default for PatchEditor {
    fn default() -> Self {
        Self {
            cable: CableState::Idle,
            drag_node: None,
            drag_offset: egui::Vec2::ZERO,
            jack_cache: HashMap::new(),
        }
    }
}

impl PatchEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchState,
        commands: &mut Producer<RtCommand>,
    ) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cable.cancel();
        }

        ui.horizontal(|ui| {
            if ui.button("+ VCO").clicked() {
                patch.add_node(NodeKind::Vco);
                patch.recompile(commands);
            }
            if ui.button("+ Output").clicked() {
                patch.add_node(NodeKind::Output);
                patch.recompile(commands);
            }
            if ui.button("删除选中").clicked() && patch.remove_selected() {
                patch.recompile(commands);
            }
        });

        if let Some(err) = &patch.compile_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err.to_string());
        }

        ui.separator();

        let (response, painter) = ui.allocate_painter(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(28, 30, 36));

        self.jack_cache.clear();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut all_jacks: Vec<JackPos> = Vec::new();
        let params = patch.compiled.as_ref().map(|p| p.params.clone());

        let nodes: Vec<_> = patch.graph.nodes().to_vec();
        for node in &nodes {
            let pos = patch.position(node.id);
            let selected = patch.selected == Some(node.id);
            let layout = show_node(
                ui,
                node,
                pos,
                selected,
                params.as_ref(),
            );
            self.jack_cache.insert(node.id, layout.jacks.clone());
            all_jacks.extend(layout.jacks);

            let header_rect = egui::Rect::from_min_size(
                layout.rect.min,
                egui::vec2(layout.rect.width(), NODE_HEADER_HEIGHT),
            );

            if response.clicked() {
                if header_rect.contains(pointer.unwrap_or(egui::Pos2::ZERO)) {
                    patch.selected = Some(node.id);
                } else if !layout.rect.contains(pointer.unwrap_or(egui::Pos2::ZERO))
                    && patch.selected.is_some()
                {
                    patch.selected = None;
                }
            }

            if response.drag_started()
                && header_rect.contains(pointer.unwrap_or(egui::Pos2::ZERO))
            {
                self.drag_node = Some(node.id);
                self.drag_offset = pointer.unwrap() - pos;
            }
        }

        if response.dragged() {
            if let (Some(id), Some(p)) = (self.drag_node, pointer) {
                patch.set_position(id, p - self.drag_offset);
            }
        }
        if response.drag_stopped() {
            self.drag_node = None;
        }

        self.draw_edges(&painter, patch, &all_jacks);

        if let Some(p) = pointer {
            self.handle_cable_input(ui, patch, commands, p, &all_jacks);
        }

        if let CableState::Dragging { from_pos, .. } = &self.cable {
            if let Some(p) = pointer {
                draw_cable(
                    &painter,
                    *from_pos,
                    p,
                    egui::Color32::from_rgb(180, 180, 100),
                    2.0,
                    true,
                );
            }
        }

        ui.separator();
        self.param_panel(ui, patch);
    }

    fn draw_edges(
        &self,
        painter: &egui::Painter,
        patch: &PatchState,
        jacks: &[JackPos],
    ) {
        for edge in patch.graph.edges() {
            let from = jacks.iter().find(|j| j.port == edge.from);
            let to = jacks.iter().find(|j| j.port == edge.to);
            if let (Some(from), Some(to)) = (from, to) {
                draw_cable(
                    painter,
                    from.center,
                    to.center,
                    egui::Color32::from_rgb(200, 200, 80),
                    2.5,
                    false,
                );
            }
        }
    }

    fn handle_cable_input(
        &mut self,
        ui: &egui::Ui,
        patch: &mut PatchState,
        commands: &mut Producer<RtCommand>,
        pointer: egui::Pos2,
        jacks: &[JackPos],
    ) {
        if ui.input(|i| i.pointer.secondary_clicked()) {
            for (idx, edge) in patch.graph.edges().iter().enumerate() {
                let from = jacks.iter().find(|j| j.port == edge.from);
                let to = jacks.iter().find(|j| j.port == edge.to);
                if let (Some(from), Some(to)) = (from, to) {
                    let mid = from.center.lerp(to.center, 0.5);
                    if pointer.distance(mid) < 16.0 {
                        if patch.disconnect_edge(idx) {
                            patch.recompile(commands);
                        }
                        return;
                    }
                }
            }
            self.cable.cancel();
            return;
        }

        if ui.input(|i| i.pointer.primary_clicked()) {
            if let Some(jack) = jack_at(pointer, jacks) {
                match &self.cable {
                    CableState::Idle => {
                        if jack.is_output {
                            self.cable.start_drag(jack.port, jack.center);
                        }
                    }
                    CableState::Dragging { from, .. } => {
                        if !jack.is_output && jack.port.node != from.node {
                            if patch.try_connect(*from, jack.port) {
                                patch.recompile(commands);
                            }
                            self.cable.cancel();
                        } else if jack.is_output {
                            self.cable.start_drag(jack.port, jack.center);
                        }
                    }
                }
            } else {
                self.cable.cancel();
            }
        }
    }

    fn param_panel(&self, ui: &mut egui::Ui, patch: &PatchState) {
        let Some(selected) = patch.selected else {
            ui.label("选中节点标题栏以编辑；VCO 旋钮可直接在模块上拖动。");
            return;
        };
        let Some(node) = patch.graph.node(selected) else {
            return;
        };
        let Some(compiled) = &patch.compiled else {
            ui.label("补丁尚未编译。");
            return;
        };

        if node.kind == NodeKind::Vco {
            ui.label(
                egui::RichText::new("VCO：拖动 FREQ / AMP 旋钮，或点击波形按钮。双击旋钮恢复默认。")
                    .color(egui::Color32::from_gray(160)),
            );
            ui.horizontal(|ui| {
                if let Some(freq) = compiled.params.get(selected, ParamId::new(0)) {
                    let mut v = freq.value();
                    if rotary_knob(
                        ui,
                        egui::Id::new(("panel_knob", selected.raw(), 0u32)),
                        "频率",
                        &mut v,
                        20.0..=2000.0,
                        KnobScale::Logarithmic,
                    )
                    .changed()
                    {
                        freq.set(v);
                    }
                }
                if let Some(amp) = compiled.params.get(selected, ParamId::new(1)) {
                    let mut v = amp.value();
                    if rotary_knob(
                        ui,
                        egui::Id::new(("panel_knob", selected.raw(), 1u32)),
                        "振幅",
                        &mut v,
                        0.0..=1.0,
                        KnobScale::Linear,
                    )
                    .changed()
                    {
                        amp.set(v);
                    }
                }
                if let Some(wave) = compiled.params.get(selected, ParamId::new(2)) {
                    let mut v = wave.value();
                    wave_selector(ui, &mut v);
                    if (v - wave.value()).abs() > f32::EPSILON {
                        wave.set(v);
                    }
                }
            });
            return;
        }

        ui.heading(format!("{} 参数", kind_label(node.kind)));
        let param_count = node.kind.port_counts().params;
        for raw in 0..param_count {
            let param = ParamId::new(raw);
            let Some(cell) = compiled.params.get(selected, param) else {
                continue;
            };
            let label = param_label(node.kind, param);
            let mut value = cell.value();
            ui.add(egui::Slider::new(&mut value, 0.0..=1.0).text(label));
            cell.set(value);
        }
    }
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Vco => "VCO",
        NodeKind::Output => "Output",
        NodeKind::Delay => "Delay",
        _ => "节点",
    }
}

//! Patch canvas with painter-based nodes and manual hit testing.

mod cable;
mod knob;
mod node_view;

use std::collections::HashMap;

use eframe::egui;
use rtrb::Producer;
use waver_core::{NodeId, NodeKind, ParamId, RtCommand};

use crate::patch_state::PatchState;

pub use cable::CableState;

use self::cable::{
    cable_distance, draw_cable, JackPos, CABLE_HIT_RADIUS, JACK_HIT_RADIUS,
};
use self::knob::{KnobScale, rotary_knob, wave_selector};
use self::node_view::{draw_node_visual, hit_test_node, NodeHit, VCO_SIZE};

#[derive(Clone, Copy)]
enum DragKind {
    Node { id: NodeId, grab_offset: egui::Vec2 },
    Knob {
        node: NodeId,
        param: u32,
        last_pointer: egui::Pos2,
    },
}

/// Patch editor widget.
pub struct PatchEditor {
    cable: CableState,
    drag: Option<DragKind>,
    jack_cache: Vec<JackPos>,
    node_rects: HashMap<NodeId, egui::Rect>,
    /// Last hovered cable (kept so Delete / toolbar button still work).
    hovered_cable: Option<usize>,
}

impl Default for PatchEditor {
    fn default() -> Self {
        Self {
            cable: CableState::Idle,
            drag: None,
            jack_cache: Vec::new(),
            node_rects: HashMap::new(),
            hovered_cable: None,
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
            self.drag = None;
        }

        ui.horizontal(|ui| {
            if ui.button("+ VCO").clicked() {
                let id = patch.add_node(NodeKind::Vco);
                patch.selected = Some(id);
                patch.recompile(commands);
            }
            if ui.button("+ Output").clicked() {
                let id = patch.add_node(NodeKind::Output);
                patch.selected = Some(id);
                patch.recompile(commands);
            }
            if ui.button("删除选中").clicked() && patch.remove_selected() {
                patch.recompile(commands);
            }
            if ui.button("删除连线").clicked() {
                let idx = self.hovered_cable.or_else(|| {
                    if patch.graph.edges().len() == 1 {
                        Some(0)
                    } else {
                        None
                    }
                });
                if let Some(idx) = idx {
                    if patch.disconnect_edge(idx) {
                        self.hovered_cable = None;
                        patch.recompile(commands);
                    }
                }
            }
            ui.label(
                egui::RichText::new("拖标题移动 · 拖旋钮调参 · 点 OUT→IN 连线 · 悬停线 Delete 删除")
                    .color(egui::Color32::from_gray(150))
                    .size(12.0),
            );
        });

        if let Some(err) = &patch.compile_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err.to_string());
        }

        ui.separator();

        let param_height = 120.0;
        let canvas_height = (ui.available_height() - param_height - 8.0).max(160.0);
        let (response, painter) =
            ui.allocate_painter(egui::vec2(ui.available_width(), canvas_height), egui::Sense::click_and_drag());
        let canvas_rect = response.rect;
        painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_rgb(28, 30, 36));

        let pointer = response.interact_pointer_pos().or_else(|| {
            ui.input(|i| i.pointer.hover_pos())
        });

        // Rebuild geometry for this frame.
        self.jack_cache.clear();
        self.node_rects.clear();
        let nodes: Vec<_> = patch.graph.nodes().to_vec();
        for node in &nodes {
            let pos = patch.position(node.id);
            let size = match node.kind {
                NodeKind::Vco => VCO_SIZE,
                _ => egui::vec2(140.0, 72.0),
            };
            // Keep nodes inside canvas on first layout if needed.
            let pos = {
                let mut p = pos;
                if p.x < canvas_rect.left() || p.y < canvas_rect.top() {
                    p = canvas_rect.min
                        + egui::vec2(30.0 + node.id.raw() as f32 * 280.0, 40.0);
                    patch.set_position(node.id, p);
                }
                p
            };
            let rect = egui::Rect::from_min_size(pos, size);
            self.node_rects.insert(node.id, rect);
            let jacks = draw_node_visual(
                &painter,
                node,
                rect,
                patch.selected == Some(node.id),
                patch.compiled.as_ref().map(|c| &c.params),
            );
            self.jack_cache.extend(jacks);
        }

        // Hovered cable highlight + draw.
        let hovered_now = pointer.and_then(|p| nearest_cable_index(p, patch, &self.jack_cache));
        if hovered_now.is_some() {
            self.hovered_cable = hovered_now;
            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
        } else if pointer.is_some() {
            // Clear hover when pointer is on canvas but not near a cable.
            if pointer.is_some_and(|p| canvas_rect.contains(p)) {
                let over_node = self.node_rects.values().any(|r| r.contains(pointer.unwrap()));
                if !over_node {
                    self.hovered_cable = None;
                }
            }
        }
        let hovered_cable = self.hovered_cable;
        for (idx, edge) in patch.graph.edges().iter().enumerate() {
            let Some(from) = self.jack_cache.iter().find(|j| j.port == edge.from) else {
                continue;
            };
            let Some(to) = self.jack_cache.iter().find(|j| j.port == edge.to) else {
                continue;
            };
            let hot = hovered_cable == Some(idx);
            draw_cable(
                &painter,
                from.center,
                to.center,
                if hot {
                    egui::Color32::from_rgb(255, 120, 80)
                } else {
                    egui::Color32::from_rgb(200, 200, 80)
                },
                if hot { 3.5 } else { 2.5 },
                false,
            );
        }

        // Explicit delete-cable control when hovering (more reliable than OS-eaten right-click).
        if let Some(idx) = hovered_cable {
            egui::Area::new(egui::Id::new("cable_delete_hint"))
                .fixed_pos(pointer.unwrap_or(canvas_rect.center()) + egui::vec2(12.0, 12.0))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    if ui
                        .add(
                            egui::Button::new("删除连线")
                                .fill(egui::Color32::from_rgb(180, 60, 50)),
                        )
                        .clicked()
                        && patch.disconnect_edge(idx)
                    {
                        patch.recompile(commands);
                    }
                });
        }

        // Live cable preview.
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

        // --- Interaction (manual) ---
        self.handle_pointer(ui, &response, patch, commands, pointer, hovered_cable, canvas_rect);

        ui.add_space(4.0);
        ui.separator();
        self.param_panel(ui, patch);
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_pointer(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        patch: &mut PatchState,
        commands: &mut Producer<RtCommand>,
        pointer: Option<egui::Pos2>,
        hovered_cable: Option<usize>,
        canvas_rect: egui::Rect,
    ) {
        let Some(pointer) = pointer else {
            return;
        };

        let primary_down = ui.input(|i| i.pointer.primary_down());
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_released = ui.input(|i| i.pointer.primary_released());
        let pointer_delta = ui.input(|i| i.pointer.delta());
        let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let delete_key = ui.input(|i| {
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
        });

        // Double-click near a cable also deletes it.
        if response.double_clicked() {
            if let Some(idx) = hovered_cable {
                if patch.disconnect_edge(idx) {
                    patch.recompile(commands);
                }
                self.drag = None;
                return;
            }
        }

        if (secondary_clicked || delete_key) && hovered_cable.is_some() {
            if let Some(idx) = hovered_cable {
                if patch.disconnect_edge(idx) {
                    patch.recompile(commands);
                }
            }
            self.drag = None;
            return;
        }
        if secondary_clicked {
            self.cable.cancel();
            self.drag = None;
            return;
        }

        // Begin drag / click on press.
        if primary_pressed {
            if let Some(jack) = self
                .jack_cache
                .iter()
                .copied()
                .find(|j| j.center.distance(pointer) <= JACK_HIT_RADIUS)
            {
                self.on_jack_click(patch, commands, jack);
                self.drag = None;
                return;
            }
            if let Some((id, hit, rect)) = top_hit(patch, &self.node_rects, pointer) {
                patch.selected = Some(id);
                match hit {
                    NodeHit::Header | NodeHit::Body => {
                        self.drag = Some(DragKind::Node {
                            id,
                            grab_offset: pointer - rect.min,
                        });
                    }
                    NodeHit::Knob { param } => {
                        self.drag = Some(DragKind::Knob {
                            node: id,
                            param,
                            last_pointer: pointer,
                        });
                    }
                    NodeHit::Wave { index } => {
                        if let Some(compiled) = &patch.compiled {
                            if let Some(cell) = compiled.params.get(id, ParamId::new(2)) {
                                cell.set(index as f32);
                            }
                        }
                        self.drag = None;
                    }
                }
                return;
            }
            // Empty canvas press
            patch.selected = None;
            self.cable.cancel();
            self.drag = None;
            return;
        }

        // Update ongoing drag with raw pointer delta (more reliable than Response::dragged).
        if primary_down {
            match self.drag {
                Some(DragKind::Node { id, grab_offset }) => {
                    let next = pointer - grab_offset;
                    let size = self
                        .node_rects
                        .get(&id)
                        .map(|r| r.size())
                        .unwrap_or(egui::vec2(140.0, 72.0));
                    let clamped = egui::pos2(
                        next.x.clamp(
                            canvas_rect.left(),
                            (canvas_rect.right() - size.x).max(canvas_rect.left()),
                        ),
                        next.y.clamp(
                            canvas_rect.top(),
                            (canvas_rect.bottom() - size.y).max(canvas_rect.top()),
                        ),
                    );
                    patch.set_position(id, clamped);
                    let _ = pointer_delta;
                    let _ = response;
                    return;
                }
                Some(DragKind::Knob {
                    node,
                    param,
                    last_pointer,
                }) => {
                    if let Some(compiled) = &patch.compiled {
                        if let Some(cell) = compiled.params.get(node, ParamId::new(param)) {
                            let dy = last_pointer.y - pointer.y;
                            let mut v = cell.value();
                            if param == 0 {
                                let log_v = v.max(20.0).ln();
                                v = (log_v + dy * 0.012).exp().clamp(20.0, 2000.0);
                            } else if param == 1 {
                                v = (v + dy * 0.006).clamp(0.0, 1.0);
                            }
                            cell.set(v);
                        }
                    }
                    self.drag = Some(DragKind::Knob {
                        node,
                        param,
                        last_pointer: pointer,
                    });
                    return;
                }
                None => {}
            }
        }

        if primary_released {
            self.drag = None;
        }
    }

    fn on_jack_click(
        &mut self,
        patch: &mut PatchState,
        commands: &mut Producer<RtCommand>,
        jack: JackPos,
    ) {
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
                } else {
                    self.cable.cancel();
                }
            }
        }
    }

    fn param_panel(&self, ui: &mut egui::Ui, patch: &PatchState) {
        // Always expose first VCO params so knobs are usable even without selection.
        let vco_id = patch
            .selected
            .filter(|id| patch.graph.node(*id).map(|n| n.kind) == Some(NodeKind::Vco))
            .or_else(|| {
                patch
                    .graph
                    .nodes()
                    .iter()
                    .find(|n| n.kind == NodeKind::Vco)
                    .map(|n| n.id)
            });

        let Some(selected) = vco_id else {
            ui.label("添加 VCO 以编辑参数。");
            return;
        };
        let Some(compiled) = &patch.compiled else {
            ui.label("补丁尚未编译。");
            return;
        };

        ui.label(
            egui::RichText::new("VCO 参数（底部旋钮与模块旋钮同步）")
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
                if wave_selector(ui, &mut v) {
                    wave.set(v);
                }
            }
        });
    }
}

fn top_hit(
    patch: &PatchState,
    node_rects: &HashMap<NodeId, egui::Rect>,
    pointer: egui::Pos2,
) -> Option<(NodeId, NodeHit, egui::Rect)> {
    for node in patch.graph.nodes().iter().rev() {
        if let Some(rect) = node_rects.get(&node.id).copied() {
            if let Some(hit) = hit_test_node(node.kind, rect, pointer) {
                return Some((node.id, hit, rect));
            }
        }
    }
    None
}

fn nearest_cable_index(
    pointer: egui::Pos2,
    patch: &PatchState,
    jacks: &[JackPos],
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (idx, edge) in patch.graph.edges().iter().enumerate() {
        let Some(from) = jacks.iter().find(|j| j.port == edge.from) else {
            continue;
        };
        let Some(to) = jacks.iter().find(|j| j.port == edge.to) else {
            continue;
        };
        let d = cable_distance(pointer, from.center, to.center);
        if d <= CABLE_HIT_RADIUS && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((idx, d));
        }
    }
    best.map(|(idx, _)| idx)
}

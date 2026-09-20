//! Patch canvas with painter-based nodes and manual hit testing.

mod cable;
mod knob;
mod node_view;

use std::collections::HashMap;

use eframe::egui;
use rtrb::Producer;
use waver_core::{NodeId, NodeKind, ParamId, RtCommand};

use crate::patch_state::PatchState;
use crate::theme;

pub use cable::CableState;

use self::cable::{CABLE_HIT_RADIUS, JACK_HIT_RADIUS, JackPos, cable_distance, draw_cable};
use self::knob::wave_selector;
use self::node_view::{NodeHit, draw_node_visual, hit_test_node, kind_label, node_size};

#[derive(Clone, Copy)]
enum DragKind {
    Node {
        id: NodeId,
        grab_offset: egui::Vec2,
    },
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

        if let Some(err) = &patch.compile_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err.to_string());
        }

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let canvas_rect = response.rect;
        painter.rect_filled(canvas_rect, 0.0, theme::CANVAS);
        for x in (0..canvas_rect.width() as i32).step_by(24) {
            for y in (0..canvas_rect.height() as i32).step_by(24) {
                painter.circle_filled(
                    canvas_rect.min + egui::vec2(x as f32 + 1.0, y as f32 + 1.0),
                    1.0,
                    egui::Color32::from_rgb(47, 50, 58),
                );
            }
        }
        painter.text(
            canvas_rect.min + egui::vec2(28.0, 28.0),
            egui::Align2::LEFT_TOP,
            "PATCH 画布 · 拖动模块 · 连接端口",
            egui::FontId::proportional(12.0),
            theme::MUTED,
        );

        let pointer = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|i| i.pointer.hover_pos()));

        // Rebuild geometry for this frame.
        self.jack_cache.clear();
        self.node_rects.clear();
        let nodes: Vec<_> = patch.graph.nodes().to_vec();
        for node in &nodes {
            // Positions are canvas-local, so panel/window changes do not move the patch.
            let size = node_size(node.kind);
            let local = patch.position(node.id);
            let local = egui::pos2(
                local.x.clamp(0.0, (canvas_rect.width() - size.x).max(0.0)),
                local
                    .y
                    .clamp(60.0, (canvas_rect.height() - size.y).max(60.0)),
            );
            let pos = canvas_rect.min + local.to_vec2();
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

        // Hovered cable: sticky index kept for Delete/toolbar, but highlight + floating
        // button only while the pointer is actually near a cable this frame.
        let hovered_now = pointer
            .filter(|p| canvas_rect.contains(*p))
            .and_then(|p| nearest_cable_index(p, patch, &self.jack_cache));
        if let Some(idx) = hovered_now {
            self.hovered_cable = Some(idx);
            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
        } else if let Some(p) = pointer {
            if canvas_rect.contains(p) {
                // Leaving the cable path clears sticky hover so the floating delete
                // Area cannot linger over nodes/knobs (was area_shown:true during drags).
                let still_near_sticky = self.hovered_cable.and_then(|idx| {
                    let edge = patch.graph.edges().get(idx)?;
                    let from = self.jack_cache.iter().find(|j| j.port == edge.from)?;
                    let to = self.jack_cache.iter().find(|j| j.port == edge.to)?;
                    let d = cable_distance(p, from.center, to.center);
                    Some(d <= CABLE_HIT_RADIUS * 2.5)
                });
                if still_near_sticky != Some(true) {
                    self.hovered_cable = None;
                }
            }
        }
        let sticky_cable = self.hovered_cable;
        let near_cable = hovered_now;
        for (idx, edge) in patch.graph.edges().iter().enumerate() {
            let Some(from) = self.jack_cache.iter().find(|j| j.port == edge.from) else {
                continue;
            };
            let Some(to) = self.jack_cache.iter().find(|j| j.port == edge.to) else {
                continue;
            };
            let hot = near_cable == Some(idx);
            draw_cable(
                &painter,
                from.center,
                to.center,
                if hot {
                    egui::Color32::from_rgb(255, 120, 80)
                } else {
                    theme::CABLE
                },
                if hot { 3.5 } else { 2.5 },
                false,
            );
        }

        // Floating delete only while pointer is near a cable — never while sticky-only
        // over a module (that overlay stole/confused knob and header hits).
        if let Some(idx) = near_cable {
            // Anchor away from the pointer so the button does not cover the cable hit.
            let anchor = pointer.unwrap_or(canvas_rect.center()) + egui::vec2(18.0, -36.0);
            egui::Area::new(egui::Id::new("cable_delete_hint"))
                .fixed_pos(anchor)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    let resp = ui.add(
                        egui::Button::new("删除连线").fill(egui::Color32::from_rgb(180, 60, 50)),
                    );
                    if resp.clicked() && patch.disconnect_edge(idx) {
                        self.hovered_cable = None;
                        patch.recompile(commands);
                    }
                });
        }
        let hovered_cable = sticky_cable.or(near_cable);

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
        self.handle_pointer(
            ui,
            &response,
            patch,
            commands,
            pointer,
            hovered_cable,
            canvas_rect,
        );
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
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_released = ui.input(|i| i.pointer.primary_released());
        let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let delete_key = !ui.ctx().egui_wants_keyboard_input()
            && ui
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));

        let Some(pointer) = pointer else {
            return;
        };

        // Sidebars and popup controls own their pointer events. Only an active drag
        // may continue outside the canvas; never clear selection from an inspector click.
        if self.drag.is_none() && !response.contains_pointer() {
            return;
        }

        // Double-click near a cable also deletes it.
        if response.double_clicked() {
            if let Some(idx) = hovered_cable {
                if patch.disconnect_edge(idx) {
                    self.hovered_cable = None;
                    patch.recompile(commands);
                }
                self.drag = None;
                return;
            }
        }

        if (secondary_clicked || delete_key) && hovered_cable.is_some() {
            if let Some(idx) = hovered_cable {
                if patch.disconnect_edge(idx) {
                    self.hovered_cable = None;
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
        if primary_pressed && canvas_rect.contains(pointer) {
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
                    // Only the title bar moves modules — Body used to steal knob misses.
                    NodeHit::Header => {
                        self.hovered_cable = None;
                        self.drag = Some(DragKind::Node {
                            id,
                            grab_offset: pointer - rect.min,
                        });
                    }
                    NodeHit::Body => {
                        self.hovered_cable = None;
                        self.drag = None;
                    }
                    NodeHit::Knob { param } => {
                        self.hovered_cable = None;
                        self.drag = Some(DragKind::Knob {
                            node: id,
                            param,
                            last_pointer: pointer,
                        });
                    }
                    NodeHit::Wave { index } => {
                        self.hovered_cable = None;
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
            self.hovered_cable = None;
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
                    patch.set_position(id, (clamped - canvas_rect.min).to_pos2());
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

    pub fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        patch: &mut PatchState,
        commands: &mut Producer<RtCommand>,
    ) {
        ui.heading("属性检查器");
        let Some(node) = patch.selected.and_then(|id| patch.graph.node(id)).copied() else {
            ui.add_space(16.0);
            theme::caption(ui, "选择一个模块以查看属性");
            return;
        };
        ui.label(
            egui::RichText::new(kind_label(node.kind))
                .size(12.0)
                .color(theme::ACCENT),
        );
        ui.add_space(28.0);
        if node.kind == NodeKind::Vco {
            if let Some(compiled) = &patch.compiled {
                ui.spacing_mut().slider_width = ui.available_width();
                if let Some(freq) = compiled.params.get(node.id, ParamId::new(0)) {
                    theme::caption(ui, "频率");
                    let mut value = freq.value();
                    ui.label(
                        egui::RichText::new(format!("{value:.0} Hz"))
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut value, 20.0..=2000.0)
                                .logarithmic(true)
                                .show_value(false),
                        )
                        .changed()
                    {
                        freq.set(value);
                    }
                }
                ui.add_space(28.0);
                if let Some(amp) = compiled.params.get(node.id, ParamId::new(1)) {
                    theme::caption(ui, "振幅");
                    let mut value = amp.value();
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", value * 100.0))
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    );
                    if ui
                        .add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(false))
                        .changed()
                    {
                        amp.set(value);
                    }
                    if value == 0.0 {
                        ui.label(
                            egui::RichText::new("当前振荡器静音")
                                .size(11.0)
                                .color(theme::GOOD),
                        );
                    }
                }
                ui.add_space(28.0);
                if let Some(wave) = compiled.params.get(node.id, ParamId::new(2)) {
                    let mut value = wave.value();
                    if wave_selector(ui, &mut value) {
                        wave.set(value);
                    }
                }
            }
        } else if node.kind.port_counts().params > 0 {
            if let Some(compiled) = &patch.compiled {
                // Reserve room for the numeric editor beside a generic slider.
                ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(80.0);
                let blurb = node.kind.desc().inspector_blurb;
                if !blurb.is_empty() {
                    theme::caption(ui, blurb);
                    ui.add_space(16.0);
                }
                let param_count = node.kind.port_counts().params;
                for raw in 0..param_count {
                    let param = ParamId::new(raw);
                    let Some(cell) = compiled.params.get(node.id, param) else {
                        continue;
                    };
                    theme::caption(ui, waver_core::param_label(node.kind, param));
                    let mut value = cell.value();
                    if ui
                        .add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(true))
                        .changed()
                    {
                        cell.set(value);
                    }
                    ui.add_space(16.0);
                }
            }
        } else {
            theme::caption(ui, node.kind.desc().inspector_blurb);
        }
        ui.add_space(28.0);
        theme::caption(ui, "端口");
        let counts = node.kind.port_counts();
        if counts.inputs > 0 {
            ui.colored_label(theme::INPUT, format!("IN 输入 · {}", counts.inputs));
        }
        if counts.outputs > 0 {
            ui.colored_label(theme::OUTPUT, format!("OUT 输出 · {}", counts.outputs));
        }
        ui.add_space(24.0);
        if ui
            .add_sized(
                [ui.available_width(), 36.0],
                egui::Button::new("删除选中模块"),
            )
            .clicked()
            && patch.remove_selected()
        {
            self.cable.cancel();
            self.drag = None;
            self.hovered_cable = None;
            patch.recompile(commands);
        }
        let cable_index = self
            .hovered_cable
            .filter(|idx| *idx < patch.graph.edges().len())
            .or_else(|| (patch.graph.edges().len() == 1).then_some(0));
        if ui
            .add_enabled(
                cable_index.is_some(),
                egui::Button::new("删除连线").min_size(egui::vec2(ui.available_width(), 36.0)),
            )
            .clicked()
        {
            if let Some(index) = cable_index {
                if patch.disconnect_edge(index) {
                    self.hovered_cable = None;
                    patch.recompile(commands);
                }
            }
        }
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

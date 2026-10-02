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
            ui.colored_label(theme::DANGER, err.to_string());
        }

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let canvas_rect = response.rect;
        painter.rect_filled(canvas_rect, 0.0, theme::CANVAS);
        for x in (0..canvas_rect.width() as i32).step_by(48) {
            let x = canvas_rect.left() + x as f32;
            painter.line_segment(
                [
                    egui::pos2(x, canvas_rect.top() + 72.0),
                    egui::pos2(x, canvas_rect.bottom()),
                ],
                egui::Stroke::new(0.5, theme::GRID),
            );
        }
        for y in (72..canvas_rect.height() as i32).step_by(48) {
            let y = canvas_rect.top() + y as f32;
            painter.line_segment(
                [
                    egui::pos2(canvas_rect.left(), y),
                    egui::pos2(canvas_rect.right(), y),
                ],
                egui::Stroke::new(0.5, theme::GRID),
            );
        }
        painter.text(
            canvas_rect.min + egui::vec2(28.0, 28.0),
            egui::Align2::LEFT_TOP,
            "PATCH / 信号画布",
            egui::FontId::proportional(12.0),
            theme::MUTED,
        );

        painter.text(
            canvas_rect.min + egui::vec2(28.0, 52.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{} 个模块   /   {} 条连线",
                patch.graph.nodes().len(),
                patch.graph.edges().len()
            ),
            egui::FontId::proportional(12.0),
            theme::MUTED,
        );
        painter.line_segment(
            [
                canvas_rect.min + egui::vec2(24.0, 80.0),
                egui::pos2(canvas_rect.right() - 24.0, canvas_rect.top() + 80.0),
            ],
            egui::Stroke::new(1.0, theme::GRID),
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
                    let from = self
                        .jack_cache
                        .iter()
                        .find(|j| j.is_output && j.port == edge.from)?;
                    let to = self
                        .jack_cache
                        .iter()
                        .find(|j| !j.is_output && j.port == edge.to)?;
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
            let Some(from) = self
                .jack_cache
                .iter()
                .find(|j| j.is_output && j.port == edge.from)
            else {
                continue;
            };
            let Some(to) = self
                .jack_cache
                .iter()
                .find(|j| !j.is_output && j.port == edge.to)
            else {
                continue;
            };
            let hot = near_cable == Some(idx);
            draw_cable(
                &painter,
                from.center,
                to.center,
                if hot { theme::DANGER } else { theme::CABLE },
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
                    let resp = ui.add(egui::Button::new("删除连线").fill(theme::DANGER_SURFACE));
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
                draw_cable(&painter, *from_pos, p, theme::ACCENT, 2.0, true);
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
                            let hz_range = if param == 0 {
                                patch.graph.node(node).and_then(|n| frequency_range(n.kind))
                            } else {
                                None
                            };
                            if let Some((min_hz, max_hz)) = hz_range {
                                let log_v = v.max(min_hz).ln();
                                v = (log_v + dy * 0.012).exp().clamp(min_hz, max_hz);
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
        ui.horizontal(|ui| {
            ui.heading("属性检查器");
            theme::caption(ui, "设备 / 参数");
        });
        ui.separator();
        let Some(node) = patch.selected.and_then(|id| patch.graph.node(id)).copied() else {
            ui.add_space(16.0);
            theme::caption(ui, "选择一个模块以查看属性");
            theme::caption(ui, "点击模块选择 · 拖动标题移动 · 点击 OUT 再点击 IN 连接");
            return;
        };
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(160.0, 98.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    egui::Frame::new()
                        .fill(theme::ACCENT)
                        .inner_margin(6)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(kind_label(node.kind))
                                    .strong()
                                    .color(theme::TEXT),
                            );
                        });
                    ui.add_space(4.0);
                    let counts = node.kind.port_counts();
                    theme::caption(
                        ui,
                        &format!("{} 输入 / {} 输出", counts.inputs, counts.outputs),
                    );
                    theme::caption(ui, "参数实时生效");
                    theme::caption(ui, "Esc 取消连接");
                },
            );
            ui.separator();
            if let Some(compiled) = &patch.compiled {
                for raw in 0..node.kind.port_counts().params {
                    let Some(cell) = compiled.params.get(node.id, ParamId::new(raw)) else {
                        continue;
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(148.0, 98.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.spacing_mut().slider_width = 148.0;
                            let mut value = cell.value();
                            if node.kind == NodeKind::Vco && raw == 2 {
                                if wave_selector(ui, &mut value) {
                                    cell.set(value);
                                }
                            } else {
                                theme::caption(
                                    ui,
                                    waver_core::param_label(node.kind, ParamId::new(raw)),
                                );
                                ui.add_space(4.0);
                                let frequency = if raw == 0 {
                                    frequency_range(node.kind)
                                } else {
                                    None
                                };
                                ui.label(
                                    egui::RichText::new(match frequency {
                                        Some(_) => format!("{value:.0} Hz"),
                                        None => format!("{:.0}%", value * 100.0),
                                    })
                                    .size(20.0)
                                    .color(theme::TEXT),
                                );
                                let changed = if let Some((min_hz, max_hz)) = frequency {
                                    ui.add(
                                        egui::Slider::new(&mut value, min_hz..=max_hz)
                                            .logarithmic(true)
                                            .show_value(false),
                                    )
                                    .changed()
                                } else {
                                    ui.spacing_mut().slider_width = 96.0;
                                    ui.add(
                                        egui::Slider::new(&mut value, 0.0..=1.0).show_value(true),
                                    )
                                    .changed()
                                };
                                if changed {
                                    cell.set(value);
                                }
                            }
                        },
                    );
                }
            }
            if node.kind.port_counts().params == 0 {
                ui.allocate_ui_with_layout(
                    egui::vec2(220.0, 98.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        theme::caption(ui, node.kind.desc().inspector_blurb);
                    },
                );
            }
            ui.separator();
            ui.vertical(|ui| {
                theme::caption(ui, "编辑");
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("删除选中模块").color(theme::DANGER),
                    ))
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
                    .add_enabled(cable_index.is_some(), egui::Button::new("删除连线"))
                    .clicked()
                {
                    if let Some(index) = cable_index {
                        if patch.disconnect_edge(index) {
                            self.hovered_cable = None;
                            patch.recompile(commands);
                        }
                    }
                }
            });
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
        let Some(from) = jacks.iter().find(|j| j.is_output && j.port == edge.from) else {
            continue;
        };
        let Some(to) = jacks.iter().find(|j| !j.is_output && j.port == edge.to) else {
            continue;
        };
        let d = cable_distance(pointer, from.center, to.center);
        if d <= CABLE_HIT_RADIUS && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((idx, d));
        }
    }
    best.map(|(idx, _)| idx)
}

/// 参数 0 是频率（Hz）的模块：给出 `(下限, 上限)` 的对数范围。
///
/// `Vco` 的上限与 `node_view.rs` 里画旋钮弧度用的 20–2000 映射一致；
/// 改这里必须同时改那张卡的绘制，否则旋钮位置与数值会不一致。
fn frequency_range(kind: NodeKind) -> Option<(f32, f32)> {
    match kind {
        NodeKind::Vco => Some((20.0, 2_000.0)),
        NodeKind::Vcf => Some((20.0, 20_000.0)),
        _ => None,
    }
}

#[cfg(test)]
mod param_tests {
    use super::*;

    #[test]
    fn only_frequency_parameters_use_hertz_ranges() {
        assert_eq!(frequency_range(NodeKind::Vco), Some((20.0, 2_000.0)));
        assert_eq!(frequency_range(NodeKind::Vcf), Some((20.0, 20_000.0)));
        assert_eq!(frequency_range(NodeKind::Noise), None);
        assert_eq!(frequency_range(NodeKind::Vca), None);
    }
}

#[cfg(test)]
mod cable_routing_tests {
    use super::*;
    use waver_core::{PortId, PortRef};

    #[test]
    fn filter_output_cable_hit_test_uses_output_not_same_numbered_input() {
        let mut patch = PatchState::default_patch();
        let filter = patch.add_node(NodeKind::Vcf);
        let sink = patch.add_node(NodeKind::Output);
        let from = PortRef {
            node: filter,
            port: PortId::new(0),
        };
        let to = PortRef {
            node: sink,
            port: PortId::new(0),
        };
        patch.try_connect(from, to);
        let index = patch.graph.edges().len() - 1;
        // Input and output port 0 deliberately share the same PortRef.
        // Inputs are cached first, exactly as in draw_node_visual.
        let jacks = [
            JackPos {
                port: from,
                center: egui::pos2(0.0, 100.0),
                is_output: false,
            },
            JackPos {
                port: from,
                center: egui::pos2(180.0, 100.0),
                is_output: true,
            },
            JackPos {
                port: to,
                center: egui::pos2(360.0, 200.0),
                is_output: false,
            },
        ];
        assert_eq!(
            nearest_cable_index(jacks[1].center, &patch, &jacks),
            Some(index)
        );
        assert_eq!(nearest_cable_index(jacks[0].center, &patch, &jacks), None);
    }
}

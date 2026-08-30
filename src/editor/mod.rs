//! Patch canvas with painter-based nodes and manual hit testing.

mod cable;
mod knob;
mod node_view;

use std::collections::HashMap;
use std::io::Write;

use eframe::egui;
use rtrb::Producer;
use waver_core::{NodeId, NodeKind, ParamId, RtCommand};

use crate::patch_state::PatchState;

// #region agent log
fn waver_dbg(hypothesis_id: &str, location: &str, message: &str, data: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!(
        "{{\"id\":\"log_{ts}_{hypothesis_id}\",\"timestamp\":{ts},\"location\":{location:?},\"message\":{message:?},\"data\":{data},\"hypothesisId\":{hypothesis_id:?}}}\n"
    );
    eprintln!("WAVER_DBG {hypothesis_id} {location} {message} {data}");
    for path in ["/opt/cursor/logs/debug.log", "/tmp/waver_ui_debug.log"] {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn dbg_frame() -> u64 {
    static F: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    F.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn drag_tag(drag: &Option<DragKind>) -> String {
    match drag {
        None => "None".into(),
        Some(DragKind::Node { id, grab_offset }) => {
            format!("Node(id={},ox={:.1},oy={:.1})", id.raw(), grab_offset.x, grab_offset.y)
        }
        Some(DragKind::Knob { node, param, last_pointer }) => {
            format!(
                "Knob(node={},param={},lx={:.1},ly={:.1})",
                node.raw(),
                param,
                last_pointer.x,
                last_pointer.y
            )
        }
    }
}
// #endregion

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

// #region agent log
struct PointerDbg {
    frame: u64,
    interact_pos: Option<egui::Pos2>,
    hover_pos: Option<egui::Pos2>,
    area_shown: bool,
    area_hovered: bool,
    area_clicked: bool,
}
// #endregion

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
            let del_cable = ui.button("删除连线");
            // #region agent log
            {
                static TB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = TB.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 3 {
                    waver_dbg(
                        "F",
                        "editor/mod.rs:toolbar",
                        "delete_cable_btn_rect",
                        &format!(
                            "{{\"rect\":[{:.1},{:.1},{:.1},{:.1}]}}",
                            del_cable.rect.min.x,
                            del_cable.rect.min.y,
                            del_cable.rect.max.x,
                            del_cable.rect.max.y
                        ),
                    );
                }
            }
            // #endregion
            if del_cable.clicked() {
                let edge_len = patch.graph.edges().len();
                let idx = self.hovered_cable.or_else(|| {
                    if edge_len == 1 {
                        Some(0)
                    } else {
                        None
                    }
                });
                // #region agent log
                waver_dbg(
                    "F",
                    "editor/mod.rs:toolbar_delete",
                    "toolbar_delete_cable_clicked",
                    &format!(
                        "{{\"hovered_cable\":{},\"edge_len\":{edge_len},\"idx\":{}}}",
                        self.hovered_cable.map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
                        idx.map(|i| i.to_string()).unwrap_or_else(|| "null".into())
                    ),
                );
                // #endregion
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
        let interact_pos = response.interact_pointer_pos();
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        let frame = dbg_frame();

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
                let would_reset = p.x < canvas_rect.left() || p.y < canvas_rect.top();
                if would_reset {
                    let old = p;
                    p = canvas_rect.min
                        + egui::vec2(30.0 + node.id.raw() as f32 * 280.0, 40.0);
                    // #region agent log
                    waver_dbg(
                        "A",
                        "editor/mod.rs:layout_reset",
                        "position_reset_branch",
                        &format!(
                            "{{\"frame\":{frame},\"id\":{},\"old\":[{:.1},{:.1}],\"new\":[{:.1},{:.1}],\"canvas\":[{:.1},{:.1},{:.1},{:.1}]}}",
                            node.id.raw(),
                            old.x, old.y, p.x, p.y,
                            canvas_rect.min.x, canvas_rect.min.y, canvas_rect.max.x, canvas_rect.max.y
                        ),
                    );
                    // #endregion
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

        // #region agent log
        if frame < 5 {
            let nodes_json: String = self
                .node_rects
                .iter()
                .map(|(id, r)| {
                    format!(
                        "{{\"id\":{},\"rect\":[{:.1},{:.1},{:.1},{:.1}],\"header\":[{:.1},{:.1}],\"amp_knob\":[{:.1},{:.1}],\"freq_knob\":[{:.1},{:.1}]}}",
                        id.raw(),
                        r.min.x, r.min.y, r.max.x, r.max.y,
                        r.min.x + r.width() * 0.5, r.min.y + 13.0,
                        r.left() + 92.0, r.top() + 64.0,
                        r.left() + 36.0, r.top() + 64.0
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let jacks_json: String = self
                .jack_cache
                .iter()
                .map(|j| {
                    format!(
                        "{{\"out\":{},\"c\":[{:.1},{:.1}]}}",
                        j.is_output, j.center.x, j.center.y
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            waver_dbg(
                "E",
                "editor/mod.rs:layout_dump",
                "canvas_and_nodes",
                &format!(
                    "{{\"frame\":{frame},\"canvas\":[{:.1},{:.1},{:.1},{:.1}],\"nodes\":[{nodes_json}],\"jacks\":[{jacks_json}]}}",
                    canvas_rect.min.x, canvas_rect.min.y, canvas_rect.max.x, canvas_rect.max.y
                ),
            );
        }
        // #endregion

        // Hovered cable: sticky index kept for Delete/toolbar, but highlight + floating
        // button only while the pointer is actually near a cable this frame.
        let hovered_now = pointer.and_then(|p| nearest_cable_index(p, patch, &self.jack_cache));
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
                    egui::Color32::from_rgb(200, 200, 80)
                },
                if hot { 3.5 } else { 2.5 },
                false,
            );
        }

        // Floating delete only while pointer is near a cable — never while sticky-only
        // over a module (that overlay stole/confused knob and header hits).
        let mut area_shown = false;
        let mut area_clicked = false;
        let mut area_hovered = false;
        if let Some(idx) = near_cable {
            area_shown = true;
            // Anchor away from the pointer so the button does not cover the cable hit.
            let anchor = pointer.unwrap_or(canvas_rect.center()) + egui::vec2(18.0, -36.0);
            egui::Area::new(egui::Id::new("cable_delete_hint"))
                .fixed_pos(anchor)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    let resp = ui.add(
                        egui::Button::new("删除连线")
                            .fill(egui::Color32::from_rgb(180, 60, 50)),
                    );
                    area_hovered = resp.hovered() || resp.contains_pointer();
                    area_clicked = resp.clicked();
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
            PointerDbg {
                frame,
                interact_pos,
                hover_pos,
                area_shown,
                area_hovered,
                area_clicked,
            },
        );

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
        dbg: PointerDbg,
    ) {
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_released = ui.input(|i| i.pointer.primary_released());
        let pointer_delta = ui.input(|i| i.pointer.delta());
        let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let delete_key = ui.input(|i| {
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
        });
        let decidedly = ui.input(|i| i.pointer.is_decidedly_dragging());
        let dragged_id = ui.ctx().dragged_id().map(|id| format!("{id:?}"));

        let interacting = primary_down
            || primary_pressed
            || primary_released
            || delete_key
            || secondary_clicked
            || self.drag.is_some()
            || dbg.area_clicked;
        // #region agent log
        if interacting {
            let pjson = pointer
                .map(|p| format!("[{:.1},{:.1}]", p.x, p.y))
                .unwrap_or_else(|| "null".into());
            let ij = dbg
                .interact_pos
                .map(|p| format!("[{:.1},{:.1}]", p.x, p.y))
                .unwrap_or_else(|| "null".into());
            let hj = dbg
                .hover_pos
                .map(|p| format!("[{:.1},{:.1}]", p.x, p.y))
                .unwrap_or_else(|| "null".into());
            waver_dbg(
                "C",
                "editor/mod.rs:handle_pointer",
                "pointer_frame",
                &format!(
                    "{{\"frame\":{},\"pointer\":{pjson},\"interact_pos\":{ij},\"hover_pos\":{hj},\"primary_pressed\":{primary_pressed},\"primary_down\":{primary_down},\"primary_released\":{primary_released},\"delta\":[{:.1},{:.1}],\"resp_dragged\":{},\"resp_contains\":{},\"resp_hovered\":{},\"decidedly_dragging\":{decidedly},\"drag\":\"{}\",\"hovered_cable\":{},\"area_shown\":{},\"area_hovered\":{},\"area_clicked\":{},\"dragged_id\":{},\"canvas\":[{:.1},{:.1},{:.1},{:.1}]}}",
                    dbg.frame,
                    pointer_delta.x, pointer_delta.y,
                    response.dragged(),
                    response.contains_pointer(),
                    response.hovered(),
                    drag_tag(&self.drag).replace('"', "'"),
                    hovered_cable.map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
                    dbg.area_shown,
                    dbg.area_hovered,
                    dbg.area_clicked,
                    dragged_id
                        .map(|s| format!("\"{}\"", s.replace('"', "'")))
                        .unwrap_or_else(|| "null".into()),
                    canvas_rect.min.x, canvas_rect.min.y, canvas_rect.max.x, canvas_rect.max.y
                ),
            );
        }
        // #endregion

        let Some(pointer) = pointer else {
            // #region agent log
            if primary_pressed || primary_down {
                waver_dbg(
                    "C",
                    "editor/mod.rs:handle_pointer",
                    "pointer_none_early_return",
                    &format!(
                        "{{\"frame\":{},\"primary_pressed\":{primary_pressed},\"primary_down\":{primary_down}}}",
                        dbg.frame
                    ),
                );
            }
            // #endregion
            return;
        };

        // Double-click near a cable also deletes it.
        if response.double_clicked() {
            // #region agent log
            waver_dbg(
                "F",
                "editor/mod.rs:handle_pointer",
                "double_click_delete",
                &format!(
                    "{{\"hovered_cable\":{}}}",
                    hovered_cable.map(|i| i.to_string()).unwrap_or_else(|| "null".into())
                ),
            );
            // #endregion
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
            // #region agent log
            waver_dbg(
                "F",
                "editor/mod.rs:handle_pointer",
                "delete_key_or_rmb",
                &format!(
                    "{{\"delete_key\":{delete_key},\"secondary_clicked\":{secondary_clicked},\"hovered_cable\":{},\"runId\":\"post-fix\"}}",
                    hovered_cable.map(|i| i.to_string()).unwrap_or_else(|| "null".into())
                ),
            );
            // #endregion
            if let Some(idx) = hovered_cable {
                if patch.disconnect_edge(idx) {
                    self.hovered_cable = None;
                    patch.recompile(commands);
                }
            }
            self.drag = None;
            return;
        }
        if delete_key && hovered_cable.is_none() {
            // #region agent log
            waver_dbg(
                "F",
                "editor/mod.rs:handle_pointer",
                "delete_key_no_hovered_cable",
                &format!("{{\"edge_len\":{}}}", patch.graph.edges().len()),
            );
            // #endregion
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
                // #region agent log
                waver_dbg(
                    "D",
                    "editor/mod.rs:handle_pointer",
                    "press_jack",
                    &format!(
                        "{{\"pointer\":[{:.1},{:.1}],\"jack\":[{:.1},{:.1}],\"is_output\":{}}}",
                        pointer.x, pointer.y, jack.center.x, jack.center.y, jack.is_output
                    ),
                );
                // #endregion
                self.on_jack_click(patch, commands, jack);
                self.drag = None;
                return;
            }
            if let Some((id, hit, rect)) = top_hit(patch, &self.node_rects, pointer) {
                patch.selected = Some(id);
                let hit_s = match hit {
                    NodeHit::Header => "Header".into(),
                    NodeHit::Body => "Body".into(),
                    NodeHit::Knob { param } => format!("Knob({param})"),
                    NodeHit::Wave { index } => format!("Wave({index})"),
                };
                // #region agent log
                waver_dbg(
                    "D",
                    "editor/mod.rs:handle_pointer",
                    "press_node_hit",
                    &format!(
                        "{{\"id\":{},\"hit\":\"{hit_s}\",\"pointer\":[{:.1},{:.1}],\"rect\":[{:.1},{:.1},{:.1},{:.1}],\"area_shown\":{},\"runId\":\"post-fix\"}}",
                        id.raw(),
                        pointer.x, pointer.y,
                        rect.min.x, rect.min.y, rect.max.x, rect.max.y,
                        dbg.area_shown
                    ),
                );
                // #endregion
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
            // #region agent log
            let rects: String = self
                .node_rects
                .iter()
                .map(|(id, r)| {
                    format!(
                        "{{\"id\":{},\"r\":[{:.1},{:.1},{:.1},{:.1}]}}",
                        id.raw(),
                        r.min.x, r.min.y, r.max.x, r.max.y
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            waver_dbg(
                "D",
                "editor/mod.rs:handle_pointer",
                "press_empty_canvas",
                &format!(
                    "{{\"pointer\":[{:.1},{:.1}],\"area_shown\":{},\"node_rects\":[{rects}],\"runId\":\"post-fix\"}}",
                    pointer.x, pointer.y, dbg.area_shown
                ),
            );
            // #endregion
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
                    // #region agent log
                    waver_dbg(
                        "A",
                        "editor/mod.rs:handle_pointer",
                        "drag_node_update",
                        &format!(
                            "{{\"id\":{},\"pointer\":[{:.1},{:.1}],\"grab\":[{:.1},{:.1}],\"next\":[{:.1},{:.1}],\"clamped\":[{:.1},{:.1}],\"would_reset\":{}}}",
                            id.raw(),
                            pointer.x, pointer.y,
                            grab_offset.x, grab_offset.y,
                            next.x, next.y,
                            clamped.x, clamped.y,
                            clamped.x < canvas_rect.left() || clamped.y < canvas_rect.top()
                        ),
                    );
                    // #endregion
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
                    let mut had_cell = false;
                    if let Some(compiled) = &patch.compiled {
                        if let Some(cell) = compiled.params.get(node, ParamId::new(param)) {
                            had_cell = true;
                            let dy = last_pointer.y - pointer.y;
                            let before = cell.value();
                            let mut v = before;
                            if param == 0 {
                                let log_v = v.max(20.0).ln();
                                v = (log_v + dy * 0.012).exp().clamp(20.0, 2000.0);
                            } else if param == 1 {
                                v = (v + dy * 0.006).clamp(0.0, 1.0);
                            }
                            cell.set(v);
                            // #region agent log
                            waver_dbg(
                                "D",
                                "editor/mod.rs:handle_pointer",
                                "drag_knob_update",
                                &format!(
                                    "{{\"node\":{},\"param\":{param},\"dy\":{dy:.2},\"before\":{before:.4},\"after\":{v:.4},\"had_cell\":true,\"pointer\":[{:.1},{:.1}],\"runId\":\"post-fix\"}}",
                                    node.raw(),
                                    pointer.x, pointer.y
                                ),
                            );
                            // #endregion
                        }
                    }
                    if !had_cell {
                        // #region agent log
                        waver_dbg(
                            "D",
                            "editor/mod.rs:handle_pointer",
                            "drag_knob_no_cell",
                            &format!(
                                "{{\"node\":{},\"param\":{param},\"compiled\":{}}}",
                                node.raw(),
                                patch.compiled.is_some()
                            ),
                        );
                        // #endregion
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

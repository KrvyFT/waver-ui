//! Patch graph state, layout, and compile → audio queue.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

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
// #endregion

use eframe::egui;
use rtrb::Producer;
use waver_core::{
    CompiledPatch, Graph, GraphError, NodeId, NodeKind, PortRef, RtCommand,
};

/// UI-owned patch editing state.
pub struct PatchState {
    pub graph: Graph,
    pub positions: HashMap<NodeId, egui::Pos2>,
    pub compiled: Option<Arc<CompiledPatch>>,
    pub compile_error: Option<GraphError>,
    pub selected: Option<NodeId>,
}

impl PatchState {
    /// Default VCO → Output patch with positions.
    pub fn default_patch() -> Self {
        let mut state = Self {
            graph: Graph::new(),
            positions: HashMap::new(),
            compiled: None,
            compile_error: None,
            selected: None,
        };
        let vco = state.add_node_at(NodeKind::Vco, egui::pos2(40.0, 160.0));
        let out = state.add_node_at(NodeKind::Output, egui::pos2(360.0, 180.0));
        state.try_connect(
            PortRef {
                node: vco,
                port: waver_core::PortId::new(0),
            },
            PortRef {
                node: out,
                port: waver_core::PortId::new(0),
            },
        );
        state
    }

    pub fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let y = 80.0 + self.graph.nodes().len() as f32 * 60.0;
        self.add_node_at(kind, egui::pos2(120.0, y))
    }

    pub fn add_node_at(&mut self, kind: NodeKind, pos: egui::Pos2) -> NodeId {
        let id = self.graph.insert(kind);
        self.positions.insert(id, pos);
        id
    }

    pub fn remove_selected(&mut self) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        if !self.graph.remove(id) {
            return false;
        }
        self.positions.remove(&id);
        if self.selected == Some(id) {
            self.selected = None;
        }
        true
    }

    pub fn try_connect(&mut self, from: PortRef, to: PortRef) -> bool {
        let from_node = self.graph.node(from.node);
        let to_node = self.graph.node(to.node);
        if from_node.is_none() || to_node.is_none() {
            return false;
        }
        let from_kind = from_node.unwrap().kind;
        let to_kind = to_node.unwrap().kind;
        if !from_kind.is_output_port(from.port) || !to_kind.is_input_port(to.port) {
            return false;
        }
        if from.node == to.node {
            return false;
        }
        self.graph.connect(from, to);
        true
    }

    pub fn disconnect_edge(&mut self, index: usize) -> bool {
        let before = self.graph.edges().len();
        let ok = self.graph.disconnect_edge(index);
        // #region agent log
        waver_dbg(
            "F",
            "patch_state.rs:disconnect_edge",
            "disconnect_edge",
            &format!(
                "{{\"index\":{index},\"ok\":{ok},\"edges_before\":{before},\"edges_after\":{}}}",
                self.graph.edges().len()
            ),
        );
        // #endregion
        ok
    }

    pub fn position(&self, id: NodeId) -> egui::Pos2 {
        self.positions
            .get(&id)
            .copied()
            .unwrap_or(egui::pos2(0.0, 0.0))
    }

    pub fn set_position(&mut self, id: NodeId, pos: egui::Pos2) {
        // #region agent log
        let prev = self.positions.get(&id).copied();
        let changed = prev.is_none_or(|p| (p.x - pos.x).abs() > 0.05 || (p.y - pos.y).abs() > 0.05);
        if changed {
            waver_dbg(
                "A",
                "patch_state.rs:set_position",
                "set_position",
                &format!(
                    "{{\"id\":{},\"prev\":[{:.1},{:.1}],\"next\":[{:.1},{:.1}]}}",
                    id.raw(),
                    prev.map(|p| p.x).unwrap_or(f32::NAN),
                    prev.map(|p| p.y).unwrap_or(f32::NAN),
                    pos.x,
                    pos.y
                ),
            );
        }
        // #endregion
        self.positions.insert(id, pos);
    }

    /// Recompile and push to the audio thread when successful.
    pub fn recompile(&mut self, commands: &mut Producer<RtCommand>) {
        let existing = self.compiled.as_ref().map(|p| p.params.clone());
        match self.graph.compile_patch(existing.as_ref()) {
            Ok(patch) => {
                self.compile_error = None;
                let shared = Arc::new(patch);
                self.compiled = Some(Arc::clone(&shared));
                let _ = commands.push(RtCommand::SwapSchedule(shared));
            }
            Err(err) => {
                self.compile_error = Some(err);
            }
        }
    }
}

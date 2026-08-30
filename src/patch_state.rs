//! Patch graph state, layout, and compile → audio queue.

use std::collections::HashMap;
use std::sync::Arc;

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
        self.graph.disconnect_edge(index)
    }

    pub fn position(&self, id: NodeId) -> egui::Pos2 {
        self.positions
            .get(&id)
            .copied()
            .unwrap_or(egui::pos2(0.0, 0.0))
    }

    pub fn set_position(&mut self, id: NodeId, pos: egui::Pos2) {
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

//! Cable drag state machine.

use eframe::egui;
use waver_core::PortRef;

/// Jack screen position for routing visuals.
#[derive(Clone, Copy, Debug)]
pub struct JackPos {
    pub port: PortRef,
    pub center: egui::Pos2,
    pub is_output: bool,
}

/// Cable interaction state.
#[derive(Clone, Debug)]
pub enum CableState {
    Idle,
    Dragging {
        from: PortRef,
        from_pos: egui::Pos2,
    },
}

impl CableState {
    pub fn cancel(&mut self) {
        *self = Self::Idle;
    }

    pub fn start_drag(&mut self, from: PortRef, from_pos: egui::Pos2) {
        *self = Self::Dragging { from, from_pos };
    }
}

/// Draw a quadratic bezier cable between two jack centers.
pub fn draw_cable(
    painter: &egui::Painter,
    from: egui::Pos2,
    to: egui::Pos2,
    color: egui::Color32,
    width: f32,
    dashed: bool,
) {
    let ctrl_offset = ((to.x - from.x).abs() * 0.5).max(40.0);
    let c1 = egui::pos2(from.x + ctrl_offset, from.y);
    let c2 = egui::pos2(to.x - ctrl_offset, to.y);

    if dashed {
        let steps = 24;
        for i in (0..steps).step_by(2) {
            let t0 = i as f32 / steps as f32;
            let t1 = (i + 1) as f32 / steps as f32;
            painter.line_segment(
                [bezier_point(from, c1, c2, to, t0), bezier_point(from, c1, c2, to, t1)],
                egui::Stroke::new(width, color),
            );
        }
    } else {
        let points: Vec<egui::Pos2> = (0..=32)
            .map(|i| bezier_point(from, c1, c2, to, i as f32 / 32.0))
            .collect();
        for window in points.windows(2) {
            painter.line_segment(
                [window[0], window[1]],
                egui::Stroke::new(width, color),
            );
        }
    }
}

fn bezier_point(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, p3: egui::Pos2, t: f32) -> egui::Pos2 {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;
    egui::pos2(
        uuu * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + ttt * p3.x,
        uuu * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + ttt * p3.y,
    )
}

/// Hit-test radius around a jack center.
pub const JACK_HIT_RADIUS: f32 = 10.0;

pub fn jack_at(pointer: egui::Pos2, jacks: &[JackPos]) -> Option<JackPos> {
    jacks.iter().copied().find(|jack| {
        jack.center.distance(pointer) <= JACK_HIT_RADIUS
    })
}

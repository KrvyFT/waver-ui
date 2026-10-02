//! Painter-only module cards and their matching hit regions.

use super::cable::JackPos;
use crate::theme;
use eframe::egui;
use waver_core::{Node, NodeKind, ParamId, ParamRegistry, PortId, PortRef, ScopeTap};

pub const VCO_SIZE: egui::Vec2 = egui::vec2(188.0, 210.0);
pub const SCOPE_SIZE: egui::Vec2 = egui::vec2(200.0, 158.0);
pub const NODE_HEADER_H: f32 = 30.0;
pub const KNOB_HIT_RADIUS: f32 = 26.0;

/// Samples drawn per frame (~6.7 ms at 48 kHz, about three 440 Hz periods).
const SCOPE_TRACE_SAMPLES: usize = 320;
/// Snapshot window: the drawn trace plus the history searched for a trigger point.
const SCOPE_SNAPSHOT_SAMPLES: usize = SCOPE_TRACE_SAMPLES * 2;
/// Input jack row of a monitoring card, below the plot.
const SCOPE_JACK_Y: f32 = 138.0;

pub fn node_size(kind: NodeKind) -> egui::Vec2 {
    if kind == NodeKind::Vco {
        VCO_SIZE
    } else if kind == NodeKind::Scope {
        SCOPE_SIZE
    } else {
        let ports = kind.port_counts();
        let rows = ports.inputs.max(ports.outputs).max(1);
        egui::vec2(
            if ports.inputs > 0 && ports.outputs > 0 {
                180.0
            } else {
                156.0
            },
            112.0 + rows.saturating_sub(1) as f32 * 32.0,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NodeHit {
    Header,
    Body,
    Knob { param: u32 },
    Wave { index: u32 },
}

pub fn hit_test_node(kind: NodeKind, rect: egui::Rect, pointer: egui::Pos2) -> Option<NodeHit> {
    if !rect.contains(pointer) {
        return None;
    }
    if kind == NodeKind::Vco {
        for param in 0..2 {
            if pointer.distance(knob_center(rect, param)) <= KNOB_HIT_RADIUS {
                return Some(NodeHit::Knob { param });
            }
        }
        for index in 0..4 {
            if wave_button_rect(rect, index).contains(pointer) {
                return Some(NodeHit::Wave { index });
            }
        }
    }
    if pointer.y < rect.top() + NODE_HEADER_H {
        Some(NodeHit::Header)
    } else {
        Some(NodeHit::Body)
    }
}

pub fn draw_node_visual(
    painter: &egui::Painter,
    node: &Node,
    rect: egui::Rect,
    selected: bool,
    params: Option<&ParamRegistry>,
) -> Vec<JackPos> {
    painter.rect_filled(rect, 2.0, theme::SURFACE);
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), NODE_HEADER_H)),
        egui::CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        if selected {
            theme::ACCENT
        } else {
            theme::PANEL
        },
    );
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::ACCENT
            } else {
                theme::BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            rect.min + egui::vec2(12.0, NODE_HEADER_H),
            egui::pos2(rect.right() - 12.0, rect.top() + NODE_HEADER_H),
        ],
        egui::Stroke::new(
            2.0,
            if node.kind == NodeKind::Output {
                theme::INPUT
            } else {
                theme::OUTPUT
            },
        ),
    );
    text(
        painter,
        rect.min + egui::vec2(12.0, 15.0),
        kind_label(node.kind),
        14.0,
        theme::TEXT,
    );

    if node.kind == NodeKind::Vco {
        let value = |id, fallback| {
            params
                .and_then(|p| p.get(node.id, ParamId::new(id)))
                .map(|cell| cell.value())
                .unwrap_or(fallback)
        };
        let freq = value(0, 440.0);
        let amp = value(1, 0.5);
        draw_knob(
            painter,
            knob_center(rect, 0),
            "频率",
            &format!("{freq:.0} Hz"),
            ((freq.max(20.0).ln() - 20.0f32.ln()) / (2000.0f32.ln() - 20.0f32.ln()))
                .clamp(0.0, 1.0),
        );
        draw_knob(
            painter,
            knob_center(rect, 1),
            "振幅",
            &format!("{:.0}%", amp * 100.0),
            amp,
        );
        text(
            painter,
            rect.min + egui::vec2(12.0, 163.0),
            "波形",
            12.0,
            theme::MUTED,
        );
        let wave = value(2, 0.0).round() as u32;
        for index in 0..4 {
            let button = wave_button_rect(rect, index);
            painter.rect_filled(
                button,
                2.0,
                if index == wave {
                    theme::ACCENT
                } else {
                    theme::PANEL
                },
            );
            painter.text(
                button.center(),
                egui::Align2::CENTER_CENTER,
                ["正弦", "锯齿", "方波", "三角"][index as usize],
                egui::FontId::proportional(12.0),
                if index == wave {
                    theme::TEXT
                } else {
                    theme::MUTED
                },
            );
        }
    } else if node.kind == NodeKind::Scope {
        draw_scope(
            painter,
            rect,
            params.and_then(|p| p.tap(node.id)).as_deref(),
        );
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + SCOPE_JACK_Y),
            egui::Align2::RIGHT_CENTER,
            node.kind.desc().summary,
            egui::FontId::proportional(11.0),
            theme::MUTED,
        );
    } else {
        text(
            painter,
            egui::pos2(rect.left() + 12.0, rect.bottom() - 24.0),
            node.kind.desc().summary,
            12.0,
            theme::MUTED,
        );
    }

    let mut jacks = Vec::new();
    let counts = node.kind.port_counts();
    for (is_output, count) in [(false, counts.inputs), (true, counts.outputs)] {
        for index in 0..count {
            let center = egui::pos2(
                if is_output {
                    rect.right() - 8.0
                } else {
                    rect.left() + 8.0
                },
                rect.top() + node_port_top(node.kind) + index as f32 * 32.0,
            );
            painter.circle_filled(
                center,
                8.0,
                if is_output {
                    theme::OUTPUT
                } else {
                    theme::INPUT
                },
            );
            painter.text(
                center + egui::vec2(if is_output { -16.0 } else { 16.0 }, 0.0),
                if is_output {
                    egui::Align2::RIGHT_CENTER
                } else {
                    egui::Align2::LEFT_CENTER
                },
                if is_output { "OUT 输出" } else { "IN 输入" },
                egui::FontId::proportional(12.0),
                theme::MUTED,
            );
            jacks.push(JackPos {
                port: PortRef {
                    node: node.id,
                    port: PortId::new(index),
                },
                center,
                is_output,
            });
        }
    }
    jacks
}

fn node_port_top(kind: NodeKind) -> f32 {
    match kind {
        NodeKind::Vco => 144.0,
        NodeKind::Scope => SCOPE_JACK_Y,
        _ => 56.0,
    }
}

/// Plot area of a monitoring card: below the header, above the jack row.
fn scope_plot_rect(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        rect.min + egui::vec2(10.0, 38.0),
        egui::pos2(rect.right() - 10.0, rect.top() + 122.0),
    )
}

/// Draw the monitor tap of a scope card; an unbound tap leaves an empty grid.
fn draw_scope(painter: &egui::Painter, rect: egui::Rect, tap: Option<&ScopeTap>) {
    let plot = scope_plot_rect(rect);
    painter.rect_filled(plot, 2.0, theme::BACKGROUND);
    painter.rect_stroke(
        plot,
        2.0,
        egui::Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let mid = plot.center().y;
    painter.line_segment(
        [egui::pos2(plot.left(), mid), egui::pos2(plot.right(), mid)],
        egui::Stroke::new(1.0, theme::GRID),
    );
    let Some(tap) = tap else {
        return;
    };
    let mut buffer = [0.0f32; SCOPE_SNAPSHOT_SAMPLES];
    let count = tap.snapshot(&mut buffer);
    let trace = scope_trace(&buffer[..count]);
    if trace.len() < 2 {
        return;
    }
    let last = (trace.len() - 1) as f32;
    let points: Vec<egui::Pos2> = trace
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            egui::pos2(
                plot.left() + plot.width() * index as f32 / last,
                mid - plot.height() * 0.5 * sample.clamp(-1.0, 1.0),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, theme::GOOD),
    ));
}

/// Pick the newest window, aligned to the last rising zero crossing so it stands still.
fn scope_trace(samples: &[f32]) -> &[f32] {
    if samples.len() <= SCOPE_TRACE_SAMPLES {
        return samples;
    }
    let latest = samples.len() - SCOPE_TRACE_SAMPLES;
    let earliest = latest.saturating_sub(SCOPE_TRACE_SAMPLES);
    for index in (earliest..latest).rev() {
        if samples[index] <= 0.0 && samples[index + 1] > 0.0 {
            return &samples[index..index + SCOPE_TRACE_SAMPLES];
        }
    }
    &samples[latest..]
}

fn text(painter: &egui::Painter, pos: egui::Pos2, label: &str, size: f32, color: egui::Color32) {
    painter.text(
        pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(size),
        color,
    );
}

fn knob_center(rect: egui::Rect, index: u32) -> egui::Pos2 {
    rect.min + egui::vec2(50.0 + index as f32 * 88.0, 88.0)
}

fn wave_button_rect(rect: egui::Rect, index: u32) -> egui::Rect {
    egui::Rect::from_min_size(
        rect.min + egui::vec2(12.0 + index as f32 * 42.0, 176.0),
        egui::vec2(38.0, 26.0),
    )
}

fn draw_knob(painter: &egui::Painter, center: egui::Pos2, label: &str, value: &str, t: f32) {
    painter.circle_filled(center, 24.0, theme::CANVAS);
    let arc: Vec<_> = (0..=32)
        .map(|i| {
            let a = -2.4 + 4.8 * t.clamp(0.0, 1.0) * i as f32 / 32.0;
            center + egui::vec2(a.sin(), -a.cos()) * 22.0
        })
        .collect();
    painter.add(egui::Shape::line(
        arc,
        egui::Stroke::new(3.0, theme::ACCENT),
    ));
    painter.circle_stroke(center, 24.0, egui::Stroke::new(1.0, theme::BORDER));
    let angle = egui::remap(t.clamp(0.0, 1.0), 0.0..=1.0, -2.4..=2.4);
    painter.line_segment(
        [
            center,
            center + egui::vec2(angle.sin(), -angle.cos()) * 19.0,
        ],
        egui::Stroke::new(4.0, theme::ACCENT),
    );
    for (offset, label, size, color) in [
        (-34.0, label, 12.0, theme::MUTED),
        (34.0, value, 14.0, theme::TEXT),
    ] {
        painter.text(
            center + egui::vec2(0.0, offset),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(size),
            color,
        );
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    kind.desc().canvas_label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_port_nodes_keep_summary_below_all_jacks() {
        let ctx = egui::Context::default();
        crate::setup_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for kind in [
                NodeKind::Vcf,
                NodeKind::Vca,
                NodeKind::Mixer,
                NodeKind::Delay,
            ] {
                let rect = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), node_size(kind));
                let node = Node {
                    id: waver_core::NodeId::new(0),
                    kind,
                };
                let jacks = draw_node_visual(ui.painter(), &node, rect, false, None);
                let summary_top = rect.bottom() - 24.0 - 6.0;
                for jack in jacks {
                    assert!(rect.contains(jack.center));
                    assert!(
                        jack.center.y + 8.0 < summary_top,
                        "{kind:?}: jack overlaps summary"
                    );
                }
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn amp_knob_hit_not_body_or_header() {
        let rect = egui::Rect::from_min_size(egui::pos2(161.3, 224.7), VCO_SIZE);
        let amp = knob_center(rect, 1);
        match hit_test_node(NodeKind::Vco, rect, amp) {
            Some(NodeHit::Knob { param: 1 }) => {}
            other => panic!("expected Amp knob, got {other:?}"),
        }
        // Slightly above amp (toward header) should still prefer knob over header.
        let near_top = egui::pos2(amp.x, amp.y - 20.0);
        match hit_test_node(NodeKind::Vco, rect, near_top) {
            Some(NodeHit::Knob { param: 1 }) => {}
            other => panic!("expected Amp knob near top edge, got {other:?}"),
        }
    }

    #[test]
    fn header_hit_only_on_title_bar() {
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 160.0), VCO_SIZE);
        let header_pt = egui::pos2(rect.center().x, rect.top() + 10.0);
        assert!(matches!(
            hit_test_node(NodeKind::Vco, rect, header_pt),
            Some(NodeHit::Header)
        ));
        let body_pt = egui::pos2(rect.right() - 30.0, rect.top() + 50.0);
        assert!(matches!(
            hit_test_node(NodeKind::Vco, rect, body_pt),
            Some(NodeHit::Body)
        ));
    }

    #[test]
    fn scope_trace_aligns_to_a_rising_crossing() {
        let mut samples = vec![-0.5f32; SCOPE_SNAPSHOT_SAMPLES];
        samples[100] = -0.1;
        samples[101] = 0.1;
        let trace = scope_trace(&samples);
        assert_eq!(trace.len(), SCOPE_TRACE_SAMPLES);
        assert_eq!(trace[0], -0.1);

        // A falling-only window has no trigger edge, so the newest samples win.
        let falling: Vec<f32> = (0..SCOPE_SNAPSHOT_SAMPLES)
            .map(|index| -(index as f32) - 1.0)
            .collect();
        let trace = scope_trace(&falling);
        assert_eq!(trace.len(), SCOPE_TRACE_SAMPLES);
        assert_eq!(trace[0], -(SCOPE_TRACE_SAMPLES as f32) - 1.0);
    }

    #[test]
    fn scope_jack_stays_below_the_trace_plot() {
        let ctx = egui::Context::default();
        crate::setup_fonts(&ctx);
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 90.0), SCOPE_SIZE);
        let node = Node {
            id: waver_core::NodeId::new(0),
            kind: NodeKind::Scope,
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let jacks = draw_node_visual(ui.painter(), &node, rect, false, None);
            assert_eq!(jacks.len(), 1);
            let jack = jacks[0];
            assert!(!jack.is_output, "a monitor card only takes input");
            assert!(rect.contains(jack.center));
            assert!(
                jack.center.y - 8.0 >= scope_plot_rect(rect).bottom(),
                "the input jack must clear the plot: {jack:?}"
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn scope_card_draws_the_captured_waveform() {
        let ctx = egui::Context::default();
        crate::setup_fonts(&ctx);
        let mut graph = waver_core::Graph::new();
        let scope = graph.insert(NodeKind::Scope);
        let patch = graph.compile_patch(None).expect("compile");
        let tap = patch.params.tap(scope).expect("scope tap");
        for step in 0..SCOPE_SNAPSHOT_SAMPLES {
            tap.push((step as f32 * 0.2).sin());
        }
        let rect = egui::Rect::from_min_size(egui::pos2(30.0, 30.0), SCOPE_SIZE);
        let node = Node {
            id: scope,
            kind: NodeKind::Scope,
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_node_visual(ui.painter(), &node, rect, false, Some(&patch.params));
        });
        fn trace_points(shape: &egui::Shape) -> Option<usize> {
            match shape {
                egui::Shape::Path(path) => match path.stroke.color {
                    egui::epaint::ColorMode::Solid(color) if color == theme::GOOD => {
                        Some(path.points.len())
                    }
                    _ => None,
                },
                egui::Shape::Vec(shapes) => shapes.iter().find_map(trace_points),
                _ => None,
            }
        }
        let drawn = output
            .shapes
            .iter()
            .find_map(|clipped| trace_points(&clipped.shape))
            .expect("expected a scope trace");
        assert_eq!(drawn, SCOPE_TRACE_SAMPLES);
        output.textures_delta.clear();
    }
}

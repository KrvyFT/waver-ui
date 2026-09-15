//! Three-column patch workspace.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use rtrb::Producer;
use waver_core::{EngineStatus, NodeKind, RtCommand};

use crate::editor::PatchEditor;
use crate::patch_state::PatchState;
use crate::theme;

/// GUI state: command producer plus a read-only engine snapshot.
pub struct WaverApp {
    commands: Producer<RtCommand>,
    status: Arc<EngineStatus>,
    device_name: String,
    error: Option<String>,
    patch: PatchState,
    editor: PatchEditor,
    module_search: String,
    bootstrapped: bool,
}

impl WaverApp {
    pub fn new(
        commands: Producer<RtCommand>,
        status: Arc<EngineStatus>,
        device_name: String,
        error: Option<String>,
    ) -> Self {
        Self {
            commands,
            status,
            device_name,
            error,
            patch: PatchState::default_patch(),
            editor: PatchEditor::default(),
            module_search: String::new(),
            bootstrapped: false,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        if !self.bootstrapped {
            self.patch.recompile(&mut self.commands);
            self.bootstrapped = true;
        }

        // Panels share the App::ui root; nesting a CentralPanel steals canvas hits.
        egui::Panel::top("transport")
            .exact_size(64.0)
            .resizable(false)
            .frame(theme::panel_frame(12))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .exact_size(40.0)
            .resizable(false)
            .frame(theme::panel_frame(8))
            .show(ui, |ui| self.status_bar(ui));
        let compact = ui.available_width() < 1200.0;
        egui::Panel::left("module_library")
            .exact_size(if compact { 184.0 } else { 224.0 })
            .resizable(false)
            .frame(theme::panel_frame(16))
            .show(ui, |ui| self.module_library(ui));
        egui::Panel::right("inspector")
            .exact_size(if compact { 244.0 } else { 280.0 })
            .resizable(false)
            .frame(theme::panel_frame(24))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.editor
                        .inspector(ui, &mut self.patch, &mut self.commands);
                    ui.add_space(32.0);
                    self.device_details(ui);
                });
            });
        self.editor.ui(ui, &mut self.patch, &mut self.commands);
    }

    fn top_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("waver")
                    .size(24.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(16.0);
            ui.label(egui::RichText::new("未命名 Patch").strong());
            ui.add_space(12.0);
            let ready = self.status.running() && self.error.is_none();
            egui::Frame::new()
                .fill(if ready {
                    egui::Color32::from_rgb(24, 55, 42)
                } else {
                    theme::SURFACE
                })
                .corner_radius(14)
                .inner_margin(egui::Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(if ready {
                            "● 音频就绪"
                        } else {
                            "● 音频未就绪"
                        })
                        .size(11.0)
                        .color(if ready {
                            theme::GOOD
                        } else {
                            theme::MUTED
                        }),
                    );
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, 40.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        theme::caption(
                            ui,
                            &format!(
                                "{} Hz · {} frames",
                                self.status.sample_rate(),
                                self.status.block()
                            ),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 声道 · {} 次中断",
                                self.status.channels(),
                                self.status.xruns()
                            ))
                            .size(10.0)
                            .color(theme::GOOD),
                        );
                    },
                );
                // The engine has no transport or master-mute command yet. Keep the
                // reference controls explicitly disabled instead of reporting success.
                for (label, width, fill, color) in [
                    (
                        "紧急静音",
                        104.0,
                        egui::Color32::from_rgb(58, 32, 35),
                        theme::DANGER,
                    ),
                    ("静音", 80.0, theme::SURFACE, theme::TEXT),
                    ("播放", 88.0, theme::ACCENT, theme::BACKGROUND),
                ] {
                    ui.add_enabled(
                        false,
                        egui::Button::new(egui::RichText::new(label).color(color))
                            .fill(fill)
                            .min_size(egui::vec2(width, 40.0)),
                    )
                    .on_disabled_hover_text("计划中：当前引擎尚未提供播放 / 主静音控制");
                }
            });
        });
    }

    fn module_library(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.heading("模块");
        ui.add_space(4.0);
        ui.add_sized(
            [ui.available_width(), 36.0],
            egui::TextEdit::singleline(&mut self.module_search)
                .hint_text("搜索模块")
                .margin(egui::vec2(12.0, 8.0)),
        );
        ui.add_space(20.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let query = self.module_search.trim().to_lowercase();
            let mut matches = 0;
            for (section, modules) in [
                (
                    "核心",
                    &[
                        ("振荡器", "VCO", Some(NodeKind::Vco)),
                        ("输出", "Output", Some(NodeKind::Output)),
                    ][..],
                ),
                (
                    "工具",
                    &[
                        ("块延迟", "Delay", Some(NodeKind::Delay)),
                        ("静音源", "Silence", Some(NodeKind::Silence)),
                    ][..],
                ),
                (
                    "计划中",
                    &[
                        ("滤波器", "VCF", None),
                        ("放大器", "VCA", None),
                        ("包络", "ADSR", None),
                        ("LFO", "LFO", None),
                        ("混音器", "Mixer", None),
                    ][..],
                ),
            ] {
                let visible: Vec<_> = modules
                    .iter()
                    .filter(|(name, code, _)| {
                        query.is_empty()
                            || name.contains(&query)
                            || code.to_lowercase().contains(&query)
                    })
                    .collect();
                if visible.is_empty() {
                    continue;
                }
                theme::caption(ui, section);
                ui.add_space(4.0);
                for (name, code, kind) in visible {
                    matches += 1;
                    let response = ui.add_enabled(
                        kind.is_some(),
                        egui::Button::new(format!("{name}  ·  {code}"))
                            .min_size(egui::vec2(ui.available_width(), 46.0))
                            .stroke(egui::Stroke::NONE),
                    );
                    if let Some(kind) = kind {
                        if response.on_hover_text("点击添加到画布").clicked() {
                            let id = self.patch.add_node(*kind);
                            self.patch.selected = Some(id);
                            self.patch.recompile(&mut self.commands);
                        }
                    } else {
                        response.on_disabled_hover_text("计划中的模块，暂不可添加");
                    }
                }
                ui.add_space(16.0);
            }
            if matches == 0 {
                theme::caption(ui, "没有匹配的模块");
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(if self.status.running() {
                    "● 引擎运行中"
                } else {
                    "● 引擎已停止"
                })
                .size(11.0)
                .color(if self.status.running() {
                    theme::GOOD
                } else {
                    theme::MUTED
                }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                theme::caption(ui, "自动保存关闭");
                theme::caption(
                    ui,
                    &format!(
                        "1 个 Patch · {} 个节点 · {} 条连线",
                        self.patch.graph.nodes().len(),
                        self.patch.graph.edges().len()
                    ),
                );
            });
        });
    }

    fn device_details(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.add_space(12.0);
        theme::caption(ui, "输出设备");
        ui.label(if self.device_name.is_empty() {
            "未打开设备"
        } else {
            &self.device_name
        });
        if let Some(error) = &self.error {
            ui.colored_label(theme::DANGER, error);
        }
        ui.add_space(12.0);
        theme::caption(ui, "操作提示");
        theme::caption(ui, "拖动标题移动模块");
        theme::caption(ui, "上下拖动旋钮调节参数");
        theme::caption(ui, "点击 OUT，再点击 IN 连接");
        theme::caption(ui, "悬停连线后按 Delete 删除");
        theme::caption(ui, "Esc 取消当前连线");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waver_core::ParamId;

    fn fixture() -> (egui::Context, WaverApp, rtrb::Consumer<RtCommand>) {
        let ctx = egui::Context::default();
        crate::setup_fonts(&ctx);
        crate::setup_theme(&ctx);
        let (commands, consumer) = rtrb::RingBuffer::new(32);
        let app = WaverApp::new(commands, Arc::new(EngineStatus::new()), String::new(), None);
        (ctx, app, consumer)
    }

    fn draw(
        ctx: &egui::Context,
        app: &mut WaverApp,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| app.ui(ui),
        );
        output.textures_delta.clear();
        output
    }

    fn text_rect(output: &egui::FullOutput, label: &str) -> egui::Rect {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, label))
            .unwrap_or_else(|| panic!("missing label: {label}"))
    }

    fn click(ctx: &egui::Context, app: &mut WaverApp, size: egui::Vec2, pos: egui::Pos2) {
        for pressed in [true, false] {
            draw(
                ctx,
                app,
                size,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn toolbar_and_panels_fit_supported_window_sizes() {
        let (ctx, mut app, _consumer) = fixture();
        for size in [egui::vec2(1440.0, 960.0), egui::vec2(1000.0, 640.0)] {
            draw(&ctx, &mut app, size, vec![]);
            let output = draw(&ctx, &mut app, size, vec![]);
            let brand = text_rect(&output, "waver");
            let patch = text_rect(&output, "未命名 Patch");
            let play = text_rect(&output, "播放");
            let mute = text_rect(&output, "静音");
            let emergency = text_rect(&output, "紧急静音");
            assert!(brand.right() < patch.left());
            assert!(patch.right() < play.left());
            assert!(play.right() < mute.left());
            assert!(mute.right() < emergency.left());
            assert!(emergency.right() < size.x);
            for label in ["属性检查器", "模块", "搜索模块"] {
                assert!(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                        .contains_rect(text_rect(&output, label))
                );
            }
        }
    }

    #[test]
    fn sidebar_controls_preserve_selection_and_edit_graph() {
        let (ctx, mut app, _consumer) = fixture();
        let size = egui::vec2(1440.0, 960.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        let selected = app.patch.selected;
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "属性检查器").center(),
        );
        assert_eq!(app.patch.selected, selected);
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "振荡器  ·  VCO").center(),
        );
        assert_eq!(app.patch.graph.nodes().len(), 3);
        assert_ne!(app.patch.selected, selected);
        let output = draw(&ctx, &mut app, size, vec![]);
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "删除选中模块").center(),
        );
        assert_eq!(app.patch.graph.nodes().len(), 2);
        assert_eq!(app.patch.graph.edges().len(), 1);
    }

    #[test]
    fn module_wave_controls_update_selected_parameter() {
        let (ctx, mut app, _consumer) = fixture();
        let size = egui::vec2(1440.0, 960.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        let selected = app.patch.selected.unwrap();
        click(&ctx, &mut app, size, text_rect(&output, "方波").center());
        let value = app
            .patch
            .compiled
            .as_ref()
            .unwrap()
            .params
            .get(selected, ParamId::new(2))
            .unwrap()
            .value();
        assert_eq!(value, 2.0);
        let output = draw(&ctx, &mut app, size, vec![]);
        // The inspector now displays the same waveform as the module.
        assert!(
            output
                .shapes
                .iter()
                .filter(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "方波"))
                .count()
                >= 2
        );
    }
}

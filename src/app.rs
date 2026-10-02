//! Browser, patch workspace and bottom device panel.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use rtrb::Producer;
use waver_core::{
    AudioCatalog, AudioSelection, AudioSettingsRequest, EngineStatus, MODULE_CATALOG, ModuleFamily,
    RtCommand,
};

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
    audio_settings: crate::audio_settings::AudioSettings,
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
            audio_settings: Default::default(),
        }
    }

    pub fn configure_audio_settings(&mut self, catalog: AudioCatalog, active: AudioSelection) {
        self.audio_settings.catalog = catalog;
        self.audio_settings.active = active.clone();
        self.audio_settings.draft = active;
        self.audio_settings.configured = true;
    }

    pub fn take_audio_request(&mut self) -> Option<AudioSettingsRequest> {
        self.audio_settings.request.take()
    }

    pub fn update_audio_catalog(&mut self, catalog: AudioCatalog) {
        self.audio_settings.catalog = catalog;
    }

    pub fn audio_switch_failed(&mut self, error: String) {
        self.audio_settings.failed = true;
        self.audio_settings.message = Some(format!("切换失败，当前输出保持不变：{error}"));
    }

    pub fn replace_audio(
        &mut self,
        commands: Producer<RtCommand>,
        status: Arc<EngineStatus>,
        device_name: String,
        selection: AudioSelection,
    ) {
        self.commands = commands;
        self.status = status;
        self.device_name = device_name;
        self.error = None;
        self.audio_settings.active = selection.clone();
        self.audio_settings.draft = selection;
        self.audio_settings.failed = false;
        self.audio_settings.message = Some("音频设置已应用".into());
        self.patch.recompile(&mut self.commands);
        self.bootstrapped = true;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        if !self.bootstrapped {
            self.patch.recompile(&mut self.commands);
            self.bootstrapped = true;
        }

        // Panels share the App::ui root; nesting a CentralPanel steals canvas hits.
        egui::Panel::top("transport")
            .exact_size(38.0)
            .resizable(false)
            .frame(theme::panel_frame(6))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .exact_size(26.0)
            .resizable(false)
            .frame(theme::panel_frame(4))
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("device_panel")
            .exact_size(160.0)
            .resizable(false)
            .frame(theme::panel_frame(8))
            .show(ui, |ui| {
                self.editor
                    .inspector(ui, &mut self.patch, &mut self.commands);
            });
        let compact = ui.available_width() < 1200.0;
        if self.audio_settings.open {
            egui::Panel::right("audio_settings")
                .exact_size(288.0)
                .resizable(false)
                .frame(theme::panel_frame(12))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.audio_settings
                            .show(ui, &self.status, &self.device_name);
                        if let Some(error) = &self.error {
                            ui.colored_label(theme::DANGER, error);
                        }
                    });
                });
        }
        egui::Panel::left("module_library")
            .exact_size(if compact { 180.0 } else { 192.0 })
            .resizable(false)
            .frame(theme::panel_frame(8))
            .show(ui, |ui| self.module_library(ui));
        self.editor.ui(ui, &mut self.patch, &mut self.commands);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            for (i, height) in [8.0, 16.0, 22.0, 12.0, 6.0].into_iter().enumerate() {
                let x = rect.left() + 4.0 + i as f32 * 4.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.center().y - height / 2.0),
                        egui::pos2(x, rect.center().y + height / 2.0),
                    ],
                    egui::Stroke::new(3.0, theme::ACCENT),
                );
            }
            ui.label(
                egui::RichText::new("waver")
                    .size(18.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(egui::RichText::new("未命名 Patch").size(14.0).strong());
            theme::caption(ui, "本地工作区");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let ready = self.status.running() && self.error.is_none();
                let label = if ready {
                    "音频已连接"
                } else {
                    "音频未连接"
                };
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(label).size(12.0).color(if ready {
                            theme::GOOD
                        } else {
                            theme::MUTED
                        }))
                        .fill(if self.audio_settings.open {
                            theme::ACTIVE_SURFACE
                        } else {
                            theme::SURFACE
                        }),
                    )
                    .on_hover_text("设置声卡、输出设备和音频后端")
                    .clicked()
                {
                    self.audio_settings.open = !self.audio_settings.open;
                    if self.audio_settings.open {
                        self.audio_settings.draft = self.audio_settings.active.clone();
                        if self.audio_settings.configured {
                            self.audio_settings.request = Some(AudioSettingsRequest::Refresh);
                        }
                    }
                }
                theme::caption(ui, "PATCH / 模块合成");
            });
        });
    }

    fn module_library(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        theme::caption(ui, "浏览器");
        ui.heading("模块");
        ui.add_space(4.0);
        ui.add_sized(
            [ui.available_width(), 28.0],
            egui::TextEdit::singleline(&mut self.module_search)
                .hint_text("搜索模块")
                .margin(egui::vec2(8.0, 5.0)),
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let query = self.module_search.trim().to_lowercase();
            let mut matches = 0;
            for &family in ModuleFamily::ALL {
                let visible: Vec<_> = MODULE_CATALOG
                    .iter()
                    .filter(|desc| desc.family == family && desc.addable)
                    .filter(|desc| {
                        query.is_empty()
                            || desc.name.contains(&query)
                            || desc.code.to_lowercase().contains(&query)
                    })
                    .collect();
                if visible.is_empty() {
                    continue;
                }
                theme::caption(ui, family.label());
                ui.add_space(4.0);
                for desc in visible {
                    matches += 1;
                    let response = ui.add_enabled(
                        desc.addable,
                        egui::Button::new(format!("{}  ·  {}", desc.name, desc.code))
                            .min_size(egui::vec2(ui.available_width(), 26.0))
                            .corner_radius(0)
                            .fill(theme::PANEL)
                            .stroke(egui::Stroke::NONE),
                    );
                    if desc.addable {
                        if response.on_hover_text("点击添加到画布").clicked() {
                            let id = self.patch.add_node(desc.kind);
                            self.patch.selected = Some(id);
                            self.patch.recompile(&mut self.commands);
                        }
                    } else {
                        response.on_disabled_hover_text("计划中的模块，暂不可添加");
                    }
                }
                ui.add_space(8.0);
            }
            egui::CollapsingHeader::new("开发中的模块").show(ui, |ui| {
                for desc in MODULE_CATALOG.iter().filter(|d| !d.addable) {
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!("{} · {}", desc.name, desc.code)),
                    )
                    .on_disabled_hover_text("计划中的模块，暂不可添加");
                }
            });
            if matches == 0 {
                theme::caption(ui, "没有匹配的模块");
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.colored_label(
                if self.status.running() {
                    theme::GOOD
                } else {
                    theme::MUTED
                },
                if self.status.running() {
                    "引擎运行中"
                } else {
                    "引擎已停止"
                },
            );
            ui.separator();
            ui.add(
                egui::Label::new(
                    egui::RichText::new(if self.device_name.is_empty() {
                        "未打开设备"
                    } else {
                        &self.device_name
                    })
                    .size(12.0)
                    .color(theme::MUTED),
                )
                .truncate(),
            )
            .on_hover_text(self.error.as_deref().unwrap_or(&self.device_name));
            if self.error.is_some() {
                ui.colored_label(theme::DANGER, "设备错误")
                    .on_hover_text(self.error.as_deref().unwrap_or(""));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::caption(ui, "未保存");
                ui.separator();
                theme::caption(
                    ui,
                    &format!(
                        "{} Hz  /  {} 声道  /  {} 次中断",
                        self.status.sample_rate(),
                        self.status.channels(),
                        self.status.xruns()
                    ),
                );
            });
        });
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
    fn audio_sidebar_opens_applies_and_keeps_active_device_on_failure() {
        let (ctx, mut app, _consumer) = fixture();
        app.configure_audio_settings(
            AudioCatalog {
                backends: vec![waver_core::AudioBackend {
                    id: "WASAPI".into(),
                    label: "普通设备 / WASAPI".into(),
                    unavailable_reason: None,
                }],
                devices: vec![waver_core::AudioDevice {
                    id: "speaker".into(),
                    backend: "WASAPI".into(),
                    label: "测试声卡".into(),
                }],
                errors: vec![],
            },
            AudioSelection {
                backend: "WASAPI".into(),
                device: None,
            },
        );
        let size = egui::vec2(1000.0, 640.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "音频未连接").center(),
        );
        assert!(app.audio_settings.open);
        assert!(matches!(
            app.take_audio_request(),
            Some(AudioSettingsRequest::Refresh)
        ));
        let output = draw(&ctx, &mut app, size, vec![]);
        for label in ["音频设置", "声卡 / 输出设备", "应用音频设置"] {
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                    .contains_rect(text_rect(&output, label))
            );
        }
        app.audio_settings.draft.device = Some("speaker".into());
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "应用音频设置").center(),
        );
        assert_eq!(
            app.take_audio_request(),
            Some(AudioSettingsRequest::Apply(
                app.audio_settings.draft.clone()
            ))
        );
        app.audio_switch_failed("测试开流失败".into());
        assert_eq!(app.audio_settings.active.device, None);
        assert!(app.audio_settings.failed);
        let output = draw(&ctx, &mut app, size, vec![]);
        click(&ctx, &mut app, size, text_rect(&output, "关闭").center());
        assert!(!app.audio_settings.open);
    }

    #[test]
    fn toolbar_and_panels_fit_supported_window_sizes() {
        let (ctx, mut app, _consumer) = fixture();
        for size in [egui::vec2(1440.0, 960.0), egui::vec2(1000.0, 640.0)] {
            draw(&ctx, &mut app, size, vec![]);
            let output = draw(&ctx, &mut app, size, vec![]);
            let brand = text_rect(&output, "waver");
            let patch = text_rect(&output, "未命名 Patch");
            let status = text_rect(&output, "音频未连接");
            assert!(brand.right() < patch.left());
            assert!(patch.right() < status.left());
            assert!(status.right() < size.x);
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
            text_rect(&output, "压控振荡器  ·  VCO").center(),
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

    #[test]
    fn noise_library_entry_and_generic_inspector_fit_narrow_window() {
        let (ctx, mut app, _consumer) = fixture();
        let size = egui::vec2(1000.0, 640.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        click(
            &ctx,
            &mut app,
            size,
            text_rect(&output, "白噪声  ·  Noise").center(),
        );
        let selected = app.patch.selected.unwrap();
        assert_eq!(
            app.patch.graph.node(selected).unwrap().kind,
            waver_core::NodeKind::Noise
        );
        let output = draw(&ctx, &mut app, size, vec![]);
        let value_rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().parse::<f32>().ok() == Some(0.2) => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .expect("generic slider numeric value must be visible");
        assert!(
            value_rect.right() < size.x - 16.0,
            "numeric editor must fit inside inspector: {value_rect:?}"
        );
        let amp = app
            .patch
            .compiled
            .as_ref()
            .unwrap()
            .params
            .get(selected, ParamId::new(0))
            .unwrap();
        assert_eq!(amp.value(), 0.2);
        click(&ctx, &mut app, size, value_rect.center());
        assert_eq!(app.patch.selected, Some(selected));
    }

    #[test]
    fn vcf_cutoff_reads_in_hertz() {
        let (ctx, mut app, _consumer) = fixture();
        // Narrow window: the library collapses to a 180 px panel with an 8 px margin.
        let size = egui::vec2(1000.0, 640.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        let entry = text_rect(&output, "高切滤波器  ·  VCF");
        assert!(
            entry.right() < 172.0,
            "library entry must fit the narrow sidebar: {entry:?}"
        );
        click(&ctx, &mut app, size, entry.center());
        let selected = app.patch.selected.unwrap();
        assert_eq!(
            app.patch.graph.node(selected).unwrap().kind,
            waver_core::NodeKind::Vcf
        );
        let output = draw(&ctx, &mut app, size, vec![]);
        let hz = text_rect(&output, "2500 Hz");
        assert!(
            hz.right() < size.x - 16.0,
            "frequency readout must fit inside the inspector: {hz:?}"
        );
        let cutoff = app
            .patch
            .compiled
            .as_ref()
            .unwrap()
            .params
            .get(selected, ParamId::new(0))
            .unwrap();
        assert!((cutoff.value() - 2_500.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scope_library_entry_adds_a_monitoring_node() {
        let (ctx, mut app, _consumer) = fixture();
        let size = egui::vec2(1000.0, 640.0);
        draw(&ctx, &mut app, size, vec![]);
        let output = draw(&ctx, &mut app, size, vec![]);
        let entry = text_rect(&output, "示波器  ·  Scope");
        assert!(
            entry.right() < 172.0,
            "library entry must fit the narrow sidebar: {entry:?}"
        );
        click(&ctx, &mut app, size, entry.center());
        let selected = app.patch.selected.unwrap();
        assert_eq!(
            app.patch.graph.node(selected).unwrap().kind,
            waver_core::NodeKind::Scope
        );
        let output = draw(&ctx, &mut app, size, vec![]);
        let ports = text_rect(&output, "1 输入 / 0 输出");
        assert!(
            ports.right() < size.x - 16.0,
            "port readout must fit inside the inspector: {ports:?}"
        );
        let blurb = waver_core::NodeKind::Scope.desc().inspector_blurb;
        let blurb_rect = text_rect(&output, blurb);
        assert!(
            blurb_rect.right() < size.x - 16.0,
            "inspector blurb must fit: {blurb_rect:?}"
        );
    }
}

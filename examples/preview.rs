//! Silent UI preview: cargo run -p waver-ui --example preview
use eframe::egui;
use std::sync::Arc;
use waver_core::{EngineStatus, RtCommand};

struct Preview {
    app: waver_ui::WaverApp,
    commands: rtrb::Consumer<RtCommand>,
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while self.commands.pop().is_ok() {}
        if ui.input(|i| i.key_pressed(egui::Key::F6)) {
            let compact = ui.ctx().content_rect().width() > 1100.0;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(if compact {
                    egui::vec2(1000.0, 640.0)
                } else {
                    egui::vec2(1440.0, 960.0)
                }));
        }
        self.app.ui(ui);
    }
}

fn main() -> eframe::Result {
    let (producer, consumer) = rtrb::RingBuffer::new(256);
    let app = waver_ui::WaverApp::new(
        producer,
        Arc::new(EngineStatus::new()),
        "静音界面预览".into(),
        None,
    );
    eframe::run_native(
        "Waver · UI Preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 960.0])
                .with_min_inner_size([1000.0, 640.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            waver_ui::setup_fonts(&cc.egui_ctx);
            waver_ui::setup_theme(&cc.egui_ctx);
            Ok(Box::new(Preview {
                app,
                commands: consumer,
            }))
        }),
    )
}

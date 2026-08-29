//! Status-only shell. Node editor is not implemented yet.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use rtrb::Producer;
use waver_core::{EngineStatus, RtCommand};

/// GUI state: command producer plus a read-only engine snapshot.
pub struct WaverApp {
    commands: Producer<RtCommand>,
    status: Arc<EngineStatus>,
    device_name: String,
    error: Option<String>,
    last_queue: QueueNote,
}

#[derive(Clone, Copy)]
enum QueueNote {
    Idle,
    Sent,
    Full,
}

impl WaverApp {
    /// Bind to an already-created runtime (stream lives in the binary).
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
            last_queue: QueueNote::Idle,
        }
    }

    /// Draw the status panel into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.ctx()
            .request_repaint_after(Duration::from_millis(200));

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("waver");
            ui.label("模块化合成器骨架 · DSP 与 GUI 已拆开 · 节点编辑器未实现");
            ui.separator();

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
            }

            ui.heading("输出设备");
            let name = if self.device_name.is_empty() {
                "(未打开)"
            } else {
                self.device_name.as_str()
            };
            ui.label(format!("设备: {name}"));
            ui.label(format!(
                "采样率: {} Hz · 块大小: {} frames · 声道: {}",
                self.status.sample_rate(),
                self.status.block(),
                self.status.channels()
            ));
            ui.label(format!(
                "引擎: {} · xrun: {}",
                if self.status.running() {
                    "running"
                } else {
                    "stopped"
                },
                self.status.xruns()
            ));

            ui.separator();
            ui.heading("命令队列");
            ui.label("AllNotesOff 验证 GUI → 音频 SPSC。当前无发声器，听感不变。");
            if ui.button("All Notes Off").clicked() {
                self.last_queue = match self.commands.push(RtCommand::AllNotesOff) {
                    Ok(()) => QueueNote::Sent,
                    Err(_) => QueueNote::Full,
                };
            }
            match self.last_queue {
                QueueNote::Idle => {}
                QueueNote::Sent => {
                    ui.label("已入队");
                }
                QueueNote::Full => {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 160, 40),
                        "队列满或消费者已退出",
                    );
                }
            }
        });
    }
}

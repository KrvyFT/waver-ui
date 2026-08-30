//! Status shell + patch node editor.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use rtrb::Producer;
use waver_core::{EngineStatus, RtCommand};

use crate::editor::PatchEditor;
use crate::patch_state::PatchState;

/// GUI state: command producer plus a read-only engine snapshot.
pub struct WaverApp {
    commands: Producer<RtCommand>,
    status: Arc<EngineStatus>,
    device_name: String,
    error: Option<String>,
    last_queue: QueueNote,
    patch: PatchState,
    editor: PatchEditor,
    bootstrapped: bool,
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
            patch: PatchState::default_patch(),
            editor: PatchEditor::default(),
            bootstrapped: false,
        }
    }

    /// Draw the full application UI.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.ctx()
            .request_repaint_after(Duration::from_millis(50));

        if !self.bootstrapped {
            self.patch.recompile(&mut self.commands);
            self.bootstrapped = true;
        }

        // Panel::show on the App::ui root — do NOT nest CentralPanel (shrinks layout /
        // steals pointer layers). Right panel first so the remaining Ui is the editor.
        egui::Panel::right("status_panel")
            .default_size(220.0)
            .show(ui, |ui| {
                self.status_panel(ui);
            });

        ui.heading("waver · 节点编辑器");
        ui.label("拖标题移动 · 拖旋钮调参 · OUT→IN 连线 · 悬停线后 Delete/工具栏删除");
        ui.separator();
        self.editor.ui(ui, &mut self.patch, &mut self.commands);
    }

    fn status_panel(&mut self, ui: &mut egui::Ui) {
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
    }
}

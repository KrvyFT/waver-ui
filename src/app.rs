//! Status shell + patch node editor.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use rtrb::Producer;
use waver_core::{EngineStatus, RtCommand};

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

        // #region agent log
        let parent_avail = ui.available_rect_before_wrap();
        let parent_max = ui.max_rect();
        static APP_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let app_frame = APP_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // #endregion

        // Use an in-ui split instead of nesting CentralPanel/SidePanel inside App::ui
        // (nested panels steal layers and confuse pointer ownership).
        let status_width = 220.0;
        let full = ui.available_rect_before_wrap();
        let main_max = (full.width() - status_width - 8.0).max(200.0);

        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(main_max, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // #region agent log
                    if app_frame < 5 {
                        let central_avail = ui.available_rect_before_wrap();
                        waver_dbg(
                            "E",
                            "app.rs:ui",
                            "panel_nesting",
                            &format!(
                                "{{\"frame\":{app_frame},\"parent_avail\":[{:.1},{:.1},{:.1},{:.1}],\"parent_max\":[{:.1},{:.1},{:.1},{:.1}],\"central_avail\":[{:.1},{:.1},{:.1},{:.1}],\"layout\":\"split\"}}",
                                parent_avail.min.x, parent_avail.min.y, parent_avail.max.x, parent_avail.max.y,
                                parent_max.min.x, parent_max.min.y, parent_max.max.x, parent_max.max.y,
                                central_avail.min.x, central_avail.min.y, central_avail.max.x, central_avail.max.y
                            ),
                        );
                    }
                    // #endregion
                    ui.heading("waver · 节点编辑器");
                    ui.label("拖标题移动 · 拖旋钮调参 · OUT→IN 连线 · 悬停线后 Delete/工具栏删除");
                    ui.separator();
                    self.editor.ui(ui, &mut self.patch, &mut self.commands);
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(status_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.status_panel(ui);
                },
            );
        });
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

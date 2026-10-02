use crate::theme;
use eframe::egui;
use waver_core::{AudioCatalog, AudioSelection, AudioSettingsRequest, EngineStatus};

#[derive(Default)]
pub(crate) struct AudioSettings {
    pub open: bool,
    pub catalog: AudioCatalog,
    pub active: AudioSelection,
    pub draft: AudioSelection,
    pub request: Option<AudioSettingsRequest>,
    pub message: Option<String>,
    pub failed: bool,
    pub configured: bool,
}

impl AudioSettings {
    pub fn show(&mut self, ui: &mut egui::Ui, status: &EngineStatus, device_name: &str) {
        ui.horizontal(|ui| {
            ui.heading("音频设置");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("关闭").clicked() {
                    self.open = false;
                }
            });
        });
        ui.separator();
        ui.add_space(8.0);
        theme::caption(ui, "音频后端");
        let selected = self
            .catalog
            .backends
            .iter()
            .find(|b| b.id == self.draft.backend)
            .map(|b| b.label.as_str())
            .unwrap_or("选择后端");
        let before = self.draft.backend.clone();
        egui::ComboBox::from_id_salt("audio_backend")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for backend in &self.catalog.backends {
                    let response = ui.add_enabled(
                        backend.unavailable_reason.is_none(),
                        egui::Button::selectable(self.draft.backend == backend.id, &backend.label),
                    );
                    if response.clicked() {
                        self.draft.backend = backend.id.clone();
                    }
                    if let Some(reason) = &backend.unavailable_reason {
                        response.on_disabled_hover_text(reason);
                    }
                }
            });
        if before != self.draft.backend {
            self.draft.device = None;
            self.message = None;
        }
        for backend in &self.catalog.backends {
            if let Some(reason) = &backend.unavailable_reason {
                theme::caption(ui, &format!("{}：{}", backend.label, reason));
            }
        }
        ui.add_space(12.0);
        theme::caption(ui, "声卡 / 输出设备");
        let default_label = if self.draft.backend.eq_ignore_ascii_case("asio") {
            "首个可用输出设备"
        } else {
            "系统默认输出"
        };
        let selected = self
            .draft
            .device
            .as_ref()
            .map(|id| {
                self.catalog
                    .devices
                    .iter()
                    .find(|d| d.id == *id && d.backend == self.draft.backend)
                    .map(|d| d.label.as_str())
                    .unwrap_or("设备已断开")
            })
            .unwrap_or(default_label);
        egui::ComboBox::from_id_salt("audio_device")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.draft.device, None, default_label);
                for device in self
                    .catalog
                    .devices
                    .iter()
                    .filter(|d| d.backend == self.draft.backend)
                {
                    ui.selectable_value(
                        &mut self.draft.device,
                        Some(device.id.clone()),
                        &device.label,
                    );
                }
            });
        ui.add_space(6.0);
        if ui.button("刷新设备").clicked() {
            self.request = Some(AudioSettingsRequest::Refresh);
        }
        ui.add_space(12.0);
        ui.separator();
        theme::caption(ui, "当前输出");
        ui.label(if device_name.is_empty() {
            "未连接设备"
        } else {
            device_name
        });
        ui.label(format!(
            "{} Hz · {} 声道",
            status.sample_rate(),
            status.channels()
        ));
        theme::caption(ui, &format!("内部处理块：{} 帧", status.block()));
        theme::caption(ui, "采样率、声道和设备缓冲跟随设备默认配置。");
        ui.add_space(12.0);
        let valid_backend = self
            .catalog
            .backends
            .iter()
            .any(|b| b.id == self.draft.backend && b.unavailable_reason.is_none());
        let valid_device = self.draft.device.as_ref().is_none_or(|id| {
            self.catalog
                .devices
                .iter()
                .any(|d| d.id == *id && d.backend == self.draft.backend)
        });
        if ui
            .add_enabled(
                self.configured && valid_backend && valid_device,
                egui::Button::new("应用音频设置").fill(theme::ACCENT),
            )
            .clicked()
        {
            self.request = Some(AudioSettingsRequest::Apply(self.draft.clone()));
        }
        if self.draft != self.active {
            theme::caption(ui, "设置尚未应用");
        }
        if let Some(message) = &self.message {
            ui.colored_label(
                if self.failed {
                    theme::DANGER
                } else {
                    theme::GOOD
                },
                message,
            );
        }
        for error in &self.catalog.errors {
            ui.colored_label(theme::DANGER, error);
        }
        if !self.configured {
            theme::caption(ui, "静音预览未接入音频设备。");
        }
    }
}

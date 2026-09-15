//! Inspector waveform selector, synchronized with the on-module controls.

use crate::theme;
use eframe::egui;

pub fn wave_selector(ui: &mut egui::Ui, value: &mut f32) -> bool {
    theme::caption(ui, "波形");
    let before = value.round().clamp(0.0, 3.0) as usize;
    let mut selected = before;
    let names = ["正弦波", "锯齿波", "方波", "三角波"];
    egui::ComboBox::from_id_salt("inspector_wave")
        .width(ui.available_width())
        .selected_text(names[selected])
        .show_ui(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                ui.selectable_value(&mut selected, index, *name);
            }
        });
    *value = selected as f32;
    before != selected
}

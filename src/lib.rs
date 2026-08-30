//! egui shell. Depends on core only, not waver-dsp.

mod app;
mod editor;
mod fonts;
mod patch_state;

pub use app::WaverApp;
pub use fonts::setup as setup_fonts;

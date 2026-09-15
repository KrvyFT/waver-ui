//! egui shell. Depends on core only, not waver-dsp.

mod app;
mod editor;
mod fonts;
mod patch_state;
mod theme;

pub use app::WaverApp;
pub use fonts::setup as setup_fonts;
pub use theme::setup as setup_theme;

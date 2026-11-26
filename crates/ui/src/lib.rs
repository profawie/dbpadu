//! The dbpadu window: pages, dialogs and themes. The work behind them
//! lives elsewhere:
//!
//! - `dbpadu-core` (`crates/core`): saved connections, drivers, settings,
//!   saved passwords, SQL text helpers
//! - `dbpadu-engine` (`crates/engine`): starts the query engine and talks
//!   to it
//! - the query engine itself (`engine/`), written in Java

mod app;
mod icon;
mod theme;
mod ui;

pub use dbpadu_core::APP_NAME;
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Open the window and stay until it is closed.
pub fn run() -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_icon(icon::window_icon())
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
    .map_err(|err| err.to_string())
}

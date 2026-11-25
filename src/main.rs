// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The dbpadu desktop app: windows, pages and themes. The work behind
//! them lives elsewhere:
//!
//! - `dbpadu-core` (`crates/core`): saved connections, settings, saved
//!   passwords, SQL text helpers
//! - `dbpadu-engine` (`crates/engine`): starts the query engine and talks
//!   to it
//! - the query engine itself (`engine/`), written in Java

mod app;
mod theme;
mod ui;

pub use dbpadu_core::APP_NAME;
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

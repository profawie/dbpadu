// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The dbpadu program. It only starts the window; everything else is in
//! the shared libraries next to it:
//!
//! - `dbpadu_ui` (`crates/ui`): the window, its pages and dialogs
//! - `dbpadu_engine` (`crates/engine`): starts the query engine and talks
//!   to it
//! - `dbpadu_core` (`crates/core`): saved connections, drivers, settings,
//!   saved passwords, SQL text helpers
//!
//! The query engine itself (`engine/`) is written in Java and is one file,
//! `dbpadu-engine.jar`.

fn main() -> Result<(), String> {
    dbpadu_ui::run()
}

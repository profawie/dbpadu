//! What dbpadu keeps and knows without a database or a window: saved
//! connections and settings, saved passwords, and helpers for SQL text.

pub mod model;
pub mod secrets;
pub mod sql;
pub mod store;

pub const APP_NAME: &str = "dbpadu";

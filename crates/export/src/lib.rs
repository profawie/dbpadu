//! Query results as text: for the clipboard and for files.
//!
//! A result is written row by row through a [`Writer`], so a file of any
//! size is made without holding the rows in memory (only [`Format::Pretty`]
//! has to see every row first, to line its columns up).
//!
//! Nothing here knows about windows or databases: a result is its
//! [`Column`]s and rows of `Option<&str>`, where `None` is a NULL.

mod file;
mod format;
mod table;
mod value;
mod writer;

pub use file::{RowsFile, convert};
pub use format::{Format, Group};
pub use table::guess_table;
pub use writer::{Column, Options, Quote, Writer, to_string};

// The shared libraries form a chain: this one takes Rust's standard library
// and serde from `dbpadu_core` instead of carrying a second copy.
pub use dbpadu_core::APP_NAME;

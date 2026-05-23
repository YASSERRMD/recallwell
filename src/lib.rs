//! recallwell library entry point.
//!
//! The binary in `main.rs` uses these modules; integration tests under
//! `/tests` use them via `recallwell::...`.

pub mod cli;
pub mod commands;
pub mod config;
pub mod history;
pub mod ingest;
pub mod library;
pub mod server;
pub mod source;
pub mod ui;

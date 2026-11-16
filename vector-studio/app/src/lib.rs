//! Vector Studio: a native vector illustration app. The binary in `main.rs` only opens
//! the window; everything else lives here so the end-to-end tests can drive it.

pub mod a11y;
pub mod app;
pub mod canvas;
pub mod chrome;
pub mod commands;
pub mod dockers;
pub mod edit_tools;
pub mod files;
pub mod fill_ui;
pub mod guides;
#[cfg(target_os = "macos")]
pub mod native_menu;
pub mod outline;
pub mod page_dialogs;
pub mod paint_tools;
pub mod panels;
pub mod printing;
pub mod text_tool;
pub mod theme;
pub mod titlebar;
pub mod toolbox;
pub mod tools;

pub use app::{Startup, VectorStudioApp};

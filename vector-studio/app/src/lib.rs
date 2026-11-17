//! Vector Studio: a native vector illustration app. The binary in `main.rs` only opens
//! the window; everything else lives here so the end-to-end tests can drive it.

pub mod a11y;
pub mod app;
pub mod bitmap_menu;
pub mod canvas;
pub mod chrome;
pub mod commands;
pub mod dimensions;
pub mod dockers;
pub mod edit_tools;
pub mod export_for;
pub mod files;
pub mod fill_ui;
pub mod free_transform;
pub mod glyphs;
pub mod find_ui;
pub mod guides;
pub mod insert_codes;
#[cfg(target_os = "macos")]
pub mod native_menu;
pub mod outline;
pub mod page_dialogs;
pub mod page_sorter;
pub mod paint_tools;
pub mod panels;
pub mod paste_special;
pub mod printing;
pub mod templates;
pub mod text_menu;
pub mod text_tool;
pub mod theme;
pub mod titlebar;
pub mod toolbox;
pub mod tools;

pub use app::{Startup, VectorStudioApp};

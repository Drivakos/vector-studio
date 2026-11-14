//! Open, save and export flows (native dialogs via rfd).

use std::path::{Path, PathBuf};

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use vs_core::{Document, History, Rgba};
use vs_io::{ExportFormat, ExportOptions, FILE_EXTENSION};

use crate::app::AppState;

/// Settings of the Export window.
#[derive(Clone, Debug)]
pub struct ExportDialog {
    pub open: bool,
    pub format: ExportFormat,
    pub dpi: f64,
    pub white_background: bool,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self { open: false, format: ExportFormat::Png, dpi: 300.0, white_background: false }
    }
}

fn file_stem(state: &AppState) -> String {
    state
        .file_path
        .as_deref()
        .and_then(Path::file_stem)
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| state.doc.title.clone())
}

fn error(title: &str, detail: impl std::fmt::Display) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(title)
        .set_description(detail.to_string())
        .set_buttons(MessageButtons::Ok)
        .show();
}

/// Ask to save unsaved changes. Returns false if the user cancelled.
pub fn confirm_discard(state: &mut AppState) -> bool {
    state.history.commit(&state.doc);
    if !state.history.is_dirty() {
        return true;
    }
    let answer = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description(format!("Save changes to “{}” before closing it?", file_stem(state)))
        .set_buttons(MessageButtons::YesNoCancel)
        .show();
    match answer {
        MessageDialogResult::Yes => save(state, false),
        MessageDialogResult::No => true,
        _ => false,
    }
}

/// Replace the open document.
pub fn set_document(state: &mut AppState, doc: Document, path: Option<PathBuf>) {
    // A fresh or just-opened document has nothing to save yet.
    state.history = History::new(&doc, true);
    state.doc = doc;
    state.file_path = path;
    state.selection.clear();
    state.tools = Default::default();
    state.needs_fit = true;
}

/// Save to the current file, or ask for one. Returns true if the document was saved.
pub fn save(state: &mut AppState, ask: bool) -> bool {
    let path = match (&state.file_path, ask) {
        (Some(p), false) => p.clone(),
        _ => {
            let Some(mut p) = FileDialog::new()
                .set_title("Save Drawing")
                .add_filter("Vector Studio drawing", &[FILE_EXTENSION])
                .set_file_name(format!("{}.{FILE_EXTENSION}", file_stem(state)))
                .save_file()
            else {
                return false;
            };
            if p.extension().is_none() {
                p.set_extension(FILE_EXTENSION);
            }
            p
        }
    };
    state.history.commit(&state.doc);
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        state.doc.title = stem.to_owned();
    }
    match vs_io::save_document(&state.doc, &path) {
        Ok(()) => {
            // The title change above is not a page edit, so it adds no undo step.
            state.history.commit(&state.doc);
            state.history.mark_saved();
            state.file_path = Some(path);
            true
        }
        Err(e) => {
            error("Could not save", format!("{}: {e}", path.display()));
            false
        }
    }
}

pub fn open(state: &mut AppState) {
    if !confirm_discard(state) {
        return;
    }
    let Some(path) = FileDialog::new()
        .set_title("Open Drawing")
        .add_filter("Vector Studio drawing", &[FILE_EXTENSION])
        .pick_file()
    else {
        return;
    };
    open_path(state, &path);
}

pub fn open_path(state: &mut AppState, path: &Path) {
    match vs_io::open_document(path) {
        Ok(doc) => set_document(state, doc, Some(path.to_path_buf())),
        Err(e) => error("Could not open", format!("{}: {e}", path.display())),
    }
}

/// Ask where to export, then write the file.
pub fn export(state: &mut AppState) {
    let settings = state.export.clone();
    let format = settings.format;
    let Some(mut path) = FileDialog::new()
        .set_title("Export")
        .add_filter(format.label(), &[format.extension()])
        .set_file_name(format!("{}.{}", file_stem(state), format.extension()))
        .save_file()
    else {
        return;
    };
    if ExportFormat::from_path(&path).ok() != Some(format) {
        path.set_extension(format.extension());
    }
    let options = ExportOptions {
        dpi: settings.dpi,
        background: settings.white_background.then_some(Rgba::WHITE),
    };
    match vs_io::export(&state.doc, &path, &options) {
        Ok(_) => {
            state.export.open = false;
            state.status = Some(format!("Exported {}", path.display()));
        }
        Err(e) => error("Could not export", format!("{}: {e}", path.display())),
    }
}

/// The Export window: format and options, then a save dialog.
pub fn export_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.export.open {
        return;
    }
    let mut open = true;
    let mut go = false;
    egui::Window::new("Export")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("export_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Format:");
                egui::ComboBox::from_id_salt("export_format")
                    .selected_text(state.export.format.label())
                    .show_ui(ui, |ui| {
                        for f in ExportFormat::ALL {
                            ui.selectable_value(&mut state.export.format, f, f.label());
                        }
                    });
                ui.end_row();
                match state.export.format {
                    ExportFormat::Png => {
                        ui.label("Resolution:");
                        ui.add(egui::DragValue::new(&mut state.export.dpi).range(18.0..=1200.0).speed(1.0).suffix(" dpi"));
                        ui.end_row();
                        ui.label("Background:");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut state.export.white_background, false, "Transparent");
                            ui.radio_value(&mut state.export.white_background, true, "White");
                        });
                        ui.end_row();
                        let size = state.doc.active_page().size;
                        let k = state.export.dpi / 72.0;
                        ui.label("Image size:");
                        ui.label(format!("{} × {} px", (size.width * k).ceil(), (size.height * k).ceil()));
                        ui.end_row();
                    }
                    ExportFormat::Svg => {
                        ui.label("Exports:");
                        ui.label("the current page");
                        ui.end_row();
                    }
                    ExportFormat::Pdf => {
                        ui.label("Exports:");
                        ui.label(format!("all {} page(s)", state.doc.page_count()));
                        ui.end_row();
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Export…").clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    state.export.open = false;
                }
            });
        });
    if !open {
        state.export.open = false;
    }
    if go {
        export(state);
    }
}

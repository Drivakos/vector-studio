//! Open, save and export flows. Dialogs go through [`Dialogs`]: native ones (rfd) in the
//! app, scripted ones in the end-to-end tests.

use std::path::{Path, PathBuf};

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use vs_core::kurbo::{Affine, Vec2};
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

/// The drawing's name: its file name without the extension, or its title until saved.
pub fn file_stem(state: &AppState) -> String {
    state
        .file_path
        .as_deref()
        .and_then(Path::file_stem)
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| state.doc.title.clone())
}

/// Answer to "save changes before closing?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsaved {
    Save,
    Discard,
    Cancel,
}

/// The file and message dialogs the app shows.
pub trait Dialogs {
    /// Ask where to save. `extension` is the one file type offered.
    fn save_file(&mut self, title: &str, filter: &str, extension: &str, file_name: &str) -> Option<PathBuf>;
    /// Ask for a file to read with one of `extensions`.
    fn open_file(&mut self, title: &str, filter: &str, extensions: &[&str]) -> Option<PathBuf>;
    fn unsaved_changes(&mut self, document: &str) -> Unsaved;
    /// Ask OK or Cancel; true for OK.
    fn confirm(&mut self, title: &str, detail: &str) -> bool;
    fn error(&mut self, title: &str, detail: &str);
}

/// The system's own dialogs.
pub struct NativeDialogs;

impl Dialogs for NativeDialogs {
    fn save_file(&mut self, title: &str, filter: &str, extension: &str, file_name: &str) -> Option<PathBuf> {
        FileDialog::new().set_title(title).add_filter(filter, &[extension]).set_file_name(file_name).save_file()
    }

    fn open_file(&mut self, title: &str, filter: &str, extensions: &[&str]) -> Option<PathBuf> {
        FileDialog::new().set_title(title).add_filter(filter, extensions).pick_file()
    }

    fn unsaved_changes(&mut self, document: &str) -> Unsaved {
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description(format!("Save changes to “{document}” before closing it?"))
            .set_buttons(MessageButtons::YesNoCancel)
            .show();
        match answer {
            MessageDialogResult::Yes => Unsaved::Save,
            MessageDialogResult::No => Unsaved::Discard,
            _ => Unsaved::Cancel,
        }
    }

    fn confirm(&mut self, title: &str, detail: &str) -> bool {
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(title)
            .set_description(detail)
            .set_buttons(MessageButtons::OkCancel)
            .show();
        answer == MessageDialogResult::Ok
    }

    fn error(&mut self, title: &str, detail: &str) {
        MessageDialog::new()
            .set_level(MessageLevel::Error)
            .set_title(title)
            .set_description(detail)
            .set_buttons(MessageButtons::Ok)
            .show();
    }
}

fn error(state: &mut AppState, title: &str, detail: impl std::fmt::Display) {
    state.dialogs.error(title, &detail.to_string());
}

/// Ask to save unsaved changes. Returns false if the user cancelled.
pub fn confirm_discard(state: &mut AppState) -> bool {
    state.history.commit(&state.doc);
    if !state.history.is_dirty() {
        return true;
    }
    let name = file_stem(state);
    match state.dialogs.unsaved_changes(&name) {
        Unsaved::Save => save(state, false),
        Unsaved::Discard => true,
        Unsaved::Cancel => false,
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
            let name = format!("{}.{FILE_EXTENSION}", file_stem(state));
            let Some(mut p) = state.dialogs.save_file("Save Drawing", "Vector Studio drawing", FILE_EXTENSION, &name) else {
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
            error(state, "Could not save", format!("{}: {e}", path.display()));
            false
        }
    }
}

pub fn open(state: &mut AppState) {
    if !confirm_discard(state) {
        return;
    }
    let Some(path) = state.dialogs.open_file("Open Drawing", "Vector Studio drawing", &[FILE_EXTENSION]) else {
        return;
    };
    open_path(state, &path);
}

/// Close the drawing (after asking to save changes) and start a new one in its place.
pub fn close(state: &mut AppState) {
    if confirm_discard(state) {
        state.untitled_count += 1;
        let doc = Document::new(format!("Untitled-{}", state.untitled_count));
        set_document(state, doc, None);
    }
}

/// Whether Revert applies: the drawing has a file and changes since it was saved.
pub fn can_revert(state: &AppState) -> bool {
    state.file_path.is_some() && state.history.is_dirty()
}

/// Throw away the changes since the last save and reload the file, after asking.
pub fn revert(state: &mut AppState) {
    state.history.commit(&state.doc);
    let Some(path) = state.file_path.clone().filter(|_| state.history.is_dirty()) else { return };
    let name = file_stem(state);
    if state.dialogs.confirm("Revert", &format!("Discard the changes to “{name}” and go back to the saved version?")) {
        open_path(state, &path);
    }
}

pub fn open_path(state: &mut AppState, path: &Path) {
    match vs_io::open_document(path) {
        Ok(doc) => set_document(state, doc, Some(path.to_path_buf())),
        Err(e) => error(state, "Could not open", format!("{}: {e}", path.display())),
    }
}

/// What Import offers.
pub const IMPORT_EXTENSIONS: &[&str] = &["svg", "png", "jpg", "jpeg"];

/// Ask for an SVG, PNG or JPEG file and place it in the middle of the page, selected.
pub fn import(state: &mut AppState) {
    let Some(path) = state.dialogs.open_file("Import", "SVG, PNG or JPEG image", IMPORT_EXTENSIONS) else {
        return;
    };
    import_path(state, &path);
}

pub fn import_path(state: &mut AppState, path: &Path) {
    let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    let result = std::fs::read(path).map_err(vs_io::IoError::from).and_then(|data| {
        if svg { vs_io::import_svg(&data) } else { vs_io::import_bitmap(&data).map(|o| vec![o]) }
    });
    match result {
        Ok(objects) => {
            let Some(bounds) = objects.iter().map(|o| o.world_bounds()).reduce(|a, b| a.union(b)) else {
                error(state, "Could not import", format!("{}: the file draws nothing that can be imported", path.display()));
                return;
            };
            // Larger than the page: scaled down to fit it, as a placed image would be.
            let page = state.doc.active_page().bounds();
            let k = (page.width() / bounds.width()).min(page.height() / bounds.height()).min(1.0);
            let fit = Affine::translate(page.center().to_vec2()) * Affine::scale(k) * Affine::translate(-bounds.center().to_vec2());
            let objects: Vec<_> = objects.into_iter().map(|mut o| {
                o.transform = fit * o.transform;
                o
            }).collect();
            state.selection = state.doc.paste_objects(&objects, Vec2::ZERO);
            state.marquee_selection = None;
            state.tools.nodes.clear();
            state.status = Some(format!("Imported {}", path.display()));
        }
        Err(e) => error(state, "Could not import", format!("{}: {e}", path.display())),
    }
}

/// Ask where to export, then write the file.
pub fn export(state: &mut AppState) {
    let settings = state.export.clone();
    let format = settings.format;
    let name = format!("{}.{}", file_stem(state), format.extension());
    let Some(mut path) = state.dialogs.save_file("Export", format.label(), format.extension(), &name) else {
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
        Err(e) => error(state, "Could not export", format!("{}: {e}", path.display())),
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
                let format = egui::ComboBox::from_id_salt("export_format")
                    .selected_text(state.export.format.label())
                    .show_ui(ui, |ui| {
                        for f in ExportFormat::ALL {
                            ui.selectable_value(&mut state.export.format, f, f.label());
                        }
                    });
                crate::a11y::name(&format.response, "Export format");
                ui.end_row();
                match state.export.format {
                    ExportFormat::Png => {
                        ui.label("Resolution:");
                        crate::a11y::add(ui, egui::DragValue::new(&mut state.export.dpi).range(18.0..=1200.0).speed(1.0).suffix(" dpi"), "Resolution");
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

/// File > Document Properties: title, subject, author, copyright, keywords and notes.
/// Edits apply to the document as they are typed.
pub fn document_properties_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_document_properties {
        return;
    }
    let mut open = true;
    let mut done = false;
    egui::Window::new("Document Properties")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let p = &mut state.doc.properties;
            egui::Grid::new("document_properties").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                for (label, value) in [("Title", &mut p.title), ("Subject", &mut p.subject), ("Author", &mut p.author), ("Copyright", &mut p.copyright), ("Keywords", &mut p.keywords)] {
                    ui.label(format!("{label}:"));
                    crate::a11y::add(ui, egui::TextEdit::singleline(value).desired_width(260.0), &format!("{label} (document properties)"));
                    ui.end_row();
                }
                ui.label("Notes:");
                crate::a11y::add(ui, egui::TextEdit::multiline(&mut p.notes).desired_width(260.0).desired_rows(4), "Notes (document properties)");
                ui.end_row();
            });
            ui.label(egui::RichText::new("Separate keywords with commas. Notes are kept in the drawing only.").weak().small());
            ui.add_space(8.0);
            done = ui.button("OK").clicked();
        });
    state.show_document_properties = open && !done;
}

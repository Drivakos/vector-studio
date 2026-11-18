//! File > Print Merge: import records from a CSV or tab-separated text file, put its
//! fields into text as «Name», then merge to a new drawing or print, with a copy of the
//! pages for each record.

use vs_core::kurbo::Affine;
use vs_core::merge::token;
use vs_core::{History, MergeData, Shape, Style, Text, TextAlign};

use crate::app::AppState;
use crate::files;

const WINDOW: &str = "Print Merge";

/// The open Print Merge window: the record it shows, 0-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrintMergeDialog {
    record: usize,
}

pub fn open(state: &mut AppState) {
    state.print_merge = Some(PrintMergeDialog::default());
}

/// Ask for a data file and make it the drawing's merge data.
fn import(state: &mut AppState) {
    let Some(path) = state.dialogs.open_file("Import Merge Data", "CSV or text file", &["csv", "txt", "tsv"]) else { return };
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| MergeData::parse(&t).map_err(|e| e.to_string()));
    match data {
        Ok(data) => state.doc.merge = Some(data),
        Err(e) => state.dialogs.error("Could not import merge data", &format!("{}: {e}", path.display())),
    }
}

/// Put field `name` at the caret of the text being typed; otherwise add it to the page
/// as new artistic text in the font for new text, centred and selected.
fn insert_field(state: &mut AppState, name: &str) {
    let field = token(name);
    if state.tools.text.is_some() {
        crate::text_tool::insert_text(state, &field);
        return;
    }
    let text = Text { content: field, align: TextAlign::Center, frame: None, on_path: None, field: None, ..state.tools.settings.text.clone() };
    let centre = state.doc.active_page().bounds().center().to_vec2();
    let id = state.doc.add_object(Shape::Text(text), Style::new_text());
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.transform = Affine::translate(centre);
    }
    state.selection = vec![id];
    state.marquee_selection = None;
}

/// Start a new, untitled drawing holding the merged pages, after asking to save this one.
/// True once it is open.
fn merge_to_new(state: &mut AppState) -> bool {
    let Some(mut doc) = state.doc.merged() else { return false };
    let records = state.doc.merge.as_ref().map_or(0, |d| d.records.len());
    if !files::confirm_discard(state) {
        return false;
    }
    state.untitled_count += 1;
    doc.title = format!("Untitled-{}", state.untitled_count);
    let pages = doc.page_count();
    files::set_document(state, doc, None);
    // Unlike an opened file, the merged drawing is not saved anywhere yet.
    state.history = History::new(&state.doc, false);
    state.status = Some(format!("Merged {records} record{} into {pages} page{}", plural(records), plural(pages)));
    true
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

enum Action {
    Import,
    Remove,
    Insert(String),
    Merge,
    Print,
    Close,
}

/// The Print Merge window: the fields with the shown record's values, buttons to put a
/// field into text, and Merge to New Document or Print….
pub fn print_merge_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.print_merge else { return };
    let data = state.doc.merge.clone();
    let mut open = true;
    let mut action = None;
    egui::Window::new(WINDOW)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            match &data {
                None => {
                    ui.label("Import a CSV or tab-separated text file. Its first row names the fields; each row after it is a record.");
                }
                Some(data) => {
                    let count = data.records.len();
                    dialog.record = dialog.record.min(count.saturating_sub(1));
                    ui.horizontal(|ui| {
                        if crate::a11y::window_button(ui, dialog.record > 0, "Previous", WINDOW).clicked() {
                            dialog.record -= 1;
                        }
                        ui.label(format!("Record {} of {count}", dialog.record + 1));
                        if crate::a11y::window_button(ui, dialog.record + 1 < count, "Next", WINDOW).clicked() {
                            dialog.record += 1;
                        }
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        egui::Grid::new("print_merge_fields").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                            for (i, field) in data.fields.iter().enumerate() {
                                let button = crate::a11y::add(ui, egui::Button::new(token(field)), &format!("Insert field {field}"));
                                if button.on_hover_text("Put this field in the text").clicked() {
                                    action = Some(Action::Insert(field.clone()));
                                }
                                ui.label(data.records.get(dialog.record).map_or("", |r| r[i].as_str()));
                                ui.end_row();
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.weak("A field goes into the text being typed, or onto the page as new text.");
                }
            }
            ui.add_space(8.0);
            let has_data = data.is_some();
            ui.horizontal(|ui| {
                if crate::a11y::window_button(ui, true, "Import Data…", WINDOW).clicked() {
                    action = Some(Action::Import);
                }
                if crate::a11y::window_button(ui, has_data, "Remove Data", WINDOW).clicked() {
                    action = Some(Action::Remove);
                }
            });
            ui.horizontal(|ui| {
                if crate::a11y::window_button(ui, has_data, "Merge to New Document", WINDOW).clicked() {
                    action = Some(Action::Merge);
                }
                if crate::a11y::window_button(ui, has_data, "Print…", WINDOW).clicked() {
                    action = Some(Action::Print);
                }
                if crate::a11y::window_button(ui, true, "Close", WINDOW).clicked() {
                    action = Some(Action::Close);
                }
            });
        });
    state.print_merge = open.then_some(dialog);
    match action {
        Some(Action::Import) => import(state),
        Some(Action::Remove) => state.doc.merge = None,
        Some(Action::Insert(name)) => insert_field(state, &name),
        Some(Action::Merge) => {
            if merge_to_new(state) {
                state.print_merge = None;
            }
        }
        Some(Action::Print) => {
            if let Some(doc) = state.doc.merged() {
                let records = state.doc.merge.as_ref().map_or(0, |d| d.records.len());
                state.print_merge = None;
                crate::printing::open_merged(state, doc, records);
            }
        }
        Some(Action::Close) => state.print_merge = None,
        None => {}
    }
}

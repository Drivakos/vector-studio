//! File > Save as Template and New from Template. Templates are drawings saved in the
//! app's Templates folder; a new drawing made from one starts untitled, with the
//! template's pages, objects and settings.

use std::path::{Path, PathBuf};

use crate::app::AppState;
use crate::files;

/// File extension of templates.
pub const TEMPLATE_EXTENSION: &str = "vstemplate";

/// The Templates folder in the app's support folder.
pub fn default_dir() -> Option<PathBuf> {
    eframe::storage_dir("Vector Studio").map(|d| d.join("Templates"))
}

/// The open Save as Template window: the name typed so far.
#[derive(Clone, Debug)]
pub struct SaveTemplateDialog {
    name: String,
}

/// The open New from Template window: the templates found and the one chosen.
#[derive(Clone, Debug)]
pub struct NewFromTemplateDialog {
    templates: Vec<(String, PathBuf)>,
    chosen: Option<usize>,
}

/// Templates in `dir`, by name.
pub fn list(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == TEMPLATE_EXTENSION))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_owned(), p)))
        .collect();
    out.sort_by_key(|(name, _)| name.to_lowercase());
    out
}

/// A name that is safe as a file name: no path separators, no leading dot.
fn file_name(name: &str) -> Option<String> {
    let name: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect();
    let name = name.trim_start_matches('.').trim().to_owned();
    (!name.is_empty()).then_some(name)
}

pub fn open_save(state: &mut AppState) {
    state.save_template = Some(SaveTemplateDialog { name: files::file_stem(state) });
}

/// Save the drawing as template `name`, asking before replacing one. True once saved.
fn save(state: &mut AppState, name: &str) -> bool {
    let Some(dir) = state.templates_dir.clone() else {
        state.dialogs.error("Could not save template", "There is no Templates folder.");
        return false;
    };
    let Some(name) = file_name(name) else { return false };
    let path = dir.join(format!("{name}.{TEMPLATE_EXTENSION}"));
    if path.exists() && !state.dialogs.confirm("Replace template", &format!("A template named “{name}” already exists. Replace it?")) {
        return false;
    }
    let mut doc = state.doc.clone();
    doc.title = name.clone();
    match std::fs::create_dir_all(&dir).map_err(vs_io::IoError::from).and_then(|()| vs_io::save_document(&doc, &path)) {
        Ok(()) => {
            state.status = Some(format!("Saved template {name}"));
            true
        }
        Err(e) => {
            state.dialogs.error("Could not save template", &format!("{}: {e}", path.display()));
            false
        }
    }
}

/// The Save as Template window: a name, then Save.
pub fn save_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.save_template.take() else { return };
    let mut open = true;
    let (mut go, mut cancel) = (false, false);
    egui::Window::new("Save as Template")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                crate::a11y::add(ui, egui::TextEdit::singleline(&mut dialog.name).desired_width(220.0), "Template name");
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go = crate::a11y::window_button(ui, file_name(&dialog.name).is_some(), "Save", "Save as Template").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Save as Template").clicked();
            });
        });
    if go && save(state, &dialog.name) {
        return;
    }
    if open && !cancel {
        state.save_template = Some(dialog);
    }
}

pub fn open_new(state: &mut AppState) {
    let templates = state.templates_dir.as_deref().map(list).unwrap_or_default();
    let chosen = (!templates.is_empty()).then_some(0);
    state.new_from_template = Some(NewFromTemplateDialog { templates, chosen });
}

/// Start a new, untitled drawing from the template at `path`, after asking to save the
/// current one. True once it is open.
pub fn new_from(state: &mut AppState, path: &Path) -> bool {
    let doc = match vs_io::open_document(path) {
        Ok(doc) => doc,
        Err(e) => {
            state.dialogs.error("Could not open template", &format!("{}: {e}", path.display()));
            return false;
        }
    };
    if !files::confirm_discard(state) {
        return false;
    }
    state.untitled_count += 1;
    let mut doc = doc;
    doc.title = format!("Untitled-{}", state.untitled_count);
    files::set_document(state, doc, None);
    true
}

/// The New from Template window: the saved templates, Browse… for any other, then Open.
pub fn new_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.new_from_template.take() else { return };
    let mut open = true;
    let (mut go, mut browse, mut cancel) = (false, false, false);
    egui::Window::new("New from Template")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            if dialog.templates.is_empty() {
                ui.label("No templates yet. Save one with File › Save as Template.");
            } else {
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    for (i, (name, _)) in dialog.templates.iter().enumerate() {
                        let item = ui.selectable_label(dialog.chosen == Some(i), name);
                        crate::a11y::name(&item, &format!("Template {name}"));
                        if item.clicked() {
                            dialog.chosen = Some(i);
                        }
                        if item.double_clicked() {
                            go = true;
                        }
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go |= crate::a11y::window_button(ui, dialog.chosen.is_some(), "Open", "New from Template").clicked();
                browse = crate::a11y::window_button(ui, true, "Browse…", "New from Template").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "New from Template").clicked();
            });
        });
    let path = if go {
        dialog.chosen.and_then(|i| dialog.templates.get(i)).map(|(_, p)| p.clone())
    } else if browse {
        state.dialogs.open_file("New from Template", "Vector Studio template", &[TEMPLATE_EXTENSION, vs_io::FILE_EXTENSION])
    } else {
        None
    };
    if let Some(path) = path
        && new_from(state, &path)
    {
        return;
    }
    if open && !cancel {
        state.new_from_template = Some(dialog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_names_become_safe_file_names() {
        assert_eq!(file_name(" Poster A3 ").as_deref(), Some("Poster A3"));
        assert_eq!(file_name("a/b:c").as_deref(), Some("a-b-c"));
        assert_eq!(file_name("..hidden").as_deref(), Some("hidden"));
        assert_eq!(file_name("  "), None);
    }
}

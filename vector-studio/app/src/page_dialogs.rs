//! Windows of the Layout menu: Go to Page, Page Background and Document Options.

use vs_core::{Background, Rgba};

use crate::app::AppState;

/// Show the Go to Page window, starting at the active page.
pub fn open_go_to_page(state: &mut AppState) {
    state.go_to_page = Some(state.doc.active_page_index() + 1);
}

/// The Go to Page window: a page number, then Go.
pub fn go_to_page_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut number) = state.go_to_page else { return };
    let count = state.doc.page_count();
    let mut open = true;
    let (mut go, mut cancel) = (false, false);
    egui::Window::new("Go to Page")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Page:");
                crate::a11y::add(ui, egui::DragValue::new(&mut number).range(1..=count).speed(0.1), "Page number (go to page)");
                ui.label(format!("of {count}"));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go = crate::a11y::window_button(ui, true, "Go", "Go to Page").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Go to Page").clicked();
            });
        });
    state.go_to_page = (open && !go && !cancel).then_some(number);
    if go {
        let _ = state.doc.set_active_page(number.clamp(1, count) - 1);
    }
}

/// The open Page Background window. Its edits apply to the document as they are made.
#[derive(Clone, Debug)]
pub struct PageBackgroundDialog {
    /// The colour Solid uses, kept while No background is chosen.
    color: Rgba,
    /// The hex field as typed.
    hex: String,
}

pub fn open_page_background(state: &mut AppState) {
    let color = state.doc.background.map_or(Rgba::WHITE, |b| b.color);
    state.page_background = Some(PageBackgroundDialog { color, hex: color.to_hex() });
}

/// The Page Background window: none or a solid colour, and whether it prints and exports.
pub fn page_background_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.page_background.take() else { return };
    let mut open = true;
    let mut done = false;
    let mut background = state.doc.background;
    egui::Window::new("Page Background")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            if ui.radio(background.is_none(), "No background").clicked() {
                background = None;
            }
            ui.horizontal(|ui| {
                if ui.radio(background.is_some(), "Solid colour").clicked() && background.is_none() {
                    background = Some(Background { color: dialog.color, print_and_export: true });
                }
                let mut c = [dialog.color.r, dialog.color.g, dialog.color.b];
                let button = ui.color_edit_button_srgb(&mut c);
                crate::a11y::name(&button, "Background colour");
                let mut picked = button.changed().then(|| Rgba::rgb(c[0], c[1], c[2]));
                let field = crate::a11y::add(ui, egui::TextEdit::singleline(&mut dialog.hex).desired_width(70.0), "Background colour (hex)");
                if field.lost_focus() {
                    match Rgba::from_hex(&dialog.hex) {
                        Some(c) if c != dialog.color => picked = Some(c),
                        _ => dialog.hex = dialog.color.to_hex(),
                    }
                }
                if let Some(color) = picked {
                    dialog.color = color;
                    dialog.hex = color.to_hex();
                    background = Some(Background { color, print_and_export: background.is_none_or(|b| b.print_and_export) });
                }
            });
            ui.add_enabled_ui(background.is_some(), |ui| {
                let mut print = background.is_some_and(|b| b.print_and_export);
                if ui.checkbox(&mut print, "Print and export background").changed()
                    && let Some(b) = &mut background
                {
                    b.print_and_export = print;
                }
            });
            ui.add_space(8.0);
            done = crate::a11y::window_button(ui, true, "OK", "Page Background").clicked();
        });
    state.doc.background = background;
    if open && !done {
        state.page_background = Some(dialog);
    }
}

/// The open Document Options window: settings as edited, in the units chosen in it,
/// applied on OK.
#[derive(Clone, Debug)]
pub struct DocumentOptionsDialog {
    unit: vs_core::Unit,
    /// Page width and height, grid spacing, nudge, duplicate offset x and y, in `unit`.
    width: f64,
    height: f64,
    all_pages: bool,
    grid: f64,
    nudge: f64,
    duplicate: [f64; 2],
    /// The document's CMYK profile and rendering intent.
    color: vs_core::ColorSettings,
}

impl DocumentOptionsDialog {
    /// Change the units the window shows, converting what was typed.
    fn set_unit(&mut self, unit: vs_core::Unit) {
        let k = self.unit.to_points(1.0) / unit.to_points(1.0);
        let [dx, dy] = &mut self.duplicate;
        for v in [&mut self.width, &mut self.height, &mut self.grid, &mut self.nudge, dx, dy] {
            *v *= k;
        }
        self.unit = unit;
    }
}

pub fn open_document_options(state: &mut AppState) {
    let unit = state.doc.display_unit;
    let size = state.doc.active_page().size;
    let p = &state.prefs;
    state.document_options = Some(DocumentOptionsDialog {
        unit,
        width: unit.from_points(size.width),
        height: unit.from_points(size.height),
        all_pages: false,
        grid: unit.from_points(p.grid_spacing),
        nudge: unit.from_points(p.nudge),
        duplicate: p.duplicate_offset.map(|v| unit.from_points(v)),
        color: state.doc.color.clone(),
    });
}

fn apply_document_options(state: &mut AppState, d: &DocumentOptionsDialog) {
    let pt = |v: f64| d.unit.to_points(v);
    state.doc.display_unit = d.unit;
    let size = vs_core::kurbo::Size::new(pt(d.width), pt(d.height));
    if d.all_pages {
        for i in 0..state.doc.page_count() {
            if let Some(page) = state.doc.page_mut(i) {
                page.size = size;
            }
        }
    } else {
        state.doc.active_page_mut().size = size;
    }
    state.prefs.grid_spacing = pt(d.grid);
    state.prefs.nudge = pt(d.nudge);
    state.prefs.duplicate_offset = d.duplicate.map(pt);
    if state.doc.color != d.color {
        state.doc.color = d.color.clone();
    }
}

/// Layout > Document Options: units, page size, grid spacing, nudge, duplicate distance
/// and the CMYK profile and rendering intent in one window. OK applies them, Cancel leaves everything as it was.
pub fn document_options_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.document_options.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Document Options")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("document_options_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Units:");
                let mut unit = d.unit;
                let combo = egui::ComboBox::from_id_salt("document_options_units").selected_text(unit.label()).height(400.0).show_ui(ui, |ui| {
                    for u in vs_core::Unit::ALL {
                        ui.selectable_value(&mut unit, u, u.label());
                    }
                });
                crate::a11y::name(&combo.response, "Units (document options)");
                if unit != d.unit {
                    d.set_unit(unit);
                }
                ui.end_row();
                let unit = d.unit;
                let field = |ui: &mut egui::Ui, v: &mut f64, min: f64, name: &str| {
                    let drag = egui::DragValue::new(v).speed(unit.step() / 2.0).range(min..=f64::MAX).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix()));
                    crate::a11y::add(ui, drag, name);
                };
                ui.label("Page size:");
                ui.horizontal(|ui| {
                    field(ui, &mut d.width, unit.from_points(1.0), "Page width (document options)");
                    ui.label("×");
                    field(ui, &mut d.height, unit.from_points(1.0), "Page height (document options)");
                });
                ui.end_row();
                ui.label("");
                crate::a11y::add(ui, egui::Checkbox::new(&mut d.all_pages, "Apply to all pages"), "Apply to all pages");
                ui.end_row();
                ui.label("Grid spacing:");
                field(ui, &mut d.grid, unit.from_points(0.5), "Grid spacing (document options)");
                ui.end_row();
                ui.label("Nudge:");
                field(ui, &mut d.nudge, unit.from_points(0.01), "Nudge (document options)");
                ui.end_row();
                ui.label("Duplicate offset:");
                ui.horizontal(|ui| {
                    field(ui, &mut d.duplicate[0], f64::MIN, "Duplicate offset x (document options)");
                    field(ui, &mut d.duplicate[1], f64::MIN, "Duplicate offset y (document options)");
                });
                ui.end_row();
                ui.label("CMYK profile:");
                crate::color_ui::profile_combo(ui, state.dialogs.as_mut(), "CMYK profile (document options)", &mut d.color.cmyk_profile, None, 200.0);
                ui.end_row();
                ui.label("Rendering intent:");
                crate::color_ui::intent_combo(ui, "Rendering intent (document options)", &mut d.color.intent, 200.0);
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Document Options").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Document Options").clicked();
            });
        });
    if ok {
        apply_document_options(state, &d);
    } else if open && !cancel {
        state.document_options = Some(d);
    }
}

//! Windows of the Layout menu: Go to Page and Page Background.

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

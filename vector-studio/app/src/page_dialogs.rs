//! Windows of the Layout menu: Go to Page.

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
                go = ui.button("Go").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    state.go_to_page = (open && !go && !cancel).then_some(number);
    if go {
        let _ = state.doc.set_active_page(number.clamp(1, count) - 1);
    }
}

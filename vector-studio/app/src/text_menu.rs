//! Text menu windows: Change Case and Text Statistics.

use vs_core::text_tools::{Case, TextStats, change_case};
use vs_core::{ObjectId, Shape};

use crate::app::AppState;

/// Text objects Change Case works on: the text being typed into, else the selected text.
pub fn case_targets(state: &AppState) -> Vec<ObjectId> {
    if let Some(edit) = &state.tools.text {
        return vec![edit.id];
    }
    state.selected_objects().filter(|o| matches!(o.shape, Shape::Text(_))).map(|o| o.id).collect()
}

/// Rewrite the words of `ids` in `case`.
fn apply_case(state: &mut AppState, ids: &[ObjectId], case: Case) {
    let page = state.doc.active_page_mut();
    for id in ids {
        if let Some(obj) = page.object_mut(*id)
            && let Shape::Text(t) = &mut obj.shape
        {
            t.content = change_case(&t.content, case);
        }
    }
    // Some letters change length in another case (ß → SS): keep the caret in the text.
    if let Some(edit) = &mut state.tools.text
        && let Some(obj) = state.doc.active_page().object(edit.id)
        && let Shape::Text(t) = &obj.shape
    {
        edit.caret = edit.caret.min(t.content.chars().count());
    }
}

pub fn open_change_case(state: &mut AppState) {
    state.change_case = Some(Case::default());
}

/// The Change Case window: the five cases, then OK.
pub fn change_case_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut case) = state.change_case.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Change Case")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for c in Case::ALL {
                ui.radio_value(&mut case, c, c.label());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Change Case").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Change Case").clicked();
            });
        });
    if ok {
        let ids = case_targets(state);
        apply_case(state, &ids, case);
    } else if open && !cancel {
        state.change_case = Some(case);
    }
}

/// What Text Statistics counts: the selected text, else every text object in the drawing.
fn statistics(state: &AppState) -> (&'static str, TextStats) {
    let selected = TextStats::of(state.selected_objects());
    if selected.objects > 0 {
        ("Selected text", selected)
    } else {
        ("Whole drawing", TextStats::of_document(&state.doc))
    }
}

/// The Text Statistics window.
pub fn statistics_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_text_statistics {
        return;
    }
    let (scope, stats) = statistics(state);
    let mut open = true;
    let mut close = false;
    egui::Window::new("Text Statistics")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(scope).strong());
            egui::Grid::new("text_statistics").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for (name, value) in [
                    ("Text objects", stats.objects),
                    ("Lines", stats.lines),
                    ("Words", stats.words),
                    ("Characters", stats.characters),
                ] {
                    ui.label(name);
                    let v = ui.label(value.to_string());
                    crate::a11y::name(&v, &format!("{name} (text statistics)"));
                    ui.end_row();
                }
                ui.label("Fonts");
                let fonts = if stats.fonts.is_empty() { "None".to_owned() } else { stats.fonts.join("\n") };
                let v = ui.label(fonts);
                crate::a11y::name(&v, "Fonts (text statistics)");
                ui.end_row();
            });
            ui.add_space(8.0);
            close = crate::a11y::window_button(ui, true, "Close", "Text Statistics").clicked();
        });
    if !open || close {
        state.show_text_statistics = false;
    }
}

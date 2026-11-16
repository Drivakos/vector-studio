//! Text menu windows: Change Case, Text Statistics, Columns, Bullets and Numbering, and
//! Drop Cap.

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
            for (name, value) in [
                ("Text objects", stats.objects),
                ("Lines", stats.lines),
                ("Words", stats.words),
                ("Characters", stats.characters),
            ] {
                ui.label(format!("{name}: {value}"));
            }
            let fonts = if stats.fonts.is_empty() { "None".to_owned() } else { stats.fonts.join(", ") };
            ui.label(format!("Fonts: {fonts}"));
            ui.add_space(8.0);
            close = crate::a11y::window_button(ui, true, "Close", "Text Statistics").clicked();
        });
    if !open || close {
        state.show_text_statistics = false;
    }
}

/// Paragraph text Columns works on: the selected unlocked paragraph text.
pub fn column_targets(state: &AppState) -> Vec<ObjectId> {
    state
        .selected_objects()
        .filter(|o| !o.locked && matches!(&o.shape, Shape::Text(t) if t.is_paragraph()))
        .map(|o| o.id)
        .collect()
}

/// The open Columns window: number of columns and gutter, the gutter in display units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnsDialog {
    columns: u32,
    gutter: f64,
}

pub fn open_columns(state: &mut AppState) {
    let unit = state.doc.display_unit;
    let first = state.selected_objects().find_map(|o| match &o.shape {
        Shape::Text(t) if t.is_paragraph() => Some((t.columns, t.gutter)),
        _ => None,
    });
    let (columns, gutter) = first.unwrap_or((1, 0.0));
    // A new split starts with a 5 mm gutter.
    let gutter = if columns > 1 { gutter } else { vs_core::Unit::Millimeters.to_points(5.0) };
    state.columns = Some(ColumnsDialog { columns: columns.max(1), gutter: unit.from_points(gutter) });
}

/// The Columns window: how many columns the selected paragraph text has, and the space
/// between them. OK applies.
pub fn columns_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.columns.take() else { return };
    let unit = state.doc.display_unit;
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Columns")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("columns_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Number of columns:");
                crate::a11y::add(ui, egui::DragValue::new(&mut d.columns).range(1..=12).speed(0.1), "Number of columns");
                ui.end_row();
                ui.label("Gutter:");
                let gutter = egui::DragValue::new(&mut d.gutter)
                    .range(0.0..=f64::MAX)
                    .speed(unit.step() / 2.0)
                    .max_decimals(unit.decimals())
                    .suffix(format!(" {}", unit.suffix()));
                ui.add_enabled_ui(d.columns > 1, |ui| crate::a11y::add(ui, gutter, "Gutter"));
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Columns").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Columns").clicked();
            });
        });
    if ok {
        let ids = column_targets(state);
        let gutter = unit.to_points(d.gutter);
        let page = state.doc.active_page_mut();
        for id in ids {
            if let Some(obj) = page.object_mut(id)
                && let Shape::Text(t) = &mut obj.shape
            {
                t.columns = d.columns;
                t.gutter = if d.columns > 1 { gutter } else { 0.0 };
            }
        }
    } else if open && !cancel {
        state.columns = Some(d);
    }
}

/// Text Bullets and Numbering works on: the text being typed into, else the selected
/// unlocked text, but not text fitted to a path.
pub fn list_targets(state: &AppState) -> Vec<ObjectId> {
    let page = state.doc.active_page();
    let ids: Vec<ObjectId> = match &state.tools.text {
        Some(edit) => vec![edit.id],
        None => state.selection.clone(),
    };
    ids.into_iter()
        .filter(|id| page.object(*id).is_some_and(|o| !o.locked && matches!(&o.shape, Shape::Text(t) if t.on_path.is_none())))
        .collect()
}

pub fn open_bullets(state: &mut AppState) {
    let page = state.doc.active_page();
    let current = list_targets(state).first().and_then(|id| match &page.object(*id)?.shape {
        Shape::Text(t) => t.list,
        _ => None,
    });
    state.bullets = Some(current);
}

/// The Bullets and Numbering window: none, bullets or numbers, then OK.
pub fn bullets_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut list) = state.bullets.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Bullets and Numbering")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.radio_value(&mut list, None, "None");
            for style in [vs_core::ListStyle::Bullet, vs_core::ListStyle::Numbered] {
                ui.radio_value(&mut list, Some(style), style.label());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Bullets and Numbering").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Bullets and Numbering").clicked();
            });
        });
    if ok {
        let ids = list_targets(state);
        let page = state.doc.active_page_mut();
        for id in ids {
            if let Some(obj) = page.object_mut(id)
                && let Shape::Text(t) = &mut obj.shape
            {
                t.list = list;
            }
        }
    } else if open && !cancel {
        state.bullets = Some(list);
    }
}

/// The open Drop Cap window: whether to use one, and how many lines it drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropCapDialog {
    on: bool,
    lines: u32,
}

pub fn open_drop_cap(state: &mut AppState) {
    let current = state.selected_objects().find_map(|o| match &o.shape {
        Shape::Text(t) if t.is_paragraph() => Some(t.drop_cap),
        _ => None,
    });
    let lines = current.unwrap_or(0);
    state.drop_cap = Some(DropCapDialog { on: lines >= 2, lines: if lines >= 2 { lines } else { 3 } });
}

/// The Drop Cap window: on or off and the lines dropped, for the selected paragraph
/// text. OK applies. A drop cap replaces bullets or numbers.
pub fn drop_cap_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.drop_cap.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Drop Cap")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            crate::a11y::add(ui, egui::Checkbox::new(&mut d.on, "Use drop cap"), "Use drop cap");
            ui.add_enabled_ui(d.on, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Lines dropped:");
                    crate::a11y::add(ui, egui::DragValue::new(&mut d.lines).range(2..=10).speed(0.1), "Lines dropped");
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Drop Cap").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Drop Cap").clicked();
            });
        });
    if ok {
        let ids = column_targets(state);
        let page = state.doc.active_page_mut();
        for id in ids {
            if let Some(obj) = page.object_mut(id)
                && let Shape::Text(t) = &mut obj.shape
            {
                t.drop_cap = if d.on { d.lines } else { 0 };
                if d.on {
                    t.list = None;
                }
            }
        }
    } else if open && !cancel {
        state.drop_cap = Some(d);
    }
}

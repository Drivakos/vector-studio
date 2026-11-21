//! Drop Shadow tool: drag from an object to cast its shadow there; the property bar sets
//! the shadow's offset, opacity, feather and colour, or clears it.

use egui_material_icons::icons::*;
use vs_core::kurbo::{Point, Vec2};
use vs_core::{DropShadow, ObjectId};

use crate::app::AppState;
use crate::chrome::{icon_button, number_row};
use crate::theme;

/// A press with the tool: select the object there. Returns it if a drag from it may cast
/// its shadow (it is unlocked).
pub fn press(state: &mut AppState, p: Point, tolerance: f64) -> Option<ObjectId> {
    let page = state.doc.active_page();
    let Some(hit) = page.hit_test(p, tolerance) else {
        state.selection.clear();
        return None;
    };
    if !state.selection.contains(&hit) {
        state.selection = vec![hit];
        state.marquee_selection = None;
    }
    page.object(hit).filter(|o| !o.locked).map(|o| o.id)
}

/// Cast `object`'s shadow `offset` away from it, keeping its other settings (or the
/// property bar's, for a new shadow).
pub fn cast(state: &mut AppState, object: ObjectId, offset: Vec2) {
    let current = state.doc.active_page().object(object).and_then(|o| o.shadow.clone());
    let shadow = DropShadow { offset, ..current.unwrap_or_else(|| state.tools.settings.shadow.clone()) };
    state.doc.set_shadow(&[object], &shadow);
}

pub fn property_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = state.doc.with_shadow(&state.selection);
    let page = state.doc.active_page();
    let current = targets.first().and_then(|id| page.object(*id)).and_then(|o| o.shadow.clone());
    let mut edit = current.clone().unwrap_or_else(|| state.tools.settings.shadow.clone());
    let unit = state.doc.display_unit;
    let mut changed = false;
    if current.is_some() {
        let range = unit.from_points(-5000.0)..=unit.from_points(5000.0);
        let (mut x, mut y) = (unit.from_points(edit.offset.x), unit.from_points(edit.offset.y));
        if number_row(ui, Some(ICON_EAST), "Shadow offset X", &mut x, unit.suffix(), range.clone(), unit.step(), unit.decimals()) {
            edit.offset.x = unit.to_points(x);
            changed = true;
        }
        if number_row(ui, Some(ICON_SOUTH), "Shadow offset Y", &mut y, unit.suffix(), range, unit.step(), unit.decimals()) {
            edit.offset.y = unit.to_points(y);
            changed = true;
        }
        theme::vsep(ui);
    }
    let mut opacity = edit.opacity * 100.0;
    if number_row(ui, Some(ICON_OPACITY), "Shadow opacity", &mut opacity, "%", 0.0..=100.0, 1.0, 0) {
        edit.opacity = opacity / 100.0;
        changed = true;
    }
    let mut feather = unit.from_points(edit.feather);
    if number_row(ui, Some(ICON_BLUR_ON), "Shadow feathering", &mut feather, unit.suffix(), 0.0..=unit.from_points(DropShadow::MAX_FEATHER), unit.step(), unit.decimals()) {
        edit.feather = unit.to_points(feather).clamp(0.0, DropShadow::MAX_FEATHER);
        changed = true;
    }
    changed |= crate::fill_ui::color_button(ui, &mut edit.color, "Shadow color");
    if changed {
        if targets.is_empty() {
            state.tools.settings.shadow = edit;
        } else {
            state.doc.set_shadow(&targets, &edit);
        }
    }
    theme::vsep(ui);
    if icon_button(ui, ICON_CLOSE, "Clear shadow", !targets.is_empty()).clicked() {
        state.doc.clear_shadow(&targets);
    }
    theme::vsep(ui);
    ui.label(egui::RichText::new(crate::tools::hint(state)).color(theme::TEXT_DIM));
}

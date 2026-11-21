//! Artistic Media tool: drag to draw a stroke whose width follows a preset shape or a
//! calligraphic nib. With curves selected, the property bar (or Effects > Artistic Media)
//! puts the stroke on them; the Shape tool still edits the curve under the stroke.

use egui_material_icons::icons::*;
use vs_core::kurbo::{BezPath, Shape as _};
use vs_core::{ArtisticMedia, MediaKind, MediaPreset, ObjectId, Rgba, Shape, Style};

use crate::app::AppState;
use crate::chrome::{number_row, segmented};
use crate::theme;

/// What new strokes take.
#[derive(Clone, Debug)]
pub struct MediaSettings {
    pub media: ArtisticMedia,
    /// How much a drawn stroke is smoothed, 0 to 100.
    pub smoothing: f64,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self { media: ArtisticMedia::preset(MediaPreset::TaperBoth, ArtisticMedia::WIDTH), smoothing: 50.0 }
    }
}

/// The selected objects a stroke can go on.
pub fn targets(state: &AppState) -> Vec<ObjectId> {
    state.selected_objects().filter(|o| o.can_take_media()).map(|o| o.id).collect()
}

/// Put `media` on each of `ids`. An object that had no stroke yet is filled with its
/// outline colour (black without one) and loses its outline, so the stroke shows as
/// the line did.
pub fn apply(state: &mut AppState, ids: &[ObjectId], media: &ArtisticMedia) {
    state.doc.edit_effects(ids, |o| {
        if !o.can_take_media() {
            return;
        }
        if o.effects.media.is_none() {
            let color = o.style.outline.as_ref().map_or(Rgba::BLACK, |l| l.color);
            o.style.fill = Some(color.into());
            o.style.outline = None;
        }
        o.effects.media = Some(media.clone());
    });
}

/// Effects > Artistic Media: the tool, with the current stroke on the selected curves
/// that have none.
pub fn start(state: &mut AppState) {
    state.tool = crate::toolbox::Tool::ArtisticMedia;
    let bare: Vec<ObjectId> = state.selected_objects().filter(|o| o.can_take_media() && o.effects.media.is_none()).map(|o| o.id).collect();
    let media = state.tools.media.media.clone();
    apply(state, &bare, &media);
}

/// A drawn line, smoothed into curves: `screen_px` is the length of one screen pixel in
/// points.
pub fn finish(state: &mut AppState, line: BezPath, closed: bool, screen_px: f64) {
    let accuracy = screen_px * (0.5 + state.tools.media.smoothing / 100.0 * 4.5);
    let options = vs_core::kurbo::simplify::SimplifyOptions::default();
    let mut path = vs_core::kurbo::simplify::simplify_bezpath(line, accuracy, &options);
    if closed {
        path.close_path();
    }
    // A click without a drag draws nothing.
    if path.bounding_box().size().max_side() < 2.0 * screen_px {
        return;
    }
    let id = state.doc.add_object(Shape::Curve { path }, Style::filled(Rgba::BLACK));
    let media = state.tools.media.media.clone();
    state.doc.edit_effects(&[id], |o| o.effects.media = Some(media.clone()));
    state.selection = vec![id];
}

pub fn property_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state);
    let current = targets.iter().filter_map(|id| state.doc.active_page().object(*id)).find_map(|o| o.effects.media.clone());
    let mut edit = current.unwrap_or_else(|| state.tools.media.media.clone());
    let mut changed = false;

    let calligraphic = matches!(edit.kind, MediaKind::Calligraphic { .. });
    let segments = [(ICON_GESTURE, "Preset stroke", !calligraphic), (ICON_EDIT, "Calligraphic stroke", calligraphic)];
    match segmented(ui, "media_kind", &segments) {
        Some(0) if calligraphic => {
            edit.kind = MediaKind::Preset { preset: MediaPreset::TaperBoth };
            changed = true;
        }
        Some(1) if !calligraphic => {
            edit.kind = MediaKind::Calligraphic { angle: ArtisticMedia::ANGLE };
            changed = true;
        }
        _ => {}
    }
    theme::vsep(ui);
    match &mut edit.kind {
        MediaKind::Preset { preset } => {
            let was = *preset;
            let combo = egui::ComboBox::from_id_salt("media_preset").selected_text(preset.label()).width(140.0).show_ui(ui, |ui| {
                for p in MediaPreset::ALL {
                    ui.selectable_value(preset, p, p.label());
                }
            });
            crate::a11y::name(&combo.response, "Stroke preset");
            changed |= *preset != was;
        }
        MediaKind::Calligraphic { angle } => {
            changed |= number_row(ui, Some(ICON_ROTATE_RIGHT), "Nib angle", angle, "°", -180.0..=180.0, 1.0, 0);
        }
    }
    let unit = state.doc.display_unit;
    let mut width = unit.from_points(edit.width);
    if number_row(ui, Some(ICON_LINE_WEIGHT), "Stroke width", &mut width, unit.suffix(), unit.from_points(0.1)..=unit.from_points(ArtisticMedia::MAX_WIDTH), unit.step(), unit.decimals()) {
        edit.width = unit.to_points(width).clamp(0.1, ArtisticMedia::MAX_WIDTH);
        changed = true;
    }
    theme::vsep(ui);
    number_row(ui, Some(ICON_GRAIN), "Freehand smoothing", &mut state.tools.media.smoothing, "%", 0.0..=100.0, 1.0, 0);
    if changed {
        state.tools.media.media = edit.clone();
        apply(state, &targets, &edit);
    }
    theme::vsep(ui);
    ui.label(egui::RichText::new(crate::tools::hint(state)).color(theme::TEXT_DIM));
}

pub fn hint(state: &AppState) -> String {
    if targets(state).is_empty() {
        "Drag to draw a stroke · or select curves and pick a stroke to put it on them".into()
    } else {
        "Pick a stroke to put it on the selected curves · drag to draw a new one".into()
    }
}

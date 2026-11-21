//! Interactive Fill (G), Transparency, Color Eyedropper and Attributes Eyedropper.

use egui_material_icons::icons::*;
use vs_core::kurbo::{Point, Rect};
use vs_core::{Fill, Gradient, GradientKind, GradientStop, ObjectId, Pattern, Rgba, Shape, Style};

use crate::app::AppState;
use crate::chrome::segmented;
use crate::theme;
use crate::toolbox::Tool;

/// What the eyedroppers picked up, and whether the next click applies it.
#[derive(Clone, Debug, Default)]
pub struct Eyedropper {
    pub color: Option<Rgba>,
    pub style: Option<Style>,
    pub applying: bool,
}

/// The unlocked, non-group object under `p`.
fn paintable_at(state: &AppState, p: Point, tolerance: f64) -> Option<ObjectId> {
    let page = state.doc.active_page();
    page.hit_test(p, tolerance).filter(|id| page.object(*id).is_some_and(|o| !o.locked))
}

/// The fountain fill of the single selected object, with its transform.
pub fn selected_gradient(state: &AppState) -> Option<(ObjectId, Gradient, vs_core::kurbo::Affine)> {
    let [id] = state.selection.as_slice() else { return None };
    let obj = state.doc.active_page().object(*id)?;
    match &obj.style.fill {
        Some(Fill::Gradient(g)) if !obj.locked && !matches!(obj.shape, Shape::Group { .. }) => Some((*id, g.clone(), obj.transform)),
        _ => None,
    }
}

/// Fill `id` with a fountain from `start` to `end` (page space). An existing fountain keeps
/// its kind and colours; otherwise it runs from the uniform colour (or black) to white.
pub fn drag_fountain(state: &mut AppState, id: ObjectId, start: Point, end: Point) {
    let Some(obj) = state.doc.active_page_mut().object_mut(id) else { return };
    if obj.locked || matches!(obj.shape, Shape::Group { .. }) {
        return;
    }
    let inv = obj.transform.inverse();
    let (start, end) = (inv * start, inv * end);
    let gradient = match obj.style.fill.take() {
        Some(Fill::Gradient(g)) => Gradient { start, end, ..g },
        other => {
            let from = other.as_ref().map_or(Rgba::BLACK, Fill::color);
            Gradient {
                kind: GradientKind::Linear,
                start,
                end,
                stops: vec![GradientStop { offset: 0.0, color: from }, GradientStop { offset: 1.0, color: Rgba::WHITE }],
            }
        }
    };
    obj.style.fill = Some(Fill::Gradient(gradient));
}

/// Move the start (`end` false) or end handle of `id`'s fountain fill to `p` (page space).
pub fn move_fountain_handle(state: &mut AppState, id: ObjectId, end: bool, p: Point) {
    let Some(obj) = state.doc.active_page_mut().object_mut(id) else { return };
    let local = obj.transform.inverse() * p;
    if let Some(Fill::Gradient(g)) = &mut obj.style.fill {
        if end {
            g.end = local;
        } else {
            g.start = local;
        }
    }
}

/// A handle of a pattern fill's first tile (Interactive Fill tool).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileHandle {
    /// The corner the tiles start from: moves them.
    Origin,
    /// The far end of the tile's top side: its width and turn.
    Across,
    /// The far end of the tile's left side: its height and slant.
    Down,
}

/// The pattern fill of the single selected object, with its transform.
pub fn selected_pattern(state: &AppState) -> Option<(ObjectId, Pattern, vs_core::kurbo::Affine)> {
    let [id] = state.selection.as_slice() else { return None };
    let obj = state.doc.active_page().object(*id)?;
    match &obj.style.fill {
        Some(Fill::Pattern(p)) if !obj.locked && !matches!(obj.shape, Shape::Group { .. }) => Some((*id, p.clone(), obj.transform)),
        _ => None,
    }
}

/// Where the handles of `p`'s first tile are on the page; `t` is its object's transform.
pub fn tile_handles(p: &Pattern, t: vs_core::kurbo::Affine) -> [(TileHandle, Point); 3] {
    let place = t * p.transform;
    [(TileHandle::Origin, place * Point::ZERO), (TileHandle::Across, place * Point::new(1.0, 0.0)), (TileHandle::Down, place * Point::new(0.0, 1.0))]
}

/// Move a handle of `id`'s pattern tile to `at` (page space). A tile would fold flat is
/// left as it is.
pub fn move_tile_handle(state: &mut AppState, id: ObjectId, handle: TileHandle, at: Point) {
    let Some(obj) = state.doc.active_page_mut().object_mut(id) else { return };
    let local = obj.transform.inverse() * at;
    let Some(Fill::Pattern(p)) = &mut obj.style.fill else { return };
    let [a, b, c, d, e, f] = p.transform.as_coeffs();
    let origin = Point::new(e, f);
    let side = local - origin;
    let moved = match handle {
        TileHandle::Origin => vs_core::kurbo::Affine::new([a, b, c, d, local.x, local.y]),
        TileHandle::Across => vs_core::kurbo::Affine::new([side.x, side.y, c, d, e, f]),
        TileHandle::Down => vs_core::kurbo::Affine::new([a, b, side.x, side.y, e, f]),
    };
    if moved.determinant().abs() > 1e-6 {
        p.transform = moved;
    }
}

/// Select the object under `p` (Interactive Fill and Transparency tools); a click on
/// nothing clears the selection. Returns the object.
pub fn select_at(state: &mut AppState, p: Point, tolerance: f64) -> Option<ObjectId> {
    let hit = paintable_at(state, p, tolerance);
    state.selection = hit.into_iter().collect();
    state.marquee_selection = None;
    hit
}

/// A click with either eyedropper.
pub fn press_eyedropper(state: &mut AppState, p: Point, tolerance: f64) {
    let page = state.doc.active_page();
    let paper = state.doc.background.map_or(Rgba::WHITE, |b| b.color);
    let eye = &mut state.tools.eyedropper;
    match (state.tool, eye.applying) {
        (Tool::ColorEyedropper, false) => {
            let picked = page.paint_at(p, tolerance).map(|h| h.color).or_else(|| page.bounds().contains(p).then_some(paper));
            if picked.is_some() {
                eye.color = picked;
                eye.applying = true;
            }
        }
        (Tool::ColorEyedropper, true) => {
            let Some(color) = eye.color else { return };
            let target = page.paint_at(p, tolerance).map(|h| (h.id, h.outline)).or_else(|| page.hit_test(p, tolerance).map(|id| (id, false)));
            if let Some((id, outline)) = target {
                state.doc.restyle_objects(&[id], |s| match (&mut s.outline, outline) {
                    (Some(o), true) => o.color = color,
                    _ => s.fill = Some(Fill::Solid(color)),
                });
            }
        }
        (_, false) => {
            if let Some(obj) = page.hit_test(p, tolerance).and_then(|id| page.object(id)) {
                eye.style = Some(obj.shown_style().clone());
                eye.applying = true;
            }
        }
        (_, true) => {
            let Some(style) = eye.style.clone() else { return };
            if let Some(id) = page.hit_test(p, tolerance) {
                state.doc.restyle_objects_framed(&[id], |s, frame| *s = placed(&style, frame));
            }
        }
    }
}

/// `style` for an object with local frame `frame`: a fountain fill is placed across it,
/// a pattern starts from its corner.
fn placed(style: &Style, frame: Rect) -> Style {
    Style { fill: style.fill.as_ref().map(|f| f.placed_on(frame)), ..style.clone() }
}

/// Property bar for the paint tools.
pub fn property_bar(ui: &mut egui::Ui, state: &mut AppState) {
    match state.tool {
        Tool::InteractiveFill | Tool::Transparency if !state.has_selection() => {
            ui.label(egui::RichText::new(crate::tools::hint(state)).color(theme::TEXT_DIM));
        }
        Tool::InteractiveFill => crate::fill_ui::fill_controls(ui, state, "(property bar)"),
        Tool::Transparency => {
            ui.label(ICON_OPACITY.rich_text().size(16.0).color(theme::TEXT_DIM));
            crate::fill_ui::transparency_controls(ui, state, "Transparency (property bar)");
        }
        _ => {
            let color = state.tool == Tool::ColorEyedropper;
            let (select, apply) = if color { ("Select color", "Apply color") } else { ("Select attributes", "Apply attributes") };
            let applying = state.tools.eyedropper.applying;
            if let Some(i) = segmented(ui, "eyedropper_mode", &[(ICON_COLORIZE, select, !applying), (ICON_FORMAT_COLOR_FILL, apply, applying)]) {
                state.tools.eyedropper.applying = i == 1;
            }
            if color && let Some(c) = state.tools.eyedropper.color {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a));
                ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, theme::TEXT_DIM), egui::StrokeKind::Inside);
                crate::a11y::button(&resp, true, "Picked color");
                resp.on_hover_text(format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b));
            }
            theme::vsep(ui);
            ui.label(egui::RichText::new(crate::tools::hint(state)).color(theme::TEXT_DIM));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_fountain_fills_are_placed_on_the_new_object() {
        let mut style = Style::filled(Rgba::BLACK);
        style.fill = Some(Fill::Gradient(Gradient::across(GradientKind::Radial, Rect::new(0.0, 0.0, 10.0, 10.0), Rgba::BLACK, Rgba::WHITE)));
        let s = placed(&style, Rect::new(100.0, 100.0, 140.0, 120.0));
        let Some(Fill::Gradient(g)) = s.fill else { panic!("fountain") };
        assert_eq!(g.start, Point::new(120.0, 110.0));
        assert_eq!(g.end, Point::new(140.0, 110.0));
    }
}

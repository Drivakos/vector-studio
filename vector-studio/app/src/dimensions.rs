//! Parallel and Horizontal or Vertical Dimension tools: drag between two points, then
//! click where the dimension line goes. The result is a group: extension and dimension
//! lines, arrowheads, and the distance in the display unit as text over the line.

use vs_core::dimension::{Dimension, DimensionKind, dimension};
use vs_core::kurbo::{Affine, Point, Vec2};
use vs_core::{Outline, Rgba, Shape, Style, Text};

use crate::app::AppState;
use crate::toolbox::Tool;

/// Size of the distance text, in points.
const LABEL_SIZE: f64 = 8.0;
/// Space between the dimension line and its text, in points.
const LABEL_GAP: f64 = 2.0;

pub fn kind(tool: Tool) -> Option<DimensionKind> {
    match tool {
        Tool::ParallelDimension => Some(DimensionKind::Parallel),
        Tool::HorizontalVerticalDimension => Some(DimensionKind::HorizontalVertical),
        _ => None,
    }
}

/// The distance as the label shows it, e.g. "42.33 mm".
pub fn label(state: &AppState, length: f64) -> String {
    let unit = state.doc.display_unit;
    format!("{:.*} {}", unit.decimals(), unit.from_points(length), unit.suffix())
}

/// Add the dimension of `a` → `b` with its line through `c`, grouped and selected.
pub fn finish(state: &mut AppState, kind: DimensionKind, a: Point, b: Point, c: Point) {
    let Some(d) = dimension(kind, a, b, c) else { return };
    add(state, &d);
}

fn add(state: &mut AppState, d: &Dimension) {
    let text = label(state, d.length);
    let doc = &mut state.doc;
    let lines = doc.add_object(Shape::Curve { path: d.lines.clone() }, Style::outlined(Outline::new(Rgba::BLACK, 0.5)));
    let arrows = doc.add_object(Shape::Curve { path: d.arrows.clone() }, Style::filled(Rgba::BLACK));
    let mut t = Text::new(text);
    t.size = LABEL_SIZE;
    t.align = vs_core::TextAlign::Center;
    let label = doc.add_object(Shape::Text(t), Style::new_text());
    if let Some(obj) = doc.active_page_mut().object_mut(label) {
        // Its bottom centre, a little above the line, turned along it.
        let r = obj.world_bounds();
        let anchor = Vec2::new(r.center().x, r.y1);
        obj.transform = Affine::translate(d.label_at.to_vec2()) * Affine::rotate(d.label_angle) * Affine::translate(Vec2::new(0.0, -LABEL_GAP) - anchor);
    }
    let group = doc.group_objects(&[lines, arrows, label]);
    if let Some(id) = group
        && let Some(obj) = doc.active_page_mut().object_mut(id)
    {
        obj.name = Some("Dimension".to_owned());
    }
    state.selection = group.into_iter().collect();
    state.marquee_selection = None;
}

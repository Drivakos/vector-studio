//! The dimension tools and the 3-Point Callout tool.
//!
//! Parallel and Horizontal or Vertical: drag between two points, then click where the
//! dimension line goes. Segment: click a segment, then click where the line goes. Angular:
//! drag from the vertex along one arm, click the other arm, then click where the arc goes.
//! Each makes a group: extension and dimension lines, arrowheads, and the measure as text.
//! 3-Point Callout: drag the leader to its bend, click where the text goes, then type.

use vs_core::dimension::{DimensionKind, angular_dimension, dimension};
use vs_core::kurbo::{Affine, BezPath, Point, Vec2};
use vs_core::{Outline, Rgba, Shape, Style, Text, TextAlign};

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
        // The segment's ends, measured along it.
        Tool::SegmentDimension => Some(DimensionKind::Parallel),
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
    let text = label(state, d.length);
    add(state, Parts { lines: d.lines, arrows: d.arrows, text, label_at: d.label_at, label_angle: d.label_angle });
}

/// An angle as the label shows it, e.g. "45.0°".
pub fn degrees(angle: f64) -> String {
    format!("{:.1}°", angle.to_degrees())
}

/// Add the angle at `vertex` between the arms through `a` and `b`, its arc through `at`,
/// grouped and selected.
pub fn finish_angular(state: &mut AppState, vertex: Point, a: Point, b: Point, at: Point) {
    let Some(d) = angular_dimension(vertex, a, b, at) else { return };
    add(state, Parts { lines: d.lines, arrows: d.arrows, text: degrees(d.angle), label_at: d.label_at, label_angle: d.label_angle });
}

/// What a dimension group is made of.
struct Parts {
    lines: BezPath,
    arrows: BezPath,
    text: String,
    label_at: Point,
    label_angle: f64,
}

fn add(state: &mut AppState, d: Parts) {
    let doc = &mut state.doc;
    let lines = doc.add_object(Shape::Curve { path: d.lines }, Style::outlined(Outline::new(Rgba::BLACK, 0.5)));
    let arrows = doc.add_object(Shape::Curve { path: d.arrows }, Style::filled(Rgba::BLACK));
    let mut t = Text::new(d.text);
    t.size = LABEL_SIZE;
    t.align = TextAlign::Center;
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

/// Space between the end of a callout's leader and its text, in points.
const CALLOUT_GAP: f64 = 3.0;

/// Add a callout: a leader from `a` bending at `b` to `c`, and new text just past `c`
/// that typing goes into.
pub fn finish_callout(state: &mut AppState, a: Point, b: Point, c: Point) {
    crate::text_tool::finish(state);
    let mut leader = BezPath::new();
    leader.move_to(a);
    leader.line_to(b);
    if (c - b).hypot() > 1e-9 {
        leader.line_to(c);
    }
    let id = state.doc.add_object(Shape::Curve { path: leader }, Style::outlined(Outline::new(Rgba::BLACK, 0.5)));
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.name = Some("Callout".to_owned());
    }

    // The text reads away from the bend: after `c` to the right, before it to the left.
    let mut text = state.tools.settings.text.clone();
    text.content.clear();
    let left = c.x < b.x;
    text.align = if left { TextAlign::Right } else { TextAlign::Left };
    // The baseline a little under `c`, so the letters sit about level with it.
    let baseline = c + Vec2::new(if left { -CALLOUT_GAP } else { CALLOUT_GAP }, text.size * 0.35);
    let text_id = state.doc.add_object(Shape::Text(text), Style::new_text());
    if let Some(obj) = state.doc.active_page_mut().object_mut(text_id) {
        obj.transform = Affine::translate(baseline.to_vec2());
    }
    state.selection = vec![text_id];
    state.marquee_selection = None;
    state.tools.text = Some(crate::text_tool::TextEdit { id: text_id, caret: 0 });
}

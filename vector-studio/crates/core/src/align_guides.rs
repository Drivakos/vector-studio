//! Alignment guides and dynamic guides: temporary lines that show, and snap to, how what
//! is being drawn or moved lines up with the other objects on the page.
//!
//! - **Alignment guides** line up an edge or centre of the moving bounds (or a point) with
//!   an edge or centre of another object, and run across both.
//! - **Dynamic guides** run from the corners and centres of other objects at 0°, 45°, 90°
//!   and 135°, and pull the pointer onto the nearest one.

use kurbo::{Point, Rect, Vec2};

use crate::document::{ObjectId, Page};

/// A guide line to draw, in page points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideLine {
    pub from: Point,
    pub to: Point,
}

/// Matches closer than this count as lined up after the shift.
const EXACT: f64 = 1e-6;

fn xs(r: Rect) -> [f64; 3] {
    [r.x0, r.center().x, r.x1]
}

fn ys(r: Rect) -> [f64; 3] {
    [r.y0, r.center().y, r.y1]
}

/// Bounds of the shown objects of `page` not in `exclude`.
fn targets(page: &Page, exclude: &[ObjectId]) -> Vec<Rect> {
    page.shown_objects().filter(|o| !exclude.contains(&o.id)).map(|o| o.world_bounds()).collect()
}

/// The smallest shift (within `threshold`) that lines one of `moving` up with one of the
/// targets' values on the same axis.
fn best_shift(moving: [f64; 3], targets: &[Rect], along: fn(Rect) -> [f64; 3], threshold: f64) -> f64 {
    targets
        .iter()
        .flat_map(|t| along(*t))
        .flat_map(|t| moving.map(|v| t - v))
        .filter(|d| d.abs() <= threshold)
        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        .unwrap_or(0.0)
}

/// Alignment of `bounds` (a rectangle, or a point as an empty one) with the other objects
/// of `page`: the shift that lines it up, per axis, within `threshold`, and the guide lines
/// to draw once shifted. Each line runs along the shared edge or centre, across both.
pub fn alignment(page: &Page, bounds: Rect, exclude: &[ObjectId], threshold: f64) -> (Vec2, Vec<GuideLine>) {
    let targets = targets(page, exclude);
    let shift = Vec2::new(best_shift(xs(bounds), &targets, xs, threshold), best_shift(ys(bounds), &targets, ys, threshold));
    let b = bounds + shift;
    let mut lines = Vec::new();
    for t in &targets {
        for x in xs(*t) {
            if xs(b).iter().any(|v| (v - x).abs() < EXACT) {
                lines.push(GuideLine { from: Point::new(x, b.y0.min(t.y0)), to: Point::new(x, b.y1.max(t.y1)) });
            }
        }
        for y in ys(*t) {
            if ys(b).iter().any(|v| (v - y).abs() < EXACT) {
                lines.push(GuideLine { from: Point::new(b.x0.min(t.x0), y), to: Point::new(b.x1.max(t.x1), y) });
            }
        }
    }
    lines.dedup();
    (shift, lines)
}

/// Directions of the dynamic guides: 0°, 45°, 90° and 135°.
const DIRECTIONS: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    Vec2::new(0.0, 1.0),
    Vec2::new(-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
];

/// The dynamic guide nearest `p` within `threshold`: `p` moved onto it, and the guide from
/// its corner or centre through the moved point. Corners and centres of the shown objects
/// not in `exclude` are where guides start; `None` when no guide is that close.
pub fn dynamic_guide(page: &Page, p: Point, exclude: &[ObjectId], threshold: f64) -> Option<(Point, GuideLine)> {
    let mut best: Option<(f64, Point, GuideLine)> = None;
    for r in targets(page, exclude) {
        let anchors = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x0, r.y1), Point::new(r.x1, r.y1), r.center()];
        for a in anchors {
            let v = p - a;
            if v.hypot() < threshold {
                // On the anchor itself: that is object snapping's job, not a guide's.
                continue;
            }
            for d in DIRECTIONS {
                let along = v.dot(d);
                let off = (v - d * along).hypot();
                if off <= threshold && best.as_ref().is_none_or(|(o, ..)| off < *o) {
                    let on = a + d * along;
                    best = Some((off, on, GuideLine { from: a, to: on }));
                }
            }
        }
    }
    best.map(|(_, on, line)| (on, line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape, Style};

    fn doc_with(rect: Rect) -> (Document, ObjectId) {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, Style::default());
        (doc, id)
    }

    #[test]
    fn moving_bounds_line_up_and_get_a_guide_across_both() {
        let (doc, other) = doc_with(Rect::new(100.0, 100.0, 200.0, 150.0));
        // Left edge 3 pt right of the other's left edge, far below it.
        let (shift, lines) = alignment(doc.active_page(), Rect::new(103.0, 300.0, 143.0, 340.0), &[], 5.0);
        assert_eq!(shift, Vec2::new(-3.0, 0.0));
        assert_eq!(lines, vec![GuideLine { from: Point::new(100.0, 100.0), to: Point::new(100.0, 340.0) }]);
        // Excluded objects are not lined up with.
        let (shift, lines) = alignment(doc.active_page(), Rect::new(103.0, 300.0, 143.0, 340.0), &[other], 5.0);
        assert_eq!(shift, Vec2::ZERO);
        assert!(lines.is_empty());
    }

    #[test]
    fn a_point_lines_up_with_centres() {
        let (doc, _) = doc_with(Rect::new(100.0, 100.0, 200.0, 150.0));
        let p = Point::new(400.0, 126.0);
        let (shift, lines) = alignment(doc.active_page(), Rect::from_points(p, p), &[], 2.0);
        assert_eq!(shift, Vec2::new(0.0, -1.0));
        assert_eq!(lines, vec![GuideLine { from: Point::new(100.0, 125.0), to: Point::new(400.0, 125.0) }]);
    }

    #[test]
    fn dynamic_guides_pull_onto_diagonals_from_corners() {
        let (doc, _) = doc_with(Rect::new(100.0, 100.0, 200.0, 150.0));
        // 1 pt off the 45° line from the bottom-right corner (200, 150).
        let (on, line) = dynamic_guide(doc.active_page(), Point::new(251.0, 200.0), &[], 3.0).expect("a guide");
        assert!((on - Point::new(250.5, 200.5)).hypot() < 1e-9, "{on:?}");
        assert_eq!(line.from, Point::new(200.0, 150.0));
        assert!(dynamic_guide(doc.active_page(), Point::new(260.0, 230.0), &[], 3.0).is_none(), "too far from any guide");
    }
}

//! Shape-tool nodes of shapes that are not curves: the ellipse's pie/arc node and the
//! nodes of polygons and stars, which move all their sides together.

use std::f64::consts::{FRAC_PI_2, TAU};

use kurbo::{Affine, Point, Rect};

use crate::document::{ArcKind, EllipseArc, Shape};
use crate::geometry::ellipse_point;

/// Where the single node of a whole ellipse sits, in degrees (12 o'clock).
pub const ELLIPSE_NODE_ANGLE: f64 = 90.0;

/// A node of an analytic shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// The start (`end: false`) or end of a pie or arc. A whole ellipse has one node, its end.
    Ellipse { end: bool },
    /// Outer vertex `i` of a polygon or star.
    Outer(u32),
    /// Inner vertex `i` of a star.
    Inner(u32),
}

/// The shape's nodes in local space; empty for shapes edited another way.
pub fn controls(shape: &Shape) -> Vec<(Control, Point)> {
    match shape {
        Shape::Ellipse { rect, arc: None } => vec![(Control::Ellipse { end: true }, ellipse_point(*rect, ELLIPSE_NODE_ANGLE))],
        Shape::Ellipse { rect, arc: Some(arc) } => vec![
            (Control::Ellipse { end: false }, ellipse_point(*rect, arc.start)),
            (Control::Ellipse { end: true }, ellipse_point(*rect, arc.end)),
        ],
        Shape::Polygon { rect, sides } => {
            let n = (*sides).max(3);
            (0..n).map(|i| (Control::Outer(i), vertex(*rect, n, i, 1.0))).collect()
        }
        Shape::Star { rect, points, inner_ratio } => {
            let n = (*points).max(3);
            (0..n)
                .flat_map(|i| {
                    [
                        (Control::Outer(i), vertex(*rect, 2 * n, 2 * i, 1.0)),
                        (Control::Inner(i), vertex(*rect, 2 * n, 2 * i + 1, inner_ratio.clamp(0.01, 1.0))),
                    ]
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Vertex `i` of `count` on the ellipse inscribed in `rect`, at `k` of the radius;
/// matches `geometry::polygon_path`.
fn vertex(rect: Rect, count: u32, i: u32, k: f64) -> Point {
    let c = rect.center();
    let a = -FRAC_PI_2 + TAU * f64::from(i) / f64::from(count);
    Point::new(c.x + rect.width() / 2.0 * k * a.cos(), c.y + rect.height() / 2.0 * k * a.sin())
}

/// `original` with `control` dragged to `to` (local space). Also returns a rotation to
/// append to the object's transform (polygons and stars turn with their nodes).
/// `constrain` snaps ellipse angles to 15° and keeps polygons and stars from turning.
pub fn drag(original: &Shape, control: Control, to: Point, constrain: bool) -> Option<(Shape, Affine)> {
    match (original, control) {
        (Shape::Ellipse { rect, arc }, Control::Ellipse { end }) => {
            let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
            if rx < 1e-9 || ry < 1e-9 {
                return None;
            }
            let c = rect.center();
            let (nx, ny) = ((to.x - c.x) / rx, (c.y - to.y) / ry);
            let mut angle = ny.atan2(nx).to_degrees();
            if constrain {
                angle = (angle / 15.0).round() * 15.0;
            }
            // Inside the ellipse makes a pie, outside an arc.
            let kind = if nx * nx + ny * ny < 1.0 { ArcKind::Pie } else { ArcKind::Arc };
            let mut new = arc.unwrap_or(EllipseArc { kind, start: ELLIPSE_NODE_ANGLE, end: ELLIPSE_NODE_ANGLE });
            new.kind = kind;
            if end {
                new.end = angle;
            } else {
                new.start = angle;
            }
            let gap = (new.end - new.start).rem_euclid(360.0);
            let whole = !(0.5..=359.5).contains(&gap);
            Some((Shape::Ellipse { rect: *rect, arc: (!whole).then_some(new) }, Affine::IDENTITY))
        }
        (Shape::Polygon { rect, .. } | Shape::Star { rect, .. }, Control::Outer(_)) => {
            let from = controls(original).into_iter().find(|(c, _)| *c == control)?.1;
            let c = rect.center();
            let (v0, v) = (from - c, to - c);
            if v0.hypot() < 1e-9 || v.hypot() < 1e-9 {
                return None;
            }
            let s = (v.hypot() / v0.hypot()).max(0.01);
            let rect = Rect::from_center_size(c, rect.size() * s);
            let shape = match original {
                Shape::Polygon { sides, .. } => Shape::Polygon { rect, sides: *sides },
                Shape::Star { points, inner_ratio, .. } => Shape::Star { rect, points: *points, inner_ratio: *inner_ratio },
                _ => unreachable!(),
            };
            let turn = if constrain { 0.0 } else { v.atan2() - v0.atan2() };
            Some((shape, Affine::rotate_about(turn, c)))
        }
        (Shape::Star { rect, points, inner_ratio }, Control::Inner(_)) => {
            let from = controls(original).into_iter().find(|(c, _)| *c == control)?.1;
            let c = rect.center();
            let v0 = from - c;
            if v0.hypot2() < 1e-18 {
                return None;
            }
            // How far along its own spoke the pointer is.
            let ratio = (inner_ratio * (to - c).dot(v0) / v0.hypot2()).clamp(0.02, 1.0);
            Some((Shape::Star { rect: *rect, points: *points, inner_ratio: ratio }, Affine::IDENTITY))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Shape as _;

    use super::*;

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    #[test]
    fn ellipse_node_dragged_inside_makes_a_pie_and_outside_an_arc() {
        let ellipse = Shape::Ellipse { rect: Rect::new(0.0, 0.0, 100.0, 100.0), arc: None };
        let nodes = controls(&ellipse);
        assert_eq!(nodes.len(), 1);
        assert!(near(nodes[0].1, Point::new(50.0, 0.0)), "the node sits at the top");

        // Inside, towards 3 o'clock.
        let (pie, turn) = drag(&ellipse, Control::Ellipse { end: true }, Point::new(80.0, 50.0), false).unwrap();
        assert_eq!(turn, Affine::IDENTITY);
        let Shape::Ellipse { arc: Some(arc), .. } = pie else { panic!("{pie:?}") };
        assert_eq!((arc.kind, arc.start), (ArcKind::Pie, 90.0));
        assert!(arc.end.abs() < 1e-9);
        assert!(pie.is_closed());
        assert_eq!(controls(&pie).len(), 2);

        let (arc_shape, _) = drag(&ellipse, Control::Ellipse { end: true }, Point::new(150.0, 50.0), false).unwrap();
        assert!(matches!(arc_shape, Shape::Ellipse { arc: Some(EllipseArc { kind: ArcKind::Arc, .. }), .. }));
        assert!(!arc_shape.is_closed(), "an arc is open");
        let b = arc_shape.to_path().bounding_box();
        assert!((b.x0 - 0.0).abs() < 0.05 && (b.y1 - 100.0).abs() < 0.05, "three quarters remain: {b:?}");
    }

    #[test]
    fn ellipse_drag_snaps_and_closes_back_to_a_whole_ellipse() {
        let ellipse = Shape::Ellipse { rect: Rect::new(0.0, 0.0, 100.0, 100.0), arc: None };
        let (pie, _) = drag(&ellipse, Control::Ellipse { end: true }, Point::new(90.0, 45.0), true).unwrap();
        let Shape::Ellipse { arc: Some(arc), .. } = pie else { panic!() };
        assert_eq!(arc.end, 0.0, "Cmd snaps to 15°");

        let (whole, _) = drag(&pie, Control::Ellipse { end: true }, Point::new(50.0, 10.0), false).unwrap();
        assert!(matches!(whole, Shape::Ellipse { arc: None, .. }), "dropping the end on the start closes it");
    }

    #[test]
    fn polygon_node_scales_and_turns_every_side() {
        let poly = Shape::Polygon { rect: Rect::new(0.0, 0.0, 100.0, 100.0), sides: 6 };
        assert_eq!(controls(&poly).len(), 6);
        // Drag the top vertex (50, 0) to twice the radius, a quarter turn clockwise.
        let to = Point::new(150.0, 50.0);
        let (shape, turn) = drag(&poly, Control::Outer(0), to, false).unwrap();
        let Shape::Polygon { rect, sides: 6 } = shape else { panic!() };
        assert!((rect.width() - 200.0).abs() < 1e-9);
        let node = turn * controls(&shape)[0].1;
        assert!(near(node, to), "the dragged node follows the pointer: {node:?}");

        let (_, still) = drag(&poly, Control::Outer(0), to, true).unwrap();
        assert_eq!(still, Affine::rotate_about(0.0, Point::new(50.0, 50.0)), "Cmd: no turn");
    }

    #[test]
    fn star_inner_node_sets_the_sharpness() {
        let star = Shape::Star { rect: Rect::new(0.0, 0.0, 100.0, 100.0), points: 5, inner_ratio: 0.5 };
        let nodes = controls(&star);
        assert_eq!(nodes.len(), 10);
        let (control, at) = nodes[1];
        assert_eq!(control, Control::Inner(0));
        let c = Point::new(50.0, 50.0);
        let (shape, _) = drag(&star, control, c + (at - c) * 0.5, false).unwrap();
        let Shape::Star { inner_ratio, .. } = shape else { panic!() };
        assert!((inner_ratio - 0.25).abs() < 1e-9, "{inner_ratio}");
    }
}

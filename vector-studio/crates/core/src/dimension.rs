//! Dimension lines: the distance between two points drawn as a line with arrowheads,
//! extension lines back to the points, and where its label goes.

use kurbo::{BezPath, Point, Vec2};

use crate::construct::side_distance;

/// How a dimension measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionKind {
    /// Along the line between the points, at any angle.
    Parallel,
    /// The horizontal or the vertical distance, whichever way the line is pulled.
    HorizontalVertical,
}

/// Arrowhead length and half-width, in points.
const ARROW_LENGTH: f64 = 5.0;
const ARROW_HALF_WIDTH: f64 = 1.75;
/// How far extension lines run past the dimension line, in points.
const OVERSHOOT: f64 = 2.0;

/// A dimension ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    /// The measured distance, in points.
    pub length: f64,
    /// Ends of the dimension line.
    pub start: Point,
    pub end: Point,
    /// Extension lines and the dimension line, to stroke.
    pub lines: BezPath,
    /// The two arrowheads, to fill.
    pub arrows: BezPath,
    /// The middle of the dimension line, where the label sits.
    pub label_at: Point,
    /// Angle of the label in radians, turned so it never reads upside down.
    pub label_angle: f64,
}

/// The dimension of `a` → `b`, its line pulled through `c`. `None` when the points are
/// too close to measure.
pub fn dimension(kind: DimensionKind, a: Point, b: Point, c: Point) -> Option<Dimension> {
    let (start, end) = match kind {
        DimensionKind::Parallel => {
            let d = b - a;
            if d.hypot() < 1e-9 {
                return None;
            }
            // Right of a → b is +normal, as side_distance counts it.
            let normal = Vec2::new(-d.y, d.x) / d.hypot();
            let offset = normal * side_distance(a, b, c);
            (a + offset, b + offset)
        }
        DimensionKind::HorizontalVertical => {
            let outside = |v: f64, lo: f64, hi: f64| (lo.min(hi) - v).max(v - lo.max(hi)).max(0.0);
            // Pulled further above or below the points than beside them: horizontal.
            if outside(c.y, a.y, b.y) >= outside(c.x, a.x, b.x) {
                (Point::new(a.x, c.y), Point::new(b.x, c.y))
            } else {
                (Point::new(c.x, a.y), Point::new(c.x, b.y))
            }
        }
    };
    let length = (end - start).hypot();
    if length < 1e-9 {
        return None;
    }
    let along = (end - start) / length;

    let mut lines = BezPath::new();
    for (point, foot) in [(a, start), (b, end)] {
        let reach = foot - point;
        if reach.hypot() > 1e-9 {
            lines.move_to(point);
            lines.line_to(foot + reach / reach.hypot() * OVERSHOOT);
        }
    }
    lines.move_to(start);
    lines.line_to(end);

    let mut arrows = BezPath::new();
    let size = ARROW_LENGTH.min(length / 3.0);
    let half = ARROW_HALF_WIDTH * size / ARROW_LENGTH;
    let side = Vec2::new(-along.y, along.x);
    for (tip, inward) in [(start, along), (end, -along)] {
        let base = tip + inward * size;
        arrows.move_to(tip);
        arrows.line_to(base + side * half);
        arrows.line_to(base - side * half);
        arrows.close_path();
    }

    let mut label_angle = along.atan2();
    if label_angle > std::f64::consts::FRAC_PI_2 + 1e-9 || label_angle <= -std::f64::consts::FRAC_PI_2 + 1e-9 {
        label_angle -= std::f64::consts::PI.copysign(label_angle);
    }
    Some(Dimension { length, start, end, lines, arrows, label_at: start.midpoint(end), label_angle })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    #[test]
    fn parallel_dimension_runs_alongside_at_the_pulled_distance() {
        let d = dimension(DimensionKind::Parallel, Point::new(0.0, 0.0), Point::new(30.0, 40.0), Point::new(-8.0, 6.0)).unwrap();
        assert!((d.length - 50.0).abs() < 1e-9);
        // (−8, 6) is 10 pt from the line, on the left of a → b.
        assert!((d.start - Point::new(-8.0, 6.0)).hypot() < 1e-9, "{:?}", d.start);
        assert!((d.end - Point::new(22.0, 46.0)).hypot() < 1e-9, "{:?}", d.end);
        assert!((d.label_angle - (40f64).atan2(30.0)).abs() < 1e-9);
        assert_eq!(d.arrows.elements().iter().filter(|e| matches!(e, kurbo::PathEl::ClosePath)).count(), 2);
    }

    #[test]
    fn horizontal_or_vertical_follows_where_the_line_is_pulled() {
        let (a, b) = (Point::new(10.0, 10.0), Point::new(50.0, 30.0));
        let h = dimension(DimensionKind::HorizontalVertical, a, b, Point::new(30.0, -10.0)).unwrap();
        assert_eq!((h.start, h.end, h.length), (Point::new(10.0, -10.0), Point::new(50.0, -10.0), 40.0));
        let v = dimension(DimensionKind::HorizontalVertical, a, b, Point::new(80.0, 20.0)).unwrap();
        assert_eq!((v.start, v.end, v.length), (Point::new(80.0, 10.0), Point::new(80.0, 30.0), 20.0));
        assert!((v.label_angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Extension lines from each point run past the dimension line.
        let b_box = v.lines.bounding_box();
        assert!((b_box.x1 - 82.0).abs() < 1e-9 && b_box.x0 == 10.0, "{b_box:?}");
    }

    #[test]
    fn labels_never_read_upside_down() {
        let d = dimension(DimensionKind::Parallel, Point::new(40.0, 0.0), Point::new(0.0, 0.0), Point::new(20.0, -5.0)).unwrap();
        assert!(d.label_angle.abs() < 1e-9, "{}", d.label_angle);
        assert!(dimension(DimensionKind::Parallel, Point::new(1.0, 1.0), Point::new(1.0, 1.0), Point::ZERO).is_none());
        assert!(dimension(DimensionKind::HorizontalVertical, Point::new(0.0, 0.0), Point::new(0.0, 20.0), Point::new(5.0, -30.0)).is_none(), "no horizontal distance");
    }
}

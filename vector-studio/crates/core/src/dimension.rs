//! Dimension lines: the distance between two points drawn as a line with arrowheads,
//! extension lines back to the points, and where its label goes.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use kurbo::{BezPath, ParamCurve as _, ParamCurveNearest as _, Point, Shape as _, Vec2};

use crate::construct::side_distance;
use crate::document::{Page, Shape};

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

    let size = ARROW_LENGTH.min(length / 3.0);
    let arrows = arrowheads(&[(start, along), (end, -along)], size);
    Some(Dimension { length, start, end, lines, arrows, label_at: start.midpoint(end), label_angle: upright(along.atan2()) })
}

/// Arrowheads `size` long, each at a tip pointing away from `inward`.
fn arrowheads(tips: &[(Point, Vec2)], size: f64) -> BezPath {
    let half = ARROW_HALF_WIDTH * size / ARROW_LENGTH;
    let mut arrows = BezPath::new();
    for (tip, inward) in tips {
        let side = Vec2::new(-inward.y, inward.x);
        let base = *tip + *inward * size;
        arrows.move_to(*tip);
        arrows.line_to(base + side * half);
        arrows.line_to(base - side * half);
        arrows.close_path();
    }
    arrows
}

/// `angle` turned half round when text along it would read upside down.
fn upright(mut angle: f64) -> f64 {
    if angle > FRAC_PI_2 + 1e-9 || angle <= -FRAC_PI_2 + 1e-9 {
        angle -= PI.copysign(angle);
    }
    angle
}

/// An angle between two arms, measured along an arc round their vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct AngularDimension {
    /// The measured angle, in radians (0 to 2π).
    pub angle: f64,
    /// The arms out from the vertex to just past the arc, and the arc, to stroke.
    pub lines: BezPath,
    /// The two arrowheads at the arc's ends, to fill.
    pub arrows: BezPath,
    /// The middle of the arc, where the label sits.
    pub label_at: Point,
    /// Angle of the label in radians: along the arc, never upside down.
    pub label_angle: f64,
}

/// The angle at `vertex` from the arm through `a` to the arm through `b`, its arc through
/// `at`. The arc runs on the side of the arms where `at` is, so the angle can be over
/// 180°. `None` when an arm or the arc has no length.
pub fn angular_dimension(vertex: Point, a: Point, b: Point, at: Point) -> Option<AngularDimension> {
    let radius = (at - vertex).hypot();
    if (a - vertex).hypot() < 1e-9 || (b - vertex).hypot() < 1e-9 || radius < 1e-9 {
        return None;
    }
    let (from, to) = ((a - vertex).atan2(), (b - vertex).atan2());
    let between = (to - from).rem_euclid(TAU);
    if between < 1e-9 || TAU - between < 1e-9 {
        return None;
    }
    // Positive sweep from arm a to arm b, unless `at` is outside that, then the other way.
    let sweep = if ((at - vertex).atan2() - from).rem_euclid(TAU) <= between { between } else { between - TAU };
    let mut lines = BezPath::new();
    for angle in [from, to] {
        lines.move_to(vertex);
        lines.line_to(vertex + Vec2::from_angle(angle) * (radius + OVERSHOOT));
    }
    // `path_elements` starts the arc with its own move-to.
    lines.extend(kurbo::Arc::new(vertex, (radius, radius), from, sweep, 0.0).path_elements(0.01));

    // Each arrow points away from the arc along its tangent at that end.
    let tangent = |angle: f64| Vec2::from_angle(angle + FRAC_PI_2) * sweep.signum();
    let size = ARROW_LENGTH.min(radius * sweep.abs() / 3.0);
    let start = vertex + Vec2::from_angle(from) * radius;
    let end = vertex + Vec2::from_angle(from + sweep) * radius;
    let arrows = arrowheads(&[(start, tangent(from)), (end, -tangent(from + sweep))], size);

    let middle = from + sweep / 2.0;
    Some(AngularDimension {
        angle: sweep.abs(),
        lines,
        arrows,
        label_at: vertex + Vec2::from_angle(middle) * radius,
        label_angle: upright(middle + FRAC_PI_2),
    })
}

/// The ends of the straight or curved segment of the topmost object on `page` that
/// passes within `tolerance` of `p`: what the Segment Dimension tool measures.
pub fn segment_at(page: &Page, p: Point, tolerance: f64) -> Option<(Point, Point)> {
    page.objects
        .iter()
        .rev()
        .filter(|o| page.is_selectable(o) && !matches!(o.shape, Shape::Text(_) | Shape::Image(_)))
        .find_map(|o| {
            o.world_path()
                .segments()
                .map(|s| (s.nearest(p, 1e-6).distance_sq, s))
                .filter(|(d, _)| *d <= tolerance * tolerance)
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .map(|(_, s)| (s.start(), s.end()))
                .filter(|(a, b)| (*b - *a).hypot() > 1e-9)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn angular_dimension_measures_the_side_the_arc_is_on() {
        let v = Point::new(0.0, 0.0);
        let (a, b) = (Point::new(50.0, 0.0), Point::new(0.0, 50.0));
        let d = angular_dimension(v, a, b, Point::new(20.0, 20.0)).unwrap();
        assert!((d.angle.to_degrees() - 90.0).abs() < 1e-9, "{}", d.angle.to_degrees());
        // The arc is 20√2 from the vertex and its middle on the bisector.
        let r = 800f64.sqrt();
        assert!((d.label_at - Point::new(20.0, 20.0)).hypot() < 1e-9, "{:?}", d.label_at);
        assert!((d.label_angle - (-PI / 4.0)).abs() < 1e-9, "along the arc, upright: {}", d.label_angle);
        let b_box = d.lines.bounding_box();
        assert!((b_box.x1 - (r + OVERSHOOT)).abs() < 1e-6 && (b_box.y1 - (r + OVERSHOOT)).abs() < 1e-6, "arms reach past the arc: {b_box:?}");
        assert_eq!(d.lines.elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))).count(), 3, "two arms and the arc");
        assert_eq!(d.arrows.elements().iter().filter(|e| matches!(e, kurbo::PathEl::ClosePath)).count(), 2);

        // The arc pulled to the far side: the reflex angle.
        let reflex = angular_dimension(v, a, b, Point::new(-20.0, -20.0)).unwrap();
        assert!((reflex.angle.to_degrees() - 270.0).abs() < 1e-9, "{}", reflex.angle.to_degrees());
        assert!((reflex.label_at - Point::new(-20.0, -20.0)).hypot() < 1e-9);
        assert!(angular_dimension(v, v, b, a).is_none(), "an arm with no length");
        assert!(angular_dimension(v, a, a, b).is_none(), "the same arm twice");
    }

    #[test]
    fn segment_at_finds_the_nearest_segment_of_the_top_object() {
        let mut doc = crate::Document::default();
        doc.add_object(Shape::Rectangle { rect: kurbo::Rect::new(0.0, 0.0, 100.0, 50.0), corner_radius: 0.0 }, crate::Style::default());
        let page = doc.active_page();
        let (a, b) = segment_at(page, Point::new(50.0, 51.0), 2.0).unwrap();
        assert_eq!((a.y, b.y, (b.x - a.x).abs()), (50.0, 50.0, 100.0), "the bottom side");
        let (a, b) = segment_at(page, Point::new(99.0, 25.0), 2.0).unwrap();
        assert_eq!((a.x, b.x, (b.y - a.y).abs()), (100.0, 100.0, 50.0), "the right side");
        assert!(segment_at(page, Point::new(50.0, 25.0), 2.0).is_none(), "inside, away from the sides");
    }
}

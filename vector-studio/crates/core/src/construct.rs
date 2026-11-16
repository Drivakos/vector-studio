//! Paths the drawing tools build from a few points: B-splines, 3-point curves, spirals,
//! common shapes and graph paper.

use std::f64::consts::TAU;

use kurbo::{BezPath, Point, Rect, Vec2};

/// A cubic B-spline with `points` as its control points, as Bézier curves. Open splines
/// start and end on their first and last points; closed ones loop through every point's
/// neighbourhood. Fewer than two points make an empty path.
pub fn bspline(points: &[Point], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    if points.len() < 2 {
        return path;
    }
    let padded: Vec<Point> = if closed {
        let n = points.len();
        (0..n + 3).map(|i| points[(i + n - 1) % n]).collect()
    } else {
        // The end points repeated three times pin the curve to them.
        let (first, last) = (points[0], points[points.len() - 1]);
        [first, first].into_iter().chain(points.iter().copied()).chain([last, last]).collect()
    };
    let lerp = |a: Point, b: Point, w: f64| a + (b - a) * w;
    for (k, w) in padded.windows(4).enumerate() {
        let (a, b, c, d) = (w[0], w[1], w[2], w[3]);
        let start = lerp(lerp(a, b, 2.0 / 3.0), lerp(b, c, 1.0 / 3.0), 0.5);
        let end = lerp(lerp(b, c, 2.0 / 3.0), lerp(c, d, 1.0 / 3.0), 0.5);
        if k == 0 {
            path.move_to(start);
        }
        path.curve_to(lerp(b, c, 1.0 / 3.0), lerp(b, c, 2.0 / 3.0), end);
    }
    if closed {
        path.close_path();
    }
    path
}

/// A curve from `a` to `b` that passes through `through` halfway along, as one cubic.
pub fn three_point_curve(a: Point, b: Point, through: Point) -> BezPath {
    // The quadratic whose midpoint is `through`, raised to a cubic.
    let q = through + (through - a.midpoint(b));
    let mut path = BezPath::new();
    path.move_to(a);
    path.curve_to(a + (q - a) * (2.0 / 3.0), b + (q - b) * (2.0 / 3.0), b);
    path
}

/// The signed distance of `p` from the line through `a` and `b` (positive to the right
/// of a → b on screen, where y points down).
pub fn side_distance(a: Point, b: Point, p: Point) -> f64 {
    let d = b - a;
    let len = d.hypot();
    if len < 1e-12 {
        return 0.0;
    }
    d.cross(p - a) / len
}

/// Segments per turn of a spiral.
const SPIRAL_STEPS: usize = 8;

/// An even (Archimedean) spiral from the centre of `rect` out to its edge, `turns` times
/// round, clockwise on screen.
pub fn spiral(rect: Rect, turns: u32) -> BezPath {
    let turns = turns.max(1);
    let c = rect.center();
    let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
    let end = TAU * f64::from(turns);
    // Radius grows evenly with the angle, from 0 at the centre to 1 at the edge.
    let at = |t: f64| {
        let r = t / end;
        Point::new(c.x + rx * r * t.cos(), c.y + ry * r * t.sin())
    };
    let tangent = |t: f64| {
        let (r, dr) = (t / end, 1.0 / end);
        Vec2::new(rx * (dr * t.cos() - r * t.sin()), ry * (dr * t.sin() + r * t.cos()))
    };
    let steps = SPIRAL_STEPS * turns as usize;
    let dt = end / steps as f64;
    let mut path = BezPath::new();
    path.move_to(at(0.0));
    for i in 0..steps {
        let (t0, t1) = (i as f64 * dt, (i + 1) as f64 * dt);
        path.curve_to(at(t0) + tangent(t0) * (dt / 3.0), at(t1) - tangent(t1) * (dt / 3.0), at(t1));
    }
    path
}

/// A ready-made outline of the Common Shapes tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommonShape {
    #[default]
    Triangle,
    RightTriangle,
    Parallelogram,
    Trapezoid,
    Diamond,
    Hexagon,
    Cross,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    Heart,
    Callout,
}

impl CommonShape {
    pub const ALL: [CommonShape; 13] = [
        CommonShape::Triangle,
        CommonShape::RightTriangle,
        CommonShape::Parallelogram,
        CommonShape::Trapezoid,
        CommonShape::Diamond,
        CommonShape::Hexagon,
        CommonShape::Cross,
        CommonShape::ArrowRight,
        CommonShape::ArrowLeft,
        CommonShape::ArrowUp,
        CommonShape::ArrowDown,
        CommonShape::Heart,
        CommonShape::Callout,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CommonShape::Triangle => "Triangle",
            CommonShape::RightTriangle => "Right triangle",
            CommonShape::Parallelogram => "Parallelogram",
            CommonShape::Trapezoid => "Trapezoid",
            CommonShape::Diamond => "Diamond",
            CommonShape::Hexagon => "Hexagon",
            CommonShape::Cross => "Cross",
            CommonShape::ArrowRight => "Arrow right",
            CommonShape::ArrowLeft => "Arrow left",
            CommonShape::ArrowUp => "Arrow up",
            CommonShape::ArrowDown => "Arrow down",
            CommonShape::Heart => "Heart",
            CommonShape::Callout => "Callout",
        }
    }

    /// Corners in a unit square (0..1 across and down), for the straight-sided shapes.
    fn corners(self) -> &'static [(f64, f64)] {
        match self {
            CommonShape::Triangle => &[(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)],
            CommonShape::RightTriangle => &[(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            CommonShape::Parallelogram => &[(0.25, 0.0), (1.0, 0.0), (0.75, 1.0), (0.0, 1.0)],
            CommonShape::Trapezoid => &[(0.25, 0.0), (0.75, 0.0), (1.0, 1.0), (0.0, 1.0)],
            CommonShape::Diamond => &[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)],
            CommonShape::Hexagon => &[(0.25, 0.0), (0.75, 0.0), (1.0, 0.5), (0.75, 1.0), (0.25, 1.0), (0.0, 0.5)],
            CommonShape::Cross => &[
                (1.0 / 3.0, 0.0), (2.0 / 3.0, 0.0), (2.0 / 3.0, 1.0 / 3.0), (1.0, 1.0 / 3.0), (1.0, 2.0 / 3.0), (2.0 / 3.0, 2.0 / 3.0),
                (2.0 / 3.0, 1.0), (1.0 / 3.0, 1.0), (1.0 / 3.0, 2.0 / 3.0), (0.0, 2.0 / 3.0), (0.0, 1.0 / 3.0), (1.0 / 3.0, 1.0 / 3.0),
            ],
            CommonShape::ArrowRight | CommonShape::ArrowLeft | CommonShape::ArrowUp | CommonShape::ArrowDown => {
                &[(0.0, 0.25), (0.6, 0.25), (0.6, 0.0), (1.0, 0.5), (0.6, 1.0), (0.6, 0.75), (0.0, 0.75)]
            }
            CommonShape::Callout => &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.75), (0.45, 0.75), (0.2, 1.0), (0.25, 0.75), (0.0, 0.75)],
            CommonShape::Heart => &[],
        }
    }
}

/// The outline of `shape` filling `rect`, closed.
pub fn common_shape(shape: CommonShape, rect: Rect) -> BezPath {
    let at = |x: f64, y: f64| Point::new(rect.x0 + x * rect.width(), rect.y0 + y * rect.height());
    let mut path = BezPath::new();
    if shape == CommonShape::Heart {
        // Two lobes meeting in a dip at the top and a point at the bottom.
        path.move_to(at(0.5, 0.25));
        path.curve_to(at(0.5, 0.08), at(0.4, 0.0), at(0.25, 0.0));
        path.curve_to(at(0.1, 0.0), at(0.0, 0.12), at(0.0, 0.3));
        path.curve_to(at(0.0, 0.6), at(0.4, 0.75), at(0.5, 1.0));
        path.curve_to(at(0.6, 0.75), at(1.0, 0.6), at(1.0, 0.3));
        path.curve_to(at(1.0, 0.12), at(0.9, 0.0), at(0.75, 0.0));
        path.curve_to(at(0.6, 0.0), at(0.5, 0.08), at(0.5, 0.25));
        path.close_path();
        return path;
    }
    // Arrows are drawn pointing right, then turned within the unit square.
    let turn = |(x, y): (f64, f64)| match shape {
        CommonShape::ArrowLeft => (1.0 - x, y),
        CommonShape::ArrowDown => (y, x),
        CommonShape::ArrowUp => (y, 1.0 - x),
        _ => (x, y),
    };
    for (i, c) in shape.corners().iter().enumerate() {
        let (x, y) = turn(*c);
        if i == 0 {
            path.move_to(at(x, y));
        } else {
            path.line_to(at(x, y));
        }
    }
    path.close_path();
    path
}

/// Graph Paper: `rect` split into `columns` × `rows` equal cells, row by row from the
/// top left.
pub fn graph_paper(rect: Rect, columns: u32, rows: u32) -> Vec<Rect> {
    let (columns, rows) = (columns.max(1), rows.max(1));
    let (w, h) = (rect.width() / f64::from(columns), rect.height() / f64::from(rows));
    (0..rows)
        .flat_map(|r| (0..columns).map(move |c| (c, r)))
        .map(|(c, r)| {
            let x0 = rect.x0 + f64::from(c) * w;
            let y0 = rect.y0 + f64::from(r) * h;
            Rect::new(x0, y0, x0 + w, y0 + h)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_shapes_fill_their_box_and_arrows_point_their_way() {
        let rect = Rect::new(10.0, 20.0, 110.0, 70.0);
        for shape in CommonShape::ALL {
            let path = common_shape(shape, rect);
            let b = path.bounding_box();
            assert!((b.x0 - 10.0).abs() < 0.5 && (b.x1 - 110.0).abs() < 0.5 && (b.y0 - 20.0).abs() < 0.5 && (b.y1 - 70.0).abs() < 0.5, "{shape:?}: {b:?}");
            assert!(matches!(path.elements().last(), Some(kurbo::PathEl::ClosePath)), "{shape:?} is closed");
        }
        let tip = |shape| {
            let path = common_shape(shape, Rect::new(0.0, 0.0, 10.0, 10.0));
            path.elements().iter().filter_map(|e| e.end_point()).find(|p| [(10.0, 5.0), (0.0, 5.0), (5.0, 0.0), (5.0, 10.0)].contains(&(p.x, p.y)))
        };
        assert_eq!(tip(CommonShape::ArrowRight), Some(Point::new(10.0, 5.0)));
        assert_eq!(tip(CommonShape::ArrowLeft), Some(Point::new(0.0, 5.0)));
        assert_eq!(tip(CommonShape::ArrowUp), Some(Point::new(5.0, 0.0)));
        assert_eq!(tip(CommonShape::ArrowDown), Some(Point::new(5.0, 10.0)));
    }

    #[test]
    fn graph_paper_cells_tile_the_rectangle() {
        let cells = graph_paper(Rect::new(10.0, 20.0, 50.0, 50.0), 4, 3);
        assert_eq!(cells.len(), 12);
        assert_eq!(cells[0], Rect::new(10.0, 20.0, 20.0, 30.0));
        assert_eq!(cells[5], Rect::new(20.0, 30.0, 30.0, 40.0), "second row, second column");
        assert_eq!(cells[11], Rect::new(40.0, 40.0, 50.0, 50.0));
        assert_eq!(graph_paper(Rect::new(0.0, 0.0, 1.0, 1.0), 0, 0).len(), 1);
    }
    use kurbo::{ParamCurve, Shape as _};

    #[test]
    fn open_bsplines_start_and_end_on_their_end_points() {
        let pts = [Point::new(0.0, 0.0), Point::new(50.0, 100.0), Point::new(100.0, 0.0)];
        let path = bspline(&pts, false);
        let segs: Vec<_> = path.segments().collect();
        assert_eq!(segs.first().unwrap().eval(0.0), pts[0]);
        assert!((segs.last().unwrap().eval(1.0) - pts[2]).hypot() < 1e-9);
        let b = path.bounding_box();
        assert!(b.y1 > 30.0 && b.y1 < 100.0, "pulled towards, not through, the middle point: {b:?}");
        assert!(bspline(&pts[..1], false).is_empty());
    }

    #[test]
    fn closed_bsplines_close_and_stay_inside_their_points() {
        let pts = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0)];
        let path = bspline(&pts, true);
        assert!(path.elements().iter().any(|e| matches!(e, kurbo::PathEl::ClosePath)));
        let b = path.bounding_box();
        assert!(b.x0 > 0.0 && b.x1 < 100.0 && b.y0 > 0.0 && b.y1 < 100.0, "{b:?}");
        let first = path.segments().next().unwrap().eval(0.0);
        let last = path.segments().last().unwrap().eval(1.0);
        assert!((first - last).hypot() < 1e-9, "the loop meets itself");
    }

    #[test]
    fn three_point_curves_pass_through_the_third_point() {
        let (a, b, c) = (Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(50.0, 40.0));
        let path = three_point_curve(a, b, c);
        let seg = path.segments().next().unwrap();
        assert_eq!(seg.eval(0.0), a);
        assert!((seg.eval(1.0) - b).hypot() < 1e-9);
        assert!((seg.eval(0.5) - c).hypot() < 1e-9);
        assert!((side_distance(a, b, c) - 40.0).abs() < 1e-9, "below the line is positive");
        assert!((side_distance(a, b, Point::new(10.0, -5.0)) + 5.0).abs() < 1e-9);
    }

    #[test]
    fn spirals_grow_from_the_centre_to_the_edge() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let path = spiral(rect, 3);
        assert_eq!(path.segments().count(), 24);
        assert_eq!(path.segments().next().unwrap().eval(0.0), rect.center());
        let end = path.segments().last().unwrap().eval(1.0);
        assert!((end - Point::new(200.0, 50.0)).hypot() < 1e-9, "ends at the right edge: {end:?}");
        let b = path.bounding_box();
        assert!(b.x0 > -1.0 && b.x1 < 201.0 && b.y0 > -1.0 && b.y1 < 101.0, "{b:?}");
    }
}

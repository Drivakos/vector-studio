//! Smooth and Roughen brushes: dabs that reshape the parts of a curve under a round brush.
//!
//! Both cut the segments under the brush into short straight pieces and move the points
//! between them; the rest of the curve keeps its own segments. Smooth eases each point
//! towards its neighbours, dab by dab; Roughen pushes the points out and in by turns, as
//! spikes, along a whole stroke at once. [`refit`] turns the short pieces back into a few
//! Bézier curves once a smoothing stroke is done.

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, ParamCurveNearest, PathEl, PathSeg, Point, Vec2};

use crate::document::{Document, Object, ObjectId, Shape};
use crate::shaping::make_curve;

/// A subpath as its segments, and whether it is closed.
struct Run {
    segs: Vec<PathSeg>,
    closed: bool,
}

fn runs(path: &BezPath) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                out.push(Run { segs: Vec::new(), closed: false });
                (start, last) = (p, p);
            }
            PathEl::LineTo(p) => {
                push(&mut out, PathSeg::Line(kurbo::Line::new(last, p)));
                last = p;
            }
            PathEl::QuadTo(a, p) => {
                push(&mut out, PathSeg::Quad(kurbo::QuadBez::new(last, a, p)));
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                push(&mut out, PathSeg::Cubic(kurbo::CubicBez::new(last, a, b, p)));
                last = p;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-9 {
                    push(&mut out, PathSeg::Line(kurbo::Line::new(last, start)));
                }
                if let Some(run) = out.last_mut() {
                    run.closed = true;
                }
                last = start;
            }
        }
    }
    out.retain(|r| !r.segs.is_empty());
    out
}

fn push(out: &mut [Run], seg: PathSeg) {
    if let Some(run) = out.last_mut() {
        run.segs.push(seg);
    }
}

fn to_path(runs: &[Run]) -> BezPath {
    let mut path = BezPath::new();
    for run in runs {
        path.move_to(run.segs[0].start());
        let n = run.segs.len();
        for (i, seg) in run.segs.iter().enumerate() {
            // A closed run's last line back to the start is the ClosePath itself.
            if run.closed && i + 1 == n && matches!(seg, PathSeg::Line(_)) {
                break;
            }
            match seg {
                PathSeg::Line(l) => path.line_to(l.p1),
                PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
                PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
            }
        }
        if run.closed {
            path.close_path();
        }
    }
    path
}

fn distance(seg: &PathSeg, p: Point) -> f64 {
    seg.nearest(p, 1e-6).distance_sq.sqrt()
}

/// `seg` as straight pieces about `spacing` long.
fn pieces(seg: &PathSeg, spacing: f64) -> Vec<PathSeg> {
    let n = (seg.arclen(1e-3) / spacing).ceil().max(1.0) as usize;
    let points: Vec<Point> = (0..=n).map(|i| seg.eval(i as f64 / n as f64)).collect();
    points.windows(2).map(|w| PathSeg::Line(kurbo::Line::new(w[0], w[1]))).collect()
}

/// Whether `seg` is one of the short pieces a dab makes: a line no longer than a piece
/// whose ends were pushed apart by a spike each way (√5 pieces), with some room.
fn is_piece(seg: &PathSeg, spacing: f64) -> bool {
    matches!(seg, PathSeg::Line(l) if (l.p1 - l.p0).hypot() <= spacing * 2.5)
}

/// `path` (in page space) after one Smooth dab at `at`, `radius` points round: each
/// point under the brush moves up to `strength` (0 to 1) of the way to its neighbours'
/// midpoint, less towards the brush's edge.
pub fn smooth(path: &BezPath, at: Point, radius: f64, strength: f64) -> BezPath {
    let spacing = smooth_spacing(radius);
    let mut runs = runs(path);
    for run in &mut runs {
        // Cut the part of each touched segment under the brush into pieces; the rest stays
        // one segment, for later dabs to cut where they reach.
        let mut segs = Vec::new();
        for seg in &run.segs {
            if distance(seg, at) < radius && !is_piece(seg, spacing) {
                let ranges = inside(seg, |p| (p - at).hypot() < radius);
                segs.extend(cut(seg, &ranges, spacing).into_iter().map(|(s, _)| s));
            } else {
                segs.push(*seg);
            }
        }
        let n = segs.len();
        let old: Vec<Point> = segs.iter().map(|s| s.start()).collect();
        let mut points = old.clone();
        let movable: Vec<bool> = (0..n)
            .map(|i| {
                let before = if i == 0 { run.closed.then(|| n - 1) } else { Some(i - 1) };
                before.is_some_and(|b| is_piece(&segs[b], spacing) && is_piece(&segs[i], spacing))
            })
            .collect();
        for i in (0..n).filter(|i| movable[*i]) {
            let d = (old[i] - at).hypot();
            if d < radius {
                let (prev, next) = (old[(i + n - 1) % n], segs[i].end());
                let pull = strength.clamp(0.0, 1.0) * (1.0 - d / radius);
                points[i] = old[i] + (prev.midpoint(next) - old[i]) * pull;
            }
        }
        move_points(&mut segs, &points, &movable, run.closed);
        run.segs = segs;
    }
    to_path(&runs)
}

/// Set the ends of the line pieces to `points` where `movable`.
fn move_points(segs: &mut [PathSeg], points: &[Point], movable: &[bool], closed: bool) {
    let n = segs.len();
    for i in 0..n {
        let end = (i + 1) % n;
        if let PathSeg::Line(l) = &mut segs[i] {
            if movable[i] {
                l.p0 = points[i];
            }
            if (end != 0 || closed) && movable[end] {
                l.p1 = points[end];
            }
        }
    }
}

/// The parameter ranges of `seg` where `within` holds, found by sampling.
fn inside(seg: &PathSeg, within: impl Fn(Point) -> bool) -> Vec<(f64, f64)> {
    let n = 256;
    let mut ranges: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<f64> = None;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        match (within(seg.eval(t)), open) {
            (true, None) => open = Some(t),
            (false, Some(a)) => {
                ranges.push((a, t));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(a) = open {
        ranges.push((a, 1.0));
    }
    ranges
}

/// `seg` cut at `ranges`: the parts outside as they are (false), the parts inside as
/// pieces about `spacing` long (true).
fn cut(seg: &PathSeg, ranges: &[(f64, f64)], spacing: f64) -> Vec<(PathSeg, bool)> {
    let mut out = Vec::new();
    let mut t = 0.0;
    for (a, b) in ranges {
        if *a > t + 1e-9 {
            out.push((seg.subsegment(t..*a), false));
        }
        out.extend(pieces(&seg.subsegment(*a..*b), spacing).into_iter().map(|p| (p, true)));
        t = *b;
    }
    if t < 1.0 - 1e-9 {
        out.push((seg.subsegment(t..1.0), false));
    }
    out
}

/// Whether `p` is within `radius` of the polyline `stroke`.
fn near_stroke(stroke: &[Point], radius: f64, p: Point) -> bool {
    match stroke {
        [] => false,
        [only] => (p - *only).hypot() < radius,
        _ => stroke.windows(2).any(|w| kurbo::Line::new(w[0], w[1]).nearest(p, 1e-9).distance_sq < radius * radius),
    }
}

/// `path` (in page space) with spikes `size` points high, about `size` apart, everywhere
/// within `radius` of the brush `stroke`. Stretches roughened before are left alone, so
/// going over them again does not pile spikes on spikes.
pub fn roughen(path: &BezPath, stroke: &[Point], radius: f64, size: f64) -> BezPath {
    let spacing = size.max(0.25);
    let mut runs = runs(path);
    for run in &mut runs {
        let mut segs = Vec::new();
        // For each piece cut now, the normal of the segment it came from.
        let mut normals: Vec<Option<Vec2>> = Vec::new();
        for seg in &run.segs {
            if is_piece(seg, spacing) {
                segs.push(*seg);
                normals.push(None);
                continue;
            }
            // Slivers too short for a spike are where an earlier stroke stopped.
            let ranges: Vec<(f64, f64)> =
                inside(seg, |p| near_stroke(stroke, radius, p)).into_iter().filter(|(a, b)| seg.subsegment(*a..*b).arclen(1e-3) >= spacing).collect();
            for (piece, fresh) in cut(seg, &ranges, spacing) {
                let normal = fresh.then(|| {
                    let d = piece.end() - piece.start();
                    Vec2::new(-d.y, d.x) / d.hypot().max(1e-12)
                });
                segs.push(piece);
                normals.push(normal);
            }
        }
        // Points between two fresh pieces spike out and in by turns.
        let n = segs.len();
        let mut points: Vec<Point> = segs.iter().map(|s| s.start()).collect();
        let mut movable = vec![false; n];
        let mut flip = 1.0;
        for i in 0..n {
            let before = if i == 0 { run.closed.then(|| n - 1) } else { Some(i - 1) };
            let (Some(b), Some(normal)) = (before, normals[i]) else {
                flip = 1.0;
                continue;
            };
            if normals[b].is_none() {
                flip = 1.0;
                continue;
            }
            points[i] += normal * size * flip;
            movable[i] = true;
            flip = -flip;
        }
        move_points(&mut segs, &points, &movable, run.closed);
        run.segs = segs;
    }
    to_path(&runs)
}

/// `path` with every stretch of short pieces fitted again with Bézier curves, within
/// `accuracy` points. Other segments stay as they are.
pub fn refit(path: &BezPath, spacing: f64, accuracy: f64) -> BezPath {
    let mut runs = runs(path);
    for run in &mut runs {
        let mut out: Vec<PathSeg> = Vec::new();
        let mut stretch: Vec<Point> = Vec::new();
        let flush = |stretch: &mut Vec<Point>, out: &mut Vec<PathSeg>| {
            if stretch.len() >= 3 {
                let fitted = kurbo::simplify::simplify_bezpath(catmull_rom(stretch), accuracy, &kurbo::simplify::SimplifyOptions::default());
                out.extend(fitted.segments());
            } else {
                out.extend(stretch.windows(2).map(|w| PathSeg::Line(kurbo::Line::new(w[0], w[1]))));
            }
            stretch.clear();
        };
        for seg in &run.segs {
            if is_piece(seg, spacing) {
                if stretch.is_empty() {
                    stretch.push(seg.start());
                }
                stretch.push(seg.end());
            } else {
                flush(&mut stretch, &mut out);
                out.push(*seg);
            }
        }
        flush(&mut stretch, &mut out);
        // A closed run must still end where it starts.
        if run.closed
            && let (Some(first), Some(last)) = (out.first().map(|s| s.start()), out.last().map(|s| s.end()))
            && (first - last).hypot() > 1e-9
        {
            out.push(PathSeg::Line(kurbo::Line::new(last, first)));
        }
        run.segs = out;
    }
    to_path(&runs)
}

/// A smooth curve through `points` (Catmull–Rom), with no corners for the fitting to
/// stop at.
fn catmull_rom(points: &[Point]) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(points[0]);
    let n = points.len();
    for i in 0..n - 1 {
        let before = points[i.saturating_sub(1)];
        let after = points[(i + 2).min(n - 1)];
        let (p, q) = (points[i], points[i + 1]);
        path.curve_to(p + (q - before) / 6.0, q - (after - p) / 6.0, q);
    }
    path
}

/// The smoothing pieces' length for a brush `radius` points round, for [`refit`].
pub fn smooth_spacing(radius: f64) -> f64 {
    (radius / 8.0).max(0.25)
}

impl Object {
    /// Whether the brushes can reshape this object: an unlocked curve or shape (not text,
    /// a bitmap or a group).
    pub fn can_brush(&self) -> bool {
        !self.locked && !matches!(self.shape, Shape::Text(_) | Shape::Image(_) | Shape::Group { .. })
    }
}

impl Document {
    /// One Smooth dab on every object among `ids` that the brushes can reshape and the
    /// brush reaches; shapes become curves. Returns whether anything changed.
    pub fn smooth_at(&mut self, ids: &[ObjectId], at: Point, radius: f64, strength: f64) -> bool {
        self.reshape(ids, |world| {
            world.segments().any(|s| distance(&s, at) < radius).then(|| smooth(world, at, radius, strength))
        })
    }

    /// Roughen every object among `ids` that the brushes can reshape along `stroke`;
    /// shapes become curves. Returns whether anything changed.
    pub fn roughen_along(&mut self, ids: &[ObjectId], stroke: &[Point], radius: f64, size: f64) -> bool {
        self.reshape(ids, |world| Some(roughen(world, stroke, radius, size)))
    }

    fn reshape(&mut self, ids: &[ObjectId], f: impl Fn(&BezPath) -> Option<BezPath>) -> bool {
        let mut changed = false;
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id) && o.can_brush()) {
            let world = obj.world_path();
            if let Some(after) = f(&world).filter(|after| *after != world) {
                make_curve(obj, after);
                changed = true;
            }
        }
        changed
    }

    /// Fit the pieces smoothing left on `ids` back into curves.
    pub fn refit_brushed(&mut self, ids: &[ObjectId], radius: f64) {
        let spacing = smooth_spacing(radius);
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            let world = obj.world_path();
            if matches!(obj.shape, Shape::Curve { .. }) && world.segments().any(|s| is_piece(&s, spacing)) {
                make_curve(obj, refit(&world, spacing, spacing / 4.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    /// A zigzag line from (0, 0) to (100, 0), teeth 10 apart and 5 high.
    fn zigzag() -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        for i in 1..=10 {
            p.line_to((f64::from(i) * 10.0, if i % 2 == 1 { 5.0 } else { 0.0 }));
        }
        p
    }

    fn wobble(path: &BezPath, x0: f64, x1: f64) -> f64 {
        let ys: Vec<f64> = path.elements().iter().filter_map(|e| e.end_point()).filter(|p| p.x > x0 && p.x < x1).map(|p| p.y).collect();
        ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min)
    }

    #[test]
    fn smoothing_flattens_only_under_the_brush() {
        let mut path = zigzag();
        for _ in 0..20 {
            path = smooth(&path, Point::new(50.0, 2.5), 20.0, 1.0);
        }
        assert!(wobble(&path, 45.0, 55.0) < 1.5, "{path:?}");
        assert!((wobble(&path, 0.0, 25.0) - 5.0).abs() < 1e-9, "far from the brush stays");
        let ends: Vec<Point> = path.elements().iter().filter_map(|e| e.end_point()).collect();
        assert_eq!((ends[0], *ends.last().unwrap()), (Point::new(0.0, 0.0), Point::new(100.0, 0.0)), "open ends stay");

        let fitted = refit(&path, smooth_spacing(20.0), smooth_spacing(20.0) / 4.0);
        assert!(fitted.elements().len() < path.elements().len(), "pieces fitted back into fewer curves");
    }

    #[test]
    fn roughening_adds_spikes_once_per_place() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let stroke = [Point::new(30.0, 0.0), Point::new(70.0, 0.0)];
        let rough = roughen(&line, &stroke, 5.0, 3.0);
        assert!((wobble(&rough, 25.0, 75.0) - 6.0).abs() < 1e-9, "spikes 3 pt each way: {rough:?}");
        let spikes: Vec<Point> = rough.elements().iter().filter_map(|e| e.end_point()).filter(|p| p.y != 0.0).collect();
        assert!(spikes.iter().all(|p| p.x > 24.0 && p.x < 76.0), "only near the stroke: {spikes:?}");
        assert!(spikes.len() >= 14, "all along it: {spikes:?}");
        assert!(spikes.windows(2).all(|w| w[0].y * w[1].y < 0.0), "out and in by turns: {spikes:?}");
        assert_eq!(roughen(&rough, &stroke, 5.0, 3.0), rough, "the same place again adds nothing");
    }

    #[test]
    fn closed_shapes_stay_closed() {
        let rect = kurbo::Rect::new(0.0, 0.0, 100.0, 50.0).to_path(0.1);
        let rough = roughen(&rect, &[Point::new(50.0, 0.0)], 20.0, 2.0);
        assert_eq!(rough.elements().iter().filter(|e| matches!(e, PathEl::ClosePath)).count(), 1);
        assert!(rough.area().abs() > 100.0 * 50.0 * 0.9);
        let smooth = smooth(&rect, Point::new(100.0, 50.0), 20.0, 1.0);
        assert!(smooth.area().abs() < 100.0 * 50.0, "the corner is rounded off");
        assert_eq!(smooth.elements().iter().filter(|e| matches!(e, PathEl::ClosePath)).count(), 1);
    }

    #[test]
    fn documents_brush_shapes_into_curves() {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Rectangle { rect: kurbo::Rect::new(0.0, 0.0, 100.0, 50.0), corner_radius: 0.0 }, crate::Style::default());
        assert!(!doc.smooth_at(&[id], Point::new(300.0, 300.0), 10.0, 1.0), "out of reach");
        assert!(!doc.roughen_along(&[id], &[Point::new(300.0, 300.0)], 10.0, 2.0), "out of reach");
        assert!(doc.roughen_along(&[id], &[Point::new(50.0, 0.0)], 10.0, 2.0));
        assert!(matches!(doc.active_page().objects[0].shape, Shape::Curve { .. }));
    }
}

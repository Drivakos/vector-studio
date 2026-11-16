//! Crop, Knife and Eraser: cutting objects with a region. Closed shapes go through the
//! boolean operations; open curves are split where they cross the region's edge. Curves
//! stay curves either way.

use kurbo::{Affine, BezPath, Line, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point, Rect, Shape as _, Stroke, StrokeOpts};
use linesweeper::BinaryOp;

use crate::document::{Document, Object, ObjectId, Shape};
use crate::shaping::{boolean, closed, page_style};

/// Samples per segment when looking for where an open curve crosses a region's edge.
const SAMPLES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Keep {
    Inside,
    Outside,
}

enum Outcome {
    Unchanged,
    Removed,
    Cut(Box<Object>),
}

/// A page-space area (even-odd, no overlaps) to cut with.
struct Region {
    path: BezPath,
    bounds: Rect,
}

impl Region {
    fn new(path: BezPath) -> Self {
        let bounds = path.bounding_box();
        Self { path, bounds }
    }

    fn contains(&self, p: Point) -> bool {
        self.bounds.contains(p) && self.path.winding(p) % 2 != 0
    }

    /// Whether the region's edge may pass through `r`.
    fn edge_meets(&self, r: Rect) -> bool {
        self.path.segments().any(|s| overlaps(s.bounding_box(), r))
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

impl Document {
    /// Crop: keep only what lies inside `rect`. Cuts the unlocked objects among `ids`, or
    /// every unlocked object on the page when `ids` is empty; objects wholly outside go.
    /// Returns what is left of them.
    pub fn crop(&mut self, ids: &[ObjectId], rect: Rect) -> Vec<ObjectId> {
        let region = Region::new(rect.to_path(0.1));
        self.cut_top_level(ids, &region, Keep::Inside)
    }

    /// Eraser: remove a round stroke `width` points wide along `points` from the unlocked
    /// objects among `ids` (every unlocked object on the page when empty). Returns whether
    /// anything changed.
    pub fn erase(&mut self, ids: &[ObjectId], points: &[Point], width: f64) -> bool {
        let Some(region) = eraser_region(points, width) else { return false };
        let before = self.active_page().objects.clone();
        self.cut_top_level(ids, &region, Keep::Outside);
        self.active_page().objects != before
    }

    /// Knife: split every unlocked object that the line from `a` to `b` crosses into its
    /// parts on either side of that line. Groups and bitmaps are not cut. Returns the
    /// pieces.
    pub fn knife(&mut self, a: Point, b: Point) -> Vec<ObjectId> {
        let Some(side) = half_plane(a, b) else { return Vec::new() };
        let cut = Line::new(a, b);
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.active_page().objects.len() {
            let obj = self.active_page().objects[i].clone();
            i += 1;
            if obj.locked || matches!(obj.shape, Shape::Group { .. } | Shape::Image(_)) {
                continue;
            }
            let world = obj.world_path();
            let crossings: usize = world.segments().map(|s| s.intersect_line(cut).len()).sum();
            if crossings < if obj.shape.is_closed() { 2 } else { 1 } {
                continue;
            }
            let (Outcome::Cut(first), Outcome::Cut(second)) =
                (cut_object(&obj, Affine::IDENTITY, &side, Keep::Inside), cut_object(&obj, Affine::IDENTITY, &side, Keep::Outside))
            else {
                continue;
            };
            let (first, mut second) = (*first, *second);
            second.id = self.alloc_object_id();
            second.name = None;
            // PowerClip contents stay with the first piece.
            second.contents.clear();
            out.extend([first.id, second.id]);
            let objects = &mut self.active_page_mut().objects;
            objects[i - 1] = first;
            objects.insert(i, second);
            i += 1;
        }
        out
    }

    fn cut_top_level(&mut self, ids: &[ObjectId], region: &Region, keep: Keep) -> Vec<ObjectId> {
        let all = ids.is_empty();
        let mut kept = Vec::new();
        let objects = std::mem::take(&mut self.active_page_mut().objects);
        let mut out = Vec::with_capacity(objects.len());
        for obj in objects {
            if !all && !ids.contains(&obj.id) {
                out.push(obj);
                continue;
            }
            match cut_object(&obj, Affine::IDENTITY, region, keep) {
                Outcome::Unchanged => {
                    if !obj.locked {
                        kept.push(obj.id);
                    }
                    out.push(obj);
                }
                Outcome::Removed => {}
                Outcome::Cut(o) => {
                    kept.push(o.id);
                    out.push(*o);
                }
            }
        }
        self.active_page_mut().objects = out;
        kept
    }
}

/// `obj` (in the space `parent` maps to the page) cut by `region`, keeping the side `keep`.
fn cut_object(obj: &Object, parent: Affine, region: &Region, keep: Keep) -> Outcome {
    if obj.locked {
        return Outcome::Unchanged;
    }
    let to_page = parent * obj.transform;
    match &obj.shape {
        Shape::Group { children } => {
            let mut changed = false;
            let mut kids = Vec::new();
            for child in children {
                match cut_object(child, to_page, region, keep) {
                    Outcome::Unchanged => kids.push(child.clone()),
                    Outcome::Removed => changed = true,
                    Outcome::Cut(c) => {
                        changed = true;
                        kids.push(*c);
                    }
                }
            }
            match (changed, kids.is_empty()) {
                (false, _) => Outcome::Unchanged,
                (true, true) => Outcome::Removed,
                (true, false) => Outcome::Cut(Box::new(Object { shape: Shape::Group { children: kids }, ..obj.clone() })),
            }
        }
        // Bitmaps are not cut: they stay, or go when wholly on the side that goes.
        Shape::Image(b) => {
            let bounds = to_page.transform_rect_bbox(b.rect);
            let wholly = !region.edge_meets(bounds);
            match keep {
                Keep::Inside if wholly && !region.contains(bounds.center()) => Outcome::Removed,
                Keep::Outside if wholly && region.contains(bounds.center()) => Outcome::Removed,
                _ => Outcome::Unchanged,
            }
        }
        shape => {
            let mut world = shape.to_path();
            world.apply_affine(to_page);
            let Some(first) = world.elements().first().and_then(|e| e.end_point()) else { return Outcome::Unchanged };
            // Nothing crosses the edge: the object is wholly kept or wholly gone.
            if !region.edge_meets(world.bounding_box()) {
                return if region.contains(first) == (keep == Keep::Inside) { Outcome::Unchanged } else { Outcome::Removed };
            }
            let path = if shape.is_closed() {
                let op = if keep == Keep::Inside { BinaryOp::Intersection } else { BinaryOp::Difference };
                match boolean(&closed(&world), &region.path, op) {
                    Some(p) => p,
                    None => return Outcome::Unchanged,
                }
            } else {
                clip_open(&world, |p| region.contains(p) == (keep == Keep::Inside))
            };
            if path.elements().len() < 2 {
                return Outcome::Removed;
            }
            let mut local = path;
            local.apply_affine(parent.inverse());
            Outcome::Cut(Box::new(Object { shape: Shape::Curve { path: local }, style: page_style(obj), transform: Affine::IDENTITY, ..obj.clone() }))
        }
    }
}

/// The parts of the open path `path` where `keep` holds, split exactly where it changes.
fn clip_open(path: &BezPath, keep: impl Fn(Point) -> bool) -> BezPath {
    let mut out = BezPath::new();
    let mut open_end: Option<Point> = None;
    for seg in path.segments() {
        let mut breaks = vec![0.0];
        let mut prev = keep(seg.eval(0.0));
        for k in 1..=SAMPLES {
            let t = k as f64 / SAMPLES as f64;
            let now = keep(seg.eval(t));
            if now != prev {
                let (mut lo, mut hi) = ((k - 1) as f64 / SAMPLES as f64, t);
                for _ in 0..40 {
                    let mid = (lo + hi) / 2.0;
                    if keep(seg.eval(mid)) == prev { lo = mid } else { hi = mid }
                }
                breaks.push((lo + hi) / 2.0);
                prev = now;
            }
        }
        breaks.push(1.0);
        for w in breaks.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if t1 - t0 < 1e-9 {
                continue;
            }
            if !keep(seg.eval((t0 + t1) / 2.0)) {
                open_end = None;
                continue;
            }
            let piece = seg.subsegment(t0..t1);
            if open_end.is_none_or(|e| (e - piece.start()).hypot() > 1e-9) {
                out.move_to(piece.start());
            }
            match piece {
                PathSeg::Line(l) => out.line_to(l.p1),
                PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
            }
            open_end = Some(piece.end());
        }
    }
    out
}

/// The area a round eraser `width` wide covers along `points`.
fn eraser_region(points: &[Point], width: f64) -> Option<Region> {
    let first = *points.first()?;
    if width <= 0.0 {
        return None;
    }
    let mut line = BezPath::new();
    line.move_to(first);
    for p in &points[1..] {
        line.line_to(*p);
    }
    let path = if points.len() < 2 {
        kurbo::Circle::new(first, width / 2.0).to_path(0.01)
    } else {
        let stroke = Stroke::new(width).with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round);
        crate::curves::nonzero_region(&kurbo::stroke(line.iter(), &stroke, &StrokeOpts::default(), 0.01))?
    };
    Some(Region::new(path))
}

/// The side of the line through `a` and `b` to its left, as a huge polygon.
fn half_plane(a: Point, b: Point) -> Option<Region> {
    let d = b - a;
    if d.hypot() < 1e-9 {
        return None;
    }
    let d = d / d.hypot() * 1e6;
    let n = kurbo::Vec2::new(-d.y, d.x);
    let mut path = BezPath::new();
    path.move_to(a - d);
    path.line_to(b + d);
    path.line_to(b + d + n);
    path.line_to(a - d + n);
    path.close_path();
    Some(Region::new(path))
}

/// Tolerance of the outlines other objects are flattened to when finding where they
/// cross the curve a virtual segment is taken from.
const CROSSING_TOLERANCE: f64 = 0.01;

impl Document {
    /// Virtual Segment Delete: on the topmost unlocked shape or curve whose outline passes
    /// within `tolerance` of `at`, delete the part of the outline between the crossings
    /// (with other objects' outlines, or itself) on either side of `at`. An open curve with
    /// no crossing there loses everything up to its end; a closed subpath crossed fewer
    /// than twice goes whole. The object becomes a curve, and goes when nothing is left.
    /// Groups, text and bitmaps are not cut, and text and bitmaps do not cross. Returns
    /// whether anything was deleted.
    pub fn delete_virtual_segment(&mut self, at: Point, tolerance: f64) -> bool {
        let page = self.active_page();
        let hit = page.objects.iter().enumerate().rev().find_map(|(i, obj)| {
            let cuttable = !obj.locked && !matches!(obj.shape, Shape::Group { .. } | Shape::Text(_) | Shape::Image(_));
            let subpaths = subpaths(&obj.world_path());
            let (k, u) = cuttable.then(|| nearest_on(&subpaths, at, tolerance)).flatten()?;
            Some((i, subpaths, k, u))
        });
        let Some((index, subpaths, k, u)) = hit else { return false };
        let target = &page.objects[index];
        // Everything that can cross it, flattened to line segments.
        let mut lines: Vec<(Option<(usize, usize)>, Line)> = Vec::new();
        for (i, obj) in page.objects.iter().enumerate() {
            if i != index && !matches!(obj.shape, Shape::Text(_) | Shape::Image(_)) {
                flat_lines(&obj.world_path(), &mut |l| lines.push((None, l)));
            }
        }
        for (j, (segs, _)) in subpaths.iter().enumerate() {
            for (n, seg) in segs.iter().enumerate() {
                flat_lines(&BezPath::from_path_segments(std::iter::once(*seg)), &mut |l| lines.push((Some((j, n)), l)));
            }
        }
        let (segs, closed) = &subpaths[k];
        let mut cuts: Vec<f64> = Vec::new();
        for (n, seg) in segs.iter().enumerate() {
            for (own, line) in &lines {
                if *own == Some((k, n)) {
                    continue;
                }
                for hit in seg.intersect_line(*line) {
                    // A curve meets its own neighbouring segments at its nodes: not a crossing.
                    let at_node = own.is_some() && (hit.segment_t < 1e-6 || hit.segment_t > 1.0 - 1e-6);
                    if !at_node {
                        cuts.push(n as f64 + hit.segment_t);
                    }
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-7);

        let len = segs.len() as f64;
        let kept: Vec<BezPath> = if *closed {
            if cuts.len() < 2 {
                Vec::new()
            } else {
                let next = cuts.iter().copied().find(|c| *c > u).unwrap_or(cuts[0] + len);
                let prev = cuts.iter().copied().rev().find(|c| *c < u).unwrap_or(cuts[cuts.len() - 1] - len);
                // Round from the next crossing to the previous one.
                vec![piece(segs, next, prev + len)]
            }
        } else {
            let prev = cuts.iter().copied().rev().find(|c| *c < u).unwrap_or(0.0);
            let next = cuts.iter().copied().find(|c| *c > u).unwrap_or(len);
            [(0.0, prev), (next, len)].into_iter().filter(|(a, b)| b - a > 1e-7).map(|(a, b)| piece(segs, a, b)).collect()
        };

        let mut path = BezPath::new();
        for (j, (segs, closed)) in subpaths.iter().enumerate() {
            if j == k {
                for p in &kept {
                    path.extend(p.elements().iter().copied());
                }
            } else {
                path.extend(BezPath::from_path_segments(segs.iter().copied()).elements().iter().copied());
                if *closed {
                    path.close_path();
                }
            }
        }
        let obj = target.clone();
        let objects = &mut self.active_page_mut().objects;
        if path.elements().len() < 2 {
            objects.remove(index);
        } else {
            objects[index] = Object { shape: Shape::Curve { path }, style: page_style(&obj), transform: Affine::IDENTITY, ..obj };
        }
        true
    }
}

/// The subpaths of `path` as their segments, each with whether it is closed. A closed
/// subpath's last segment ends at its start.
fn subpaths(path: &BezPath) -> Vec<(Vec<PathSeg>, bool)> {
    let mut out: Vec<(Vec<PathSeg>, bool)> = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                out.push((Vec::new(), false));
                (start, last) = (p, p);
                continue;
            }
            PathEl::LineTo(p) => PathSeg::Line(Line::new(last, p)),
            PathEl::QuadTo(a, p) => PathSeg::Quad(kurbo::QuadBez::new(last, a, p)),
            PathEl::CurveTo(a, b, p) => PathSeg::Cubic(kurbo::CubicBez::new(last, a, b, p)),
            PathEl::ClosePath => {
                if let Some((segs, closed)) = out.last_mut() {
                    if (last - start).hypot() > 1e-9 {
                        segs.push(PathSeg::Line(Line::new(last, start)));
                    }
                    *closed = true;
                }
                last = start;
                continue;
            }
        };
        last = seg.end();
        if let Some((segs, _)) = out.last_mut() {
            segs.push(seg);
        }
    }
    out.retain(|(segs, _)| !segs.is_empty());
    out
}

/// The subpath and position (segment index + t) of the outline point nearest `at`, if
/// it is within `tolerance`.
fn nearest_on(subpaths: &[(Vec<PathSeg>, bool)], at: Point, tolerance: f64) -> Option<(usize, f64)> {
    let mut best: Option<(f64, usize, f64)> = None;
    for (k, (segs, _)) in subpaths.iter().enumerate() {
        for (n, seg) in segs.iter().enumerate() {
            let near = seg.nearest(at, 1e-6);
            if best.is_none_or(|b| near.distance_sq < b.0) {
                best = Some((near.distance_sq, k, n as f64 + near.t));
            }
        }
    }
    best.filter(|b| b.0 <= tolerance * tolerance).map(|b| (b.1, b.2))
}

/// `path` flattened, line by line.
fn flat_lines(path: &BezPath, out: &mut impl FnMut(Line)) {
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    kurbo::flatten(path.elements().iter().copied(), CROSSING_TOLERANCE, |el| match el {
        PathEl::MoveTo(p) => (start, last) = (p, p),
        PathEl::LineTo(p) => {
            out(Line::new(last, p));
            last = p;
        }
        PathEl::ClosePath => {
            out(Line::new(last, start));
            last = start;
        }
        _ => {}
    });
}

/// The part of the curve `segs` from position `from` to `to` (segment index + t; past
/// the end goes round again, for closed subpaths).
fn piece(segs: &[PathSeg], from: f64, to: f64) -> BezPath {
    let n = segs.len();
    let mut path = BezPath::new();
    let mut u = from;
    while u < to - 1e-9 {
        let k = u.floor();
        let end = (k + 1.0).min(to);
        let seg = segs[(k as usize) % n].subsegment((u - k)..(end - k));
        if path.elements().is_empty() {
            path.move_to(seg.start());
        }
        path.push(seg.as_path_el());
        u = end;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Outline, Rgba, Style};

    fn rect(doc: &mut Document, r: Rect) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, Style::filled(Rgba::BLACK))
    }

    fn line(doc: &mut Document, a: (f64, f64), b: (f64, f64)) -> ObjectId {
        let mut path = BezPath::new();
        path.move_to(a);
        path.line_to(b);
        doc.add_object(Shape::Curve { path }, Style::outlined(Outline::new(Rgba::BLACK, 1.0)))
    }

    fn area(doc: &Document, id: ObjectId) -> f64 {
        doc.active_page().object(id).unwrap().world_path().area().abs()
    }

    #[test]
    fn crop_keeps_what_is_inside_and_drops_what_is_outside() {
        let mut doc = Document::default();
        let half = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0));
        let gone = rect(&mut doc, Rect::new(300.0, 300.0, 350.0, 350.0));
        let inside = rect(&mut doc, Rect::new(10.0, 10.0, 20.0, 20.0));
        let wire = line(&mut doc, (-50.0, 25.0), (150.0, 25.0));
        let kept = doc.crop(&[], Rect::new(0.0, 0.0, 50.0, 200.0));
        assert_eq!(kept, vec![half, inside, wire]);
        assert_eq!(doc.all_object_ids(), vec![half, inside, wire], "{gone:?} was outside");
        assert!((area(&doc, half) - 5000.0).abs() < 1e-6);
        assert!(matches!(doc.active_page().object(inside).unwrap().shape, Shape::Rectangle { .. }), "untouched objects stay as they were");
        let b = doc.active_page().object(wire).unwrap().world_bounds();
        assert!((b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 50.0).abs() < 1e-6, "the line is clipped to the crop: {b:?}");
    }

    #[test]
    fn knife_splits_a_shape_and_a_line_in_two() {
        let mut doc = Document::default();
        let r = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 50.0));
        let l = line(&mut doc, (0.0, 100.0), (100.0, 100.0));
        let far = rect(&mut doc, Rect::new(300.0, 0.0, 320.0, 20.0));
        let pieces = doc.knife(Point::new(30.0, -10.0), Point::new(30.0, 120.0));
        assert_eq!(pieces.len(), 4);
        assert_eq!(doc.all_object_ids().len(), 5, "{far:?} is not crossed");
        assert_eq!(pieces[0], r, "the first piece keeps the id");
        let areas: Vec<f64> = pieces[..2].iter().map(|id| area(&doc, *id)).collect();
        assert!((areas[0] + areas[1] - 5000.0).abs() < 1e-6 && areas.iter().any(|a| (a - 1500.0).abs() < 1e-6), "{areas:?}");
        assert_eq!(pieces[2], l);
        let widths: Vec<f64> = pieces[2..].iter().map(|id| doc.active_page().object(*id).unwrap().world_bounds().width()).collect();
        assert!(widths.iter().any(|w| (w - 30.0).abs() < 1e-6) && widths.iter().any(|w| (w - 70.0).abs() < 1e-6), "{widths:?}");
        assert!(doc.knife(Point::new(-10.0, 200.0), Point::new(-5.0, 300.0)).is_empty(), "a cut that crosses nothing");
    }

    #[test]
    fn eraser_removes_a_round_stroke_from_shapes_it_touches() {
        let mut doc = Document::default();
        let r = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0));
        let untouched = rect(&mut doc, Rect::new(200.0, 0.0, 220.0, 20.0));
        assert!(doc.erase(&[], &[Point::new(-20.0, 50.0), Point::new(120.0, 50.0)], 10.0));
        assert!((area(&doc, r) - 9000.0).abs() < 1e-3, "a 10 pt band is gone: {}", area(&doc, r));
        assert!(matches!(doc.active_page().object(untouched).unwrap().shape, Shape::Rectangle { .. }));
        // One click: a round hole.
        assert!(doc.erase(&[r], &[Point::new(50.0, 20.0)], 10.0));
        let hole = std::f64::consts::PI * 25.0;
        assert!((area(&doc, r) - (9000.0 - hole)).abs() < 0.05, "{}", area(&doc, r));
        assert!(!doc.erase(&[], &[Point::new(500.0, 500.0)], 10.0), "erasing empty space changes nothing");
    }

    #[test]
    fn virtual_segment_delete_removes_the_part_between_crossings() {
        let mut doc = Document::default();
        // A line crossed by two verticals at x = 30 and x = 70.
        let l = line(&mut doc, (0.0, 50.0), (100.0, 50.0));
        line(&mut doc, (30.0, 0.0), (30.0, 100.0));
        line(&mut doc, (70.0, 0.0), (70.0, 100.0));
        assert!(doc.delete_virtual_segment(Point::new(50.0, 50.5), 2.0), "the middle goes");
        let path = doc.active_page().object(l).unwrap().world_path();
        let ends: Vec<Point> = path.elements().iter().filter_map(|e| e.end_point()).collect();
        assert_eq!(path.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 2, "two pieces: {ends:?}");
        assert!(ends.iter().any(|p| (p.x - 30.0).abs() < 1e-6) && ends.iter().any(|p| (p.x - 70.0).abs() < 1e-6), "{ends:?}");
        let b = doc.active_page().object(l).unwrap().world_bounds();
        assert!((b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 100.0).abs() < 1e-6);

        // The end piece runs to the curve's end.
        assert!(doc.delete_virtual_segment(Point::new(90.0, 50.0), 2.0));
        let b = doc.active_page().object(l).unwrap().world_bounds();
        assert!((b.x1 - 30.0).abs() < 1e-6, "only 0..30 is left: {b:?}");
        assert!(doc.delete_virtual_segment(Point::new(10.0, 50.0), 2.0));
        assert!(doc.active_page().object(l).is_none(), "nothing left: the object goes");
        assert!(!doc.delete_virtual_segment(Point::new(500.0, 500.0), 2.0), "no outline there");
    }

    #[test]
    fn virtual_segment_delete_opens_a_crossed_shape_and_removes_an_uncrossed_one() {
        let mut doc = Document::default();
        let r = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0));
        line(&mut doc, (50.0, -20.0), (50.0, 120.0));
        // The right edge: the outline from x = 50 on the top round to x = 50 on the bottom.
        assert!(doc.delete_virtual_segment(Point::new(100.0, 50.0), 2.0));
        let obj = doc.active_page().object(r).unwrap();
        assert!(matches!(obj.shape, Shape::Curve { .. }), "it is a curve now");
        let b = obj.world_bounds();
        assert!((b.x0 - 0.0).abs() < 1e-6 && (b.x1 - 50.0).abs() < 1e-6, "the left half is left: {b:?}");
        assert!(!obj.shape.is_closed(), "and it is open");

        let lone = rect(&mut doc, Rect::new(300.0, 300.0, 320.0, 320.0));
        assert!(doc.delete_virtual_segment(Point::new(300.0, 310.0), 2.0));
        assert!(doc.active_page().object(lone).is_none(), "an uncrossed closed shape goes whole");
    }
}

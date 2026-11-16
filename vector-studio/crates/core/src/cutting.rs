//! Crop, Knife and Eraser: cutting objects with a region. Closed shapes go through the
//! boolean operations; open curves are split where they cross the region's edge. Curves
//! stay curves either way.

use kurbo::{Affine, BezPath, Line, ParamCurve, PathSeg, Point, Rect, Shape as _, Stroke, StrokeOpts};
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
}

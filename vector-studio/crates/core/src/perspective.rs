//! Perspective: an object drawn through a projective map, as if seen at an angle. The map
//! is set by where the four corners of the object's own bounds go; straight lines stay
//! straight and the far side shrinks.

use kurbo::{Affine, BezPath, CubicBez, ParamCurve, PathEl, Point, QuadBez, Rect};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, ObjectId, Shape};

/// A projective map of the plane, as a row-major 3 × 3 matrix acting on (x, y, 1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Perspective(pub [f64; 9]);

/// Pieces each curve segment is cut into before it is mapped, so the mapped curve stays
/// close to the true (rational) one.
const PIECES: usize = 8;

impl Perspective {
    pub const IDENTITY: Perspective = Perspective([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);

    pub fn from_affine(t: Affine) -> Self {
        let [a, b, c, d, e, f] = t.as_coeffs();
        Self([a, c, e, b, d, f, 0.0, 0.0, 1.0])
    }

    /// `self`, then `after`.
    pub fn then(self, after: Perspective) -> Perspective {
        let (m, n) = (after.0, self.0);
        let mut out = [0.0; 9];
        for r in 0..3 {
            for c in 0..3 {
                out[r * 3 + c] = (0..3).map(|k| m[r * 3 + k] * n[k * 3 + c]).sum();
            }
        }
        Perspective(out)
    }

    /// This map as seen from the space `t` maps into the one it works in.
    pub fn seen_from(self, t: Affine) -> Perspective {
        Perspective::from_affine(t).then(self).then(Perspective::from_affine(t.inverse()))
    }

    pub fn apply(&self, p: Point) -> Point {
        let m = &self.0;
        let w = m[6] * p.x + m[7] * p.y + m[8];
        Point::new((m[0] * p.x + m[1] * p.y + m[2]) / w, (m[3] * p.x + m[4] * p.y + m[5]) / w)
    }

    fn is_affine(&self) -> bool {
        self.0[6] == 0.0 && self.0[7] == 0.0
    }

    /// The map that takes the corners of `rect` (top-left, top-right, bottom-right,
    /// bottom-left) to `quad`. `None` unless `quad` is convex and `rect` has an area.
    pub fn rect_to_quad(rect: Rect, quad: [Point; 4]) -> Option<Perspective> {
        if rect.width() <= 0.0 || rect.height() <= 0.0 || !convex(&quad) {
            return None;
        }
        // Heckbert's unit square → quad, after rect → unit square.
        let [p0, p1, p2, p3] = quad;
        let (dx1, dx2, dx3) = (p1.x - p2.x, p3.x - p2.x, p0.x - p1.x + p2.x - p3.x);
        let (dy1, dy2, dy3) = (p1.y - p2.y, p3.y - p2.y, p0.y - p1.y + p2.y - p3.y);
        let det = dx1 * dy2 - dx2 * dy1;
        if det.abs() < 1e-12 {
            return None;
        }
        let g = (dx3 * dy2 - dx2 * dy3) / det;
        let h = (dx1 * dy3 - dx3 * dy1) / det;
        let square = Perspective([
            p1.x - p0.x + g * p1.x,
            p3.x - p0.x + h * p3.x,
            p0.x,
            p1.y - p0.y + g * p1.y,
            p3.y - p0.y + h * p3.y,
            p0.y,
            g,
            h,
            1.0,
        ]);
        let to_unit = Affine::scale_non_uniform(1.0 / rect.width(), 1.0 / rect.height()) * Affine::translate(-rect.origin().to_vec2());
        Some(Perspective::from_affine(to_unit).then(square))
    }

    /// `path` through the map. Lines stay exact; curves are cut into pieces first.
    pub fn map_path(&self, path: &BezPath) -> BezPath {
        let mut out = BezPath::new();
        let mut last = Point::ZERO;
        let curve = |out: &mut BezPath, c: CubicBez| {
            let pieces = if self.is_affine() { 1 } else { PIECES };
            for i in 0..pieces {
                let s = c.subsegment(i as f64 / pieces as f64..(i + 1) as f64 / pieces as f64);
                out.curve_to(self.apply(s.p1), self.apply(s.p2), self.apply(s.p3));
            }
        };
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    out.move_to(self.apply(p));
                    last = p;
                }
                PathEl::LineTo(p) => {
                    out.line_to(self.apply(p));
                    last = p;
                }
                PathEl::QuadTo(c, p) => {
                    curve(&mut out, QuadBez::new(last, c, p).raise());
                    last = p;
                }
                PathEl::CurveTo(a, b, p) => {
                    curve(&mut out, CubicBez::new(last, a, b, p));
                    last = p;
                }
                PathEl::ClosePath => out.close_path(),
            }
        }
        out
    }
}

/// Whether the four corners make a convex quadrilateral (either way round).
fn convex(q: &[Point; 4]) -> bool {
    let turns: Vec<f64> = (0..4).map(|i| (q[(i + 1) % 4] - q[i]).cross(q[(i + 2) % 4] - q[(i + 1) % 4])).collect();
    turns.iter().all(|t| *t > 1e-9) || turns.iter().all(|t| *t < -1e-9)
}

fn corners(r: Rect) -> [Point; 4] {
    [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)]
}

impl Object {
    /// The bounds the perspective is set on: the object's own, in its space, before the
    /// perspective.
    pub fn perspective_frame(&self) -> Rect {
        self.plain_bounds(Affine::IDENTITY)
    }

    /// Where the corners of [`Self::perspective_frame`] show on the page (top-left,
    /// top-right, bottom-right, bottom-left), if the object has a perspective.
    pub fn perspective_corners(&self) -> Option<[Point; 4]> {
        let h = self.perspective?;
        Some(corners(self.perspective_frame()).map(|c| self.transform * h.apply(c)))
    }

    /// Apply `h` (in this object's space) after any perspective it has.
    pub fn push_perspective(&mut self, h: Perspective) {
        self.perspective = Some(self.perspective.map_or(h, |own| own.then(h)));
    }

    /// Make the perspective part of the geometry: shapes and text become curves, and a
    /// group passes it on to its members. Bitmaps cannot take one and keep their shape.
    pub fn bake_perspective(&mut self) {
        let Some(h) = self.perspective.take() else { return };
        for c in &mut self.contents {
            c.push_perspective(h.seen_from(c.transform));
            c.bake_perspective();
        }
        match &mut self.shape {
            Shape::Group { children } => {
                for c in children {
                    c.push_perspective(h.seen_from(c.transform));
                    c.bake_perspective();
                }
            }
            Shape::Image(_) => {}
            shape => *shape = Shape::Curve { path: h.map_path(&shape.to_path()) },
        }
    }

    fn has_bitmap(&self) -> bool {
        matches!(self.shape, Shape::Image(_))
            || self.contents.iter().any(Object::has_bitmap)
            || matches!(&self.shape, Shape::Group { children } if children.iter().any(Object::has_bitmap))
    }
}

impl Document {
    fn perspective_target(&self, ids: &[ObjectId]) -> Option<&Object> {
        let picked: Vec<&Object> = self.active_page().objects.iter().filter(|o| ids.contains(&o.id)).collect();
        match picked.as_slice() {
            [o] if !o.locked && !o.has_bitmap() => Some(*o),
            _ => None,
        }
    }

    /// Add Perspective applies: one unlocked object without a bitmap and without a
    /// perspective yet, with an area to set it on.
    pub fn can_add_perspective(&self, ids: &[ObjectId]) -> bool {
        self.perspective_target(ids).is_some_and(|o| {
            let f = o.perspective_frame();
            o.perspective.is_none() && f.width() > 0.0 && f.height() > 0.0
        })
    }

    /// Add Perspective: the object gets a perspective that does not change it yet; its
    /// corners are then dragged with the Shape tool. Returns the object.
    pub fn add_perspective(&mut self, ids: &[ObjectId]) -> Option<ObjectId> {
        if !self.can_add_perspective(ids) {
            return None;
        }
        let id = self.perspective_target(ids)?.id;
        self.active_page_mut().object_mut(id)?.perspective = Some(Perspective::IDENTITY);
        Some(id)
    }

    /// The unlocked objects among `ids` with a perspective.
    pub fn with_perspective(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && !o.locked && o.perspective.is_some()).map(|o| o.id).collect()
    }

    /// Clear Effect for a perspective: the objects among `ids` go back to how they were
    /// drawn. Returns those changed.
    pub fn clear_perspective(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let changed = self.with_perspective(ids);
        for id in &changed {
            if let Some(o) = self.active_page_mut().object_mut(*id) {
                o.perspective = None;
            }
        }
        changed
    }

    /// Move corner `index` (0 top-left, then clockwise) of `id`'s perspective to `to`
    /// (page space). Refused, returning false, when the corners would no longer make a
    /// convex shape.
    pub fn set_perspective_corner(&mut self, id: ObjectId, index: usize, to: Point) -> bool {
        let Some(obj) = self.active_page_mut().object_mut(id).filter(|o| !o.locked) else { return false };
        let (Some(mut quad), true) = (obj.perspective_corners(), index < 4) else { return false };
        let to_local = obj.transform.inverse();
        quad[index] = to;
        let Some(h) = Perspective::rect_to_quad(obj.perspective_frame(), quad.map(|q| to_local * q)) else { return false };
        obj.perspective = Some(h);
        true
    }

    /// Copy Effect > Perspective From: give the objects among `ids` (one at a time, as
    /// Add Perspective would take them) `source`'s perspective, its corners placed the
    /// same way on their bounds. Returns those changed.
    pub fn copy_perspective(&mut self, ids: &[ObjectId], source: ObjectId) -> Vec<ObjectId> {
        let Some(src) = self.active_page().object(source) else { return Vec::new() };
        let Some(h) = src.perspective else { return Vec::new() };
        let frame = src.perspective_frame();
        // The corners as fractions of the source's bounds.
        let unit = corners(frame).map(|c| {
            let p = h.apply(c);
            Point::new((p.x - frame.x0) / frame.width(), (p.y - frame.y0) / frame.height())
        });
        let mut out = Vec::new();
        for id in ids.iter().copied().filter(|id| *id != source) {
            let Some(obj) = self.active_page_mut().object_mut(id).filter(|o| !o.locked && !o.has_bitmap()) else { continue };
            let f = obj.perspective_frame();
            let quad = unit.map(|u| Point::new(f.x0 + u.x * f.width(), f.y0 + u.y * f.height()));
            if let Some(h) = Perspective::rect_to_quad(f, quad) {
                obj.perspective = Some(h);
                out.push(id);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rgba, Style};
    use kurbo::Shape as _;

    fn near(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-6
    }

    fn square(doc: &mut Document) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 100.0, 100.0), corner_radius: 0.0 }, Style::filled(Rgba::rgb(255, 0, 0)))
    }

    #[test]
    fn rect_to_quad_sends_each_corner_home_and_keeps_lines_straight() {
        let rect = Rect::new(10.0, 20.0, 110.0, 70.0);
        let quad = [Point::new(30.0, 20.0), Point::new(90.0, 20.0), Point::new(110.0, 70.0), Point::new(10.0, 70.0)];
        let h = Perspective::rect_to_quad(rect, quad).unwrap();
        for (c, q) in corners(rect).into_iter().zip(quad) {
            assert!(near(h.apply(c), q), "{c:?} → {:?}, want {q:?}", h.apply(c));
        }
        // The midpoint of the top edge stays on the (mapped) top edge.
        assert!((h.apply(Point::new(60.0, 20.0)).y - 20.0).abs() < 1e-9);
        // A trapezoid: the narrow far side squeezes the middle row up.
        assert!(h.apply(rect.center()).y < 45.0, "{:?}", h.apply(rect.center()));
        assert!(Perspective::rect_to_quad(rect, [quad[0], quad[2], quad[1], quad[3]]).is_none(), "crossed");
        assert!(Perspective::rect_to_quad(Rect::new(0.0, 0.0, 0.0, 10.0), quad).is_none(), "no area");
        let same = Perspective::rect_to_quad(rect, corners(rect)).unwrap();
        assert!(near(same.apply(Point::new(37.0, 41.0)), Point::new(37.0, 41.0)), "identity");
    }

    #[test]
    fn composing_and_seen_from_agree_with_applying_in_turn() {
        let h = Perspective::rect_to_quad(Rect::new(0.0, 0.0, 10.0, 10.0), [Point::new(2.0, 0.0), Point::new(8.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)]).unwrap();
        let t = Affine::translate((5.0, -3.0)) * Affine::rotate(0.4) * Affine::scale(2.0);
        let p = Point::new(3.0, 4.0);
        assert!(near(Perspective::from_affine(t).then(h).apply(p), h.apply(t * p)));
        assert!(near(t * h.seen_from(t).apply(p), h.apply(t * p)));
    }

    #[test]
    fn a_perspective_is_drawn_hit_and_bounded_through_its_corners() {
        let mut doc = Document::default();
        let id = square(&mut doc);
        assert!(doc.can_add_perspective(&[id]));
        assert_eq!(doc.add_perspective(&[id]), Some(id));
        assert!(!doc.can_add_perspective(&[id]), "already has one");
        doc.move_objects(&[id], kurbo::Vec2::new(100.0, 0.0));
        // Pull the top corners in: a trapezoid narrow at the top.
        assert!(doc.set_perspective_corner(id, 0, Point::new(130.0, 0.0)));
        assert!(doc.set_perspective_corner(id, 1, Point::new(170.0, 0.0)));
        assert!(!doc.set_perspective_corner(id, 1, Point::new(100.0, 100.0)), "would not be convex");
        let obj = doc.active_page().object(id).unwrap();
        let c = obj.perspective_corners().unwrap();
        assert!(near(c[0], Point::new(130.0, 0.0)) && near(c[1], Point::new(170.0, 0.0)) && near(c[2], Point::new(200.0, 100.0)));
        assert!(obj.hit(Point::new(150.0, 5.0), 0.0) && !obj.hit(Point::new(110.0, 5.0), 0.0), "the top is narrow");
        assert!(obj.hit(Point::new(105.0, 95.0), 0.0), "the bottom is full width");
        let b = obj.world_bounds();
        assert!((b.x0 - 100.0).abs() < 1e-6 && (b.x1 - 200.0).abs() < 1e-6 && b.y0.abs() < 1e-6 && (b.y1 - 100.0).abs() < 1e-6, "{b:?}");
        assert!((obj.world_path().area().abs() - 7000.0).abs() < 1e-6, "trapezoid (40 + 100) / 2 × 100");

        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active_page().object(id).unwrap().perspective, obj.perspective, "saved");

        assert_eq!(doc.clear_perspective(&[id]), vec![id]);
        assert!((doc.active_page().object(id).unwrap().world_path().area().abs() - 10000.0).abs() < 1e-6);
    }

    #[test]
    fn baking_ungrouping_and_copying_keep_how_it_looks() {
        let mut doc = Document::default();
        let a = square(&mut doc);
        let b = doc.add_object(Shape::Ellipse { rect: Rect::new(200.0, 0.0, 300.0, 100.0), arc: None }, Style::filled(Rgba::BLACK));
        let group = doc.group_objects(&[a, b]).unwrap();
        doc.add_perspective(&[group]);
        assert!(doc.set_perspective_corner(group, 0, Point::new(50.0, 20.0)));
        let before = doc.active_page().object(group).unwrap().world_bounds();
        let probe = Point::new(30.0, 10.0);
        assert!(!doc.active_page().object(group).unwrap().hit(probe, 0.0));

        // Ungroup hands the perspective to each member.
        let mut ungrouped = doc.clone();
        ungrouped.ungroup_objects(&[group]);
        let page = ungrouped.active_page();
        assert!(page.objects.iter().all(|o| o.perspective.is_some()));
        let bounds = page.bounds_of(&[a, b]).unwrap();
        assert!((bounds.x0 - before.x0).abs() < 1e-6 && (bounds.y0 - before.y0).abs() < 1e-6 && (bounds.x1 - before.x1).abs() < 1e-6, "{bounds:?} vs {before:?}");
        assert!(!page.object(a).unwrap().hit(probe, 0.0));

        // Convert to Curves bakes it in.
        doc.convert_to_curves(&[group]);
        let obj = doc.active_page().object(group).unwrap();
        assert!(obj.perspective.is_none());
        let Shape::Group { children } = &obj.shape else { panic!("group") };
        assert!(children.iter().all(|c| c.perspective.is_none() && matches!(c.shape, Shape::Curve { .. })));
        let after = obj.world_bounds();
        assert!((after.x0 - before.x0).abs() < 1e-6 && (after.x1 - before.x1).abs() < 1e-6 && (after.y1 - before.y1).abs() < 1e-6);

        // Copy the perspective of one onto another of a different size.
        let mut doc = Document::default();
        let src = square(&mut doc);
        let dst = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 200.0, 50.0, 400.0), corner_radius: 0.0 }, Style::default());
        doc.add_perspective(&[src]);
        assert!(doc.set_perspective_corner(src, 1, Point::new(60.0, 30.0)));
        assert_eq!(doc.copy_perspective(&[dst, src], src), vec![dst]);
        let c = doc.active_page().object(dst).unwrap().perspective_corners().unwrap();
        assert!(near(c[1], Point::new(30.0, 260.0)), "as far across and down as the source's: {c:?}");
    }
}

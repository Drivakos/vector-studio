//! Shaping (Weld, Trim, Intersect, Simplify, Front/Back Minus, Boundary), Combine and
//! Break Apart. The boolean operations keep Bézier curves.

use kurbo::{Affine, BezPath, PathEl, Point, Shape as _};
use linesweeper::{BinaryOp, FillRule, binary_op};

use crate::document::{Document, Object, ObjectId, Shape, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shaping {
    /// One object covering every selected object, styled like the target.
    Weld,
    /// The target minus the other objects, which stay.
    Trim,
    /// A new object where the target overlaps the others; all of them stay.
    Intersect,
    /// Every object minus the objects in front of it.
    Simplify,
    /// The front object minus the ones behind it, which go.
    FrontMinusBack,
    /// The back object minus the ones in front of it, which go.
    BackMinusFront,
    /// A new unfilled object around the outside of the selection; the objects stay.
    Boundary,
}

impl Document {
    /// Whether shaping can run on `ids`: at least two unlocked objects, all closed.
    pub fn can_shape(&self, ids: &[ObjectId]) -> bool {
        let picked: Vec<_> = self.unlocked(ids).collect();
        picked.len() >= 2 && picked.iter().all(|o| o.leaves().iter().all(|l| l.closed))
    }

    /// Run `op` on the unlocked objects among `ids`. `target` is the object whose fill and
    /// outline the result takes (and the one trimmed or intersected); if it is not among
    /// them, the topmost one is used. Returns what to select afterwards.
    pub fn shape_objects(&mut self, ids: &[ObjectId], target: ObjectId, op: Shaping) -> Vec<ObjectId> {
        if !self.can_shape(ids) {
            return ids.to_vec();
        }
        let page = self.active_page();
        let picked: Vec<usize> = (0..page.objects.len())
            .filter(|&i| ids.contains(&page.objects[i].id) && !page.objects[i].locked)
            .collect();
        let Some(regions) = picked.iter().map(|&i| region(&page.objects[i])).collect::<Option<Vec<_>>>() else {
            return ids.to_vec();
        };
        let t = picked.iter().position(|&i| page.objects[i].id == target).unwrap_or(picked.len() - 1);
        let union_except = |skip: &dyn Fn(usize) -> bool| {
            (0..regions.len()).filter(|&k| !skip(k)).try_fold(BezPath::new(), |acc, k| boolean(&acc, &regions[k], BinaryOp::Union))
        };
        let picked_ids: Vec<ObjectId> = picked.iter().map(|&i| page.objects[i].id).collect();
        let id_at = |k: usize| picked_ids[k];
        let last = picked.len() - 1;

        match op {
            Shaping::Weld | Shaping::Trim | Shaping::FrontMinusBack | Shaping::BackMinusFront => {
                let keep = match op {
                    Shaping::FrontMinusBack => last,
                    Shaping::BackMinusFront => 0,
                    _ => t,
                };
                let others = union_except(&|k| k == keep);
                let path = others.and_then(|others| match op {
                    Shaping::Weld => boolean(&regions[keep], &others, BinaryOp::Union),
                    _ => boolean(&regions[keep], &others, BinaryOp::Difference),
                });
                let Some(path) = path else { return ids.to_vec() };
                let kept = id_at(keep);
                let removed: Vec<ObjectId> = match op {
                    Shaping::Trim => Vec::new(),
                    _ => (0..picked.len()).filter(|&k| k != keep).map(id_at).collect(),
                };
                let objects = &mut self.active_page_mut().objects;
                objects.retain(|o| !removed.contains(&o.id));
                if path.is_empty() {
                    objects.retain(|o| o.id != kept);
                    return Vec::new();
                }
                if let Some(obj) = objects.iter_mut().find(|o| o.id == kept) {
                    make_curve(obj, path);
                }
                vec![kept]
            }
            Shaping::Simplify => {
                // Work everything out first, so a failure changes nothing.
                let Some(paths) = regions
                    .iter()
                    .enumerate()
                    .map(|(k, region)| union_except(&|j| j <= k).and_then(|front| boolean(region, &front, BinaryOp::Difference)))
                    .collect::<Option<Vec<_>>>()
                else {
                    return ids.to_vec();
                };
                let mut out = Vec::new();
                for (id, path) in picked_ids.iter().zip(paths) {
                    let page = self.active_page_mut();
                    if path.is_empty() {
                        page.objects.retain(|o| o.id != *id);
                    } else if let Some(obj) = page.object_mut(*id) {
                        make_curve(obj, path);
                        out.push(*id);
                    }
                }
                out
            }
            Shaping::Intersect | Shaping::Boundary => {
                let path = if op == Shaping::Intersect {
                    union_except(&|k| k == t).and_then(|others| boolean(&regions[t], &others, BinaryOp::Intersection))
                } else {
                    union_except(&|_| false)
                };
                let Some(path) = path.filter(|p| !p.is_empty()) else { return ids.to_vec() };
                let style = if op == Shaping::Intersect {
                    page_style(&self.active_page().objects[picked[t]])
                } else {
                    Style::new_object()
                };
                let id = self.alloc_object_id();
                let obj = Object {
                    id,
                    name: None,
                    shape: Shape::Curve { path },
                    style,
                    transform: Affine::IDENTITY,
                    locked: false,
                };
                // On top of the selection, so the new object shows.
                self.active_page_mut().objects.insert(picked[last] + 1, obj);
                vec![id]
            }
        }
    }

    /// Whether Combine can run: at least two unlocked objects, none of them a group.
    pub fn can_combine(&self, ids: &[ObjectId]) -> bool {
        let picked: Vec<_> = self.unlocked(ids).collect();
        picked.len() >= 2 && !picked.iter().any(|o| matches!(o.shape, Shape::Group { .. }))
    }

    /// Join the unlocked objects among `ids` into one curve with all their paths, styled
    /// like `target` and in its place. Overlaps become holes (even-odd fill).
    pub fn combine_objects(&mut self, ids: &[ObjectId], target: ObjectId) -> Option<ObjectId> {
        if !self.can_combine(ids) {
            return None;
        }
        let picked: Vec<&Object> = self.unlocked(ids).collect();
        let target = picked.iter().find(|o| o.id == target).unwrap_or(picked.last()?).id;
        let mut path = BezPath::new();
        for obj in &picked {
            path.extend(obj.world_path());
        }
        let removed: Vec<ObjectId> = picked.iter().map(|o| o.id).filter(|id| *id != target).collect();
        let objects = &mut self.active_page_mut().objects;
        objects.retain(|o| !removed.contains(&o.id));
        make_curve(objects.iter_mut().find(|o| o.id == target)?, path);
        Some(target)
    }

    /// Whether Break Apart would change anything: an unlocked curve with several subpaths.
    pub fn can_break_apart(&self, ids: &[ObjectId]) -> bool {
        self.unlocked(ids).any(|o| matches!(&o.shape, Shape::Curve { path } if subpaths(path).len() > 1))
    }

    /// Split each unlocked curve among `ids` into one object per subpath, in place. The
    /// first piece keeps the object's id. Returns the pieces and the other ids, to select.
    pub fn break_apart(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.active_page().objects.len() {
            let obj = &self.active_page().objects[i];
            i += 1;
            if !ids.contains(&obj.id) {
                continue;
            }
            let pieces = match &obj.shape {
                Shape::Curve { path } if !obj.locked => subpaths(path),
                _ => Vec::new(),
            };
            if pieces.len() < 2 {
                out.push(obj.id);
                continue;
            }
            let template = obj.clone();
            let at = i - 1;
            for (k, piece) in pieces.into_iter().enumerate() {
                let id = if k == 0 { template.id } else { self.alloc_object_id() };
                let obj = Object {
                    id,
                    name: if k == 0 { template.name.clone() } else { None },
                    shape: Shape::Curve { path: piece },
                    ..template.clone()
                };
                let objects = &mut self.active_page_mut().objects;
                if k == 0 {
                    objects[at] = obj;
                } else {
                    objects.insert(at + k, obj);
                    i += 1;
                }
                out.push(id);
            }
        }
        out
    }

    fn unlocked<'a>(&'a self, ids: &'a [ObjectId]) -> impl Iterator<Item = &'a Object> + 'a {
        self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && !o.locked)
    }
}

/// Turn `obj` into a page-space curve, keeping how its fill and outline look.
fn make_curve(obj: &mut Object, path: BezPath) {
    obj.style = page_style(obj);
    obj.shape = Shape::Curve { path };
    obj.transform = Affine::IDENTITY;
}

/// The object's style (its first member's, for a group) with any outline that scales with
/// the object fixed at the width it shows now, and any fountain fill placed on the page,
/// since the result has no transform.
fn page_style(obj: &Object) -> Style {
    let leaves = obj.leaves();
    let (mut style, transform) = match leaves.first() {
        Some(leaf) => (leaf.style.clone(), leaf.transform),
        None => (obj.style.clone(), obj.transform),
    };
    if let Some(o) = style.outline.as_mut().filter(|o| o.scale_with_object && o.width > 0.0) {
        o.width = o.page_width(transform);
    }
    // Fountain fills are in local space; the result has none, so move them to the page.
    if let Some(crate::Fill::Gradient(g)) = style.fill.as_mut() {
        g.start = transform * g.start;
        g.end = transform * g.end;
    }
    style
}

/// The page-space area `obj` fills (every member of a group), as non-overlapping
/// contours. `None` if part of it is open or the geometry is invalid.
fn region(obj: &Object) -> Option<BezPath> {
    obj.leaves().iter().try_fold(BezPath::new(), |acc, leaf| {
        leaf.closed.then_some(())?;
        boolean(&acc, &closed(&leaf.path), BinaryOp::Union)
    })
}

/// `a op b` with the even-odd rule the renderer fills with.
fn boolean(a: &BezPath, b: &BezPath, op: BinaryOp) -> Option<BezPath> {
    let mut out = BezPath::new();
    for contour in binary_op(a, b, FillRule::EvenOdd, op).ok()?.contours() {
        // Shapes that touch at a point leave slivers with no area.
        if contour.path.area().abs() > TINY {
            out.extend(tidy(&contour.path).iter());
        }
    }
    Some(out)
}

/// Lengths (pt) and areas (pt²) below this are rounding left over from the booleans.
const TINY: f64 = 1e-6;

/// `path` without zero-length segments and with straight runs joined into one line, so
/// the result has only the nodes it needs.
fn tidy(path: &BezPath) -> BezPath {
    let mut out: Vec<PathEl> = Vec::new();
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => (start, last) = (p, p),
            PathEl::ClosePath => {}
            _ => {
                let end = el.end_point().unwrap_or(last);
                if (end - last).hypot() < TINY {
                    continue;
                }
                if let (PathEl::LineTo(p), Some(PathEl::LineTo(prev))) = (el, out.last().copied()) {
                    let before = out.iter().rev().nth(1).and_then(|e| e.end_point()).unwrap_or(start);
                    let (u, v) = (prev - before, p - prev);
                    if u.cross(v).abs() < TINY * u.hypot().max(v.hypot()) && u.dot(v) > 0.0 {
                        *out.last_mut().unwrap() = PathEl::LineTo(p);
                        last = p;
                        continue;
                    }
                }
                last = end;
            }
        }
        out.push(el);
    }
    // A closing line that lands on the start is drawn by ClosePath anyway.
    if matches!(out.last(), Some(PathEl::ClosePath)) && matches!(out.iter().rev().nth(1), Some(PathEl::LineTo(p)) if (*p - start).hypot() < TINY) {
        out.remove(out.len() - 2);
    }
    BezPath::from_vec(out)
}

/// `path` with every subpath closed (a fill closes them anyway).
fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    for sub in subpaths(path) {
        out.extend(sub.iter());
        if !matches!(sub.elements().last(), Some(PathEl::ClosePath)) {
            out.close_path();
        }
    }
    out
}

/// One path per subpath of `path`.
fn subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for el in path.iter() {
        match (el, out.last_mut()) {
            (PathEl::MoveTo(_), _) | (_, None) => out.push(BezPath::from_vec(vec![el])),
            (el, Some(last)) => last.push(el),
        }
    }
    out.retain(|p| p.elements().len() > 1);
    out
}

#[cfg(test)]
mod tests {
    use kurbo::{Rect, Vec2};

    use super::*;
    use crate::document::Rgba;

    fn style(c: Rgba) -> Style {
        Style::filled(c)
    }

    fn rect(doc: &mut Document, r: Rect, c: Rgba) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, style(c))
    }

    /// Two 10×10 squares overlapping by half: red at the back, white in front.
    fn two() -> (Document, ObjectId, ObjectId) {
        let mut doc = Document::default();
        let back = rect(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0), Rgba::rgb(255, 0, 0));
        let front = rect(&mut doc, Rect::new(5.0, 0.0, 15.0, 10.0), Rgba::WHITE);
        (doc, back, front)
    }

    fn obj(doc: &Document, id: ObjectId) -> &Object {
        doc.active_page().object(id).expect("object exists")
    }

    fn area(doc: &Document, id: ObjectId) -> f64 {
        obj(doc, id).world_path().area().abs()
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn weld_makes_one_curve_styled_like_the_target() {
        let (mut doc, back, front) = two();
        let sel = doc.shape_objects(&[back, front], back, Shaping::Weld);
        assert_eq!(sel, vec![back]);
        assert_eq!(doc.all_object_ids(), vec![back], "the other object is gone");
        let welded = obj(&doc, back);
        assert!(matches!(welded.shape, Shape::Curve { .. }));
        assert_eq!(welded.style.fill, Some(crate::Fill::Solid(Rgba::rgb(255, 0, 0))));
        assert!(near(area(&doc, back), 150.0), "{}", area(&doc, back));
        assert_eq!(welded.world_bounds(), Rect::new(0.0, 0.0, 15.0, 10.0));
    }

    #[test]
    fn weld_joins_shared_edges_and_keeps_curves() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0), Rgba::BLACK);
        let b = rect(&mut doc, Rect::new(10.0, 0.0, 20.0, 10.0), Rgba::BLACK);
        let c = doc.add_object(Shape::Ellipse { rect: Rect::new(15.0, 0.0, 25.0, 10.0), arc: None }, style(Rgba::BLACK));
        doc.shape_objects(&[a, b, c], a, Shaping::Weld);
        let Shape::Curve { path } = &obj(&doc, a).shape else { panic!() };
        let moves = path.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        assert_eq!(moves, 1, "one outline, no seam: {path:?}");
        let lines = path.iter().filter(|e| matches!(e, PathEl::LineTo(_))).count();
        assert!(lines <= 4, "the shared edge leaves no extra nodes: {path:?}");
        assert!(path.iter().any(|e| matches!(e, PathEl::CurveTo(..))), "the ellipse stays a curve");
        let expected = 200.0 + std::f64::consts::PI * 25.0 / 2.0;
        assert!((area(&doc, a) - expected).abs() < 0.05, "{}", area(&doc, a));
    }

    #[test]
    fn weld_of_objects_apart_is_one_object_with_two_paths() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0), Rgba::BLACK);
        let b = rect(&mut doc, Rect::new(50.0, 0.0, 60.0, 10.0), Rgba::BLACK);
        doc.shape_objects(&[a, b], b, Shaping::Weld);
        assert_eq!(doc.all_object_ids(), vec![b]);
        assert!(near(area(&doc, b), 200.0));
    }

    #[test]
    fn trim_cuts_the_target_and_keeps_the_source() {
        let (mut doc, back, front) = two();
        let sel = doc.shape_objects(&[back, front], back, Shaping::Trim);
        assert_eq!(sel, vec![back]);
        assert_eq!(doc.all_object_ids(), vec![back, front]);
        assert!(near(area(&doc, back), 50.0));
        assert!(near(area(&doc, front), 100.0));
        assert_eq!(obj(&doc, back).world_bounds(), Rect::new(0.0, 0.0, 5.0, 10.0));
        assert!(matches!(obj(&doc, front).shape, Shape::Rectangle { .. }), "the source is untouched");
    }

    #[test]
    fn trim_with_a_hole_and_trimming_everything_away() {
        let mut doc = Document::default();
        let big = rect(&mut doc, Rect::new(0.0, 0.0, 30.0, 30.0), Rgba::BLACK);
        let small = rect(&mut doc, Rect::new(10.0, 10.0, 20.0, 20.0), Rgba::WHITE);
        doc.shape_objects(&[big, small], big, Shaping::Trim);
        assert!(near(area(&doc, big), 800.0), "a square hole: {}", area(&doc, big));
        assert!(!obj(&doc, big).hit(kurbo::Point::new(15.0, 15.0), 0.1), "the hole is not filled");

        // The small square sits in the hole, so trimming it by the big one changes nothing.
        doc.shape_objects(&[big, small], small, Shaping::Trim);
        assert!(near(area(&doc, small), 100.0));

        let mut doc = Document::default();
        let big = rect(&mut doc, Rect::new(0.0, 0.0, 30.0, 30.0), Rgba::BLACK);
        let small = rect(&mut doc, Rect::new(10.0, 10.0, 20.0, 20.0), Rgba::WHITE);
        let sel = doc.shape_objects(&[big, small], small, Shaping::Trim);
        assert!(sel.is_empty());
        assert_eq!(doc.all_object_ids(), vec![big], "nothing was left of the target");
    }

    #[test]
    fn intersect_adds_the_overlap_on_top() {
        let (mut doc, back, front) = two();
        let sel = doc.shape_objects(&[back, front], back, Shaping::Intersect);
        assert_eq!(sel.len(), 1);
        let new = sel[0];
        assert_eq!(doc.all_object_ids(), vec![back, front, new]);
        assert!(near(area(&doc, new), 50.0));
        assert_eq!(obj(&doc, new).style.fill, Some(crate::Fill::Solid(Rgba::rgb(255, 0, 0))), "the target's colour");

        let mut apart = Document::default();
        let a = rect(&mut apart, Rect::new(0.0, 0.0, 1.0, 1.0), Rgba::BLACK);
        let b = rect(&mut apart, Rect::new(5.0, 0.0, 6.0, 1.0), Rgba::BLACK);
        assert_eq!(apart.shape_objects(&[a, b], a, Shaping::Intersect), vec![a, b], "no overlap: no change");
        assert_eq!(apart.all_object_ids().len(), 2);
    }

    #[test]
    fn simplify_and_front_or_back_minus() {
        let (mut doc, back, front) = two();
        doc.shape_objects(&[back, front], front, Shaping::Simplify);
        assert!(near(area(&doc, back), 50.0) && near(area(&doc, front), 100.0));

        let (mut doc, back, front) = two();
        assert_eq!(doc.shape_objects(&[back, front], back, Shaping::FrontMinusBack), vec![front]);
        assert_eq!(doc.all_object_ids(), vec![front]);
        assert_eq!(obj(&doc, front).world_bounds(), Rect::new(10.0, 0.0, 15.0, 10.0));

        let (mut doc, back, front) = two();
        assert_eq!(doc.shape_objects(&[back, front], front, Shaping::BackMinusFront), vec![back]);
        assert_eq!(obj(&doc, back).world_bounds(), Rect::new(0.0, 0.0, 5.0, 10.0));
    }

    #[test]
    fn boundary_adds_an_unfilled_outline_around_everything() {
        let (mut doc, back, front) = two();
        let sel = doc.shape_objects(&[back, front], back, Shaping::Boundary);
        let new = sel[0];
        assert_eq!(doc.all_object_ids(), vec![back, front, new]);
        assert_eq!(obj(&doc, new).style, Style::new_object());
        assert_eq!(obj(&doc, new).world_bounds(), Rect::new(0.0, 0.0, 15.0, 10.0));
    }

    #[test]
    fn shaping_works_on_transformed_shapes_and_groups() {
        let (mut doc, back, front) = two();
        doc.transform_objects(&[front], Affine::translate(Vec2::new(0.0, 5.0)));
        let extra = rect(&mut doc, Rect::new(100.0, 0.0, 110.0, 10.0), Rgba::BLACK);
        let group = doc.group_objects(&[front, extra]).unwrap();
        doc.shape_objects(&[back, group], back, Shaping::Trim);
        assert!(near(area(&doc, back), 75.0), "{}", area(&doc, back));
    }

    #[test]
    fn open_or_locked_objects_are_left_out() {
        let (mut doc, back, front) = two();
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((20.0, 20.0));
        let open = doc.add_object(Shape::Curve { path: line }, Style::new_object());
        assert!(!doc.can_shape(&[back, front, open]), "an open curve has no area");
        assert!(doc.can_shape(&[back, front]));
        doc.set_locked(&[front], true);
        assert!(!doc.can_shape(&[back, front]));
        assert_eq!(doc.shape_objects(&[back, front], back, Shaping::Weld), vec![back, front]);
        assert_eq!(doc.all_object_ids().len(), 3);
    }

    #[test]
    fn scaled_outline_keeps_its_width() {
        let (mut doc, back, front) = two();
        doc.restyle_objects(&[back], |s| s.outline = Some(crate::Outline { scale_with_object: true, ..crate::Outline::new(Rgba::BLACK, 1.0) }));
        doc.transform_objects(&[back], Affine::scale(2.0));
        doc.shape_objects(&[back, front], back, Shaping::Weld);
        assert!(near(obj(&doc, back).style.outline.as_ref().unwrap().width, 2.0));
    }

    #[test]
    fn combine_keeps_every_path_and_overlaps_become_holes() {
        let mut doc = Document::default();
        let big = rect(&mut doc, Rect::new(0.0, 0.0, 30.0, 30.0), Rgba::BLACK);
        let small = rect(&mut doc, Rect::new(10.0, 10.0, 20.0, 20.0), Rgba::WHITE);
        assert!(doc.can_combine(&[big, small]));
        assert_eq!(doc.combine_objects(&[big, small], small), Some(small));
        assert_eq!(doc.all_object_ids(), vec![small], "in the target's place");
        let combined = obj(&doc, small);
        assert_eq!(combined.style.fill, Some(crate::Fill::Solid(Rgba::WHITE)));
        assert!(combined.hit(kurbo::Point::new(5.0, 5.0), 0.1));
        assert!(!combined.hit(kurbo::Point::new(15.0, 15.0), 0.1), "the overlap is a hole");

        assert!(doc.can_break_apart(&[small]));
        let pieces = doc.break_apart(&[small]);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], small);
        assert_eq!(doc.all_object_ids(), pieces);
        assert!(near(area(&doc, pieces[0]), 900.0) && near(area(&doc, pieces[1]), 100.0));
        assert!(pieces.iter().all(|id| obj(&doc, *id).style.fill == Some(crate::Fill::Solid(Rgba::WHITE))));
        assert!(!doc.can_break_apart(&pieces));
    }

    #[test]
    fn combine_needs_two_objects_and_no_group() {
        let (mut doc, back, front) = two();
        assert!(!doc.can_combine(&[back]));
        let group = doc.group_objects(&[back, front]).unwrap();
        let other = rect(&mut doc, Rect::new(0.0, 0.0, 1.0, 1.0), Rgba::BLACK);
        assert!(!doc.can_combine(&[group, other]));
        assert_eq!(doc.combine_objects(&[group, other], other), None);
    }
}

//! Join Curves and Convert Outline to Object.

use kurbo::{Affine, BezPath, PathEl, Point, Shape as _, StrokeOpts};
use linesweeper::{BinaryOp, FillRule, binary_op};

use crate::document::{Document, Object, ObjectId, Shape, Style};
use crate::fill::Fill;
use crate::shaping::{TINY, make_curve, subpaths, tidy};

/// Default gap Join Curves closes: 1 mm, in points.
pub const JOIN_TOLERANCE: f64 = 72.0 / 25.4;

/// How closely stroked outlines follow the curve, in points.
const STROKE_ACCURACY: f64 = 0.01;

impl Document {
    /// Whether Join Curves would change anything: two or more unlocked objects (none a
    /// group or bitmap) to merge, or open ends within `tolerance` of each other.
    pub fn can_join_curves(&self, ids: &[ObjectId], tolerance: f64) -> bool {
        let picked: Vec<&Object> = self.unlocked(ids).collect();
        if picked.is_empty() || picked.iter().any(|o| matches!(o.shape, Shape::Group { .. } | Shape::Image(_))) {
            return false;
        }
        if picked.len() >= 2 {
            return true;
        }
        let open: Vec<BezPath> = subpaths(&picked[0].world_path()).into_iter().filter(|p| !is_closed(p)).collect();
        closest_ends(&open, tolerance).is_some()
    }

    /// Merge the unlocked objects among `ids` into one curve in `target`'s place (the
    /// topmost one if `target` is not among them), joining open ends that lie within
    /// `tolerance` of each other. Ends meet halfway; a path whose own ends meet closes.
    pub fn join_curves(&mut self, ids: &[ObjectId], target: ObjectId, tolerance: f64) -> Option<ObjectId> {
        if !self.can_join_curves(ids, tolerance) {
            return None;
        }
        let picked: Vec<&Object> = self.unlocked(ids).collect();
        let target = picked.iter().find(|o| o.id == target).unwrap_or(picked.last()?).id;
        let (mut open, mut closed) = (Vec::new(), Vec::new());
        for obj in &picked {
            for sub in subpaths(&obj.world_path()) {
                if is_closed(&sub) { closed.push(sub) } else { open.push(sub) }
            }
        }
        while let Some((a, a_end, b, b_end)) = closest_ends(&open, tolerance) {
            if a == b {
                let mut sub = open.swap_remove(a);
                close_onto_start(&mut sub);
                closed.push(sub);
                continue;
            }
            // Orient both so that a's end meets b's start, then append b to a.
            let mut first = open[a].clone();
            let mut second = open[b].clone();
            if !a_end {
                first = first.reverse_subpaths();
            }
            if b_end {
                second = second.reverse_subpaths();
            }
            let meet = end_of(&first).midpoint(start_of(&second));
            set_end(&mut first, meet);
            first.extend(second.iter().skip(1));
            let (hi, lo) = (a.max(b), a.min(b));
            open.swap_remove(hi);
            open[lo] = first;
        }
        let mut path = BezPath::new();
        for sub in closed.iter().chain(&open) {
            path.extend(tidy(sub).iter());
        }
        let removed: Vec<ObjectId> = picked.iter().map(|o| o.id).filter(|id| *id != target).collect();
        let objects = &mut self.active_page_mut().objects;
        objects.retain(|o| !removed.contains(&o.id));
        make_curve(objects.iter_mut().find(|o| o.id == target)?, path);
        Some(target)
    }

    /// Whether any unlocked, non-group object among `ids` has an outline.
    pub fn can_convert_outline(&self, ids: &[ObjectId]) -> bool {
        self.unlocked(ids).any(|o| !matches!(o.shape, Shape::Group { .. }) && o.style.outline.is_some())
    }

    /// Turn the outline of each unlocked, non-group object among `ids` into a filled curve
    /// of the same colour, just above the object (below it for an outline behind the
    /// fill). The object loses its outline, or goes if it showed no fill. Returns the new
    /// curves, to select.
    pub fn convert_outline(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.active_page().objects.len() {
            let obj = &self.active_page().objects[i];
            i += 1;
            if !ids.contains(&obj.id) || obj.locked || matches!(obj.shape, Shape::Group { .. }) {
                continue;
            }
            let Some(outline) = obj.style.outline.clone() else { continue };
            let leaves = obj.leaves();
            let Some(leaf) = leaves.first() else { continue };
            let shows_fill = leaf.fill().is_some();
            let stroked = kurbo::stroke(
                leaf.path.iter(),
                &outline.stroke(outline.page_width(leaf.transform)),
                &StrokeOpts::default(),
                STROKE_ACCURACY,
            );
            let Some(path) = nonzero_region(&stroked).filter(|p| !p.is_empty()) else { continue };
            // The outline's overprint becomes the new curve's fill overprint.
            let style = Style { fill: Some(Fill::Solid(outline.color)), transparency: obj.style.transparency, overprint_fill: obj.style.overprint_outline, ..Style::default() };
            let layer = obj.layer;
            let id = self.alloc_object_id();
            let curve = Object { id, name: None, shape: Shape::Curve { path }, style, transform: Affine::IDENTITY, locked: false, layer, contents: Vec::new(), clone_of: None, frame: false, symbol: None, symmetry: None, perspective: None, table: None };
            let at = i - 1;
            let objects = &mut self.active_page_mut().objects;
            if !shows_fill {
                objects[at] = curve;
            } else {
                objects[at].style.outline = None;
                objects.insert(if outline.behind_fill { at } else { at + 1 }, curve);
                i += 1;
            }
            out.push(id);
        }
        out
    }
}

/// The area `path` covers under the non-zero rule, as non-overlapping contours that the
/// renderer's even-odd fill draws the same.
pub fn nonzero_region(path: &BezPath) -> Option<BezPath> {
    let mut out = BezPath::new();
    for contour in binary_op(path, &BezPath::new(), FillRule::NonZero, BinaryOp::Union).ok()?.contours() {
        if contour.path.area().abs() > TINY {
            out.extend(tidy(&contour.path).iter());
        }
    }
    Some(out)
}

fn is_closed(sub: &BezPath) -> bool {
    matches!(sub.elements().last(), Some(PathEl::ClosePath))
}

fn start_of(sub: &BezPath) -> Point {
    match sub.elements().first() {
        Some(PathEl::MoveTo(p)) => *p,
        _ => Point::ZERO,
    }
}

fn end_of(sub: &BezPath) -> Point {
    sub.elements().last().and_then(|e| e.end_point()).unwrap_or_default()
}

/// Move the last node of `sub` to `p`.
fn set_end(sub: &mut BezPath, p: Point) {
    let mut els = sub.elements().to_vec();
    if let Some(PathEl::MoveTo(q) | PathEl::LineTo(q) | PathEl::QuadTo(_, q) | PathEl::CurveTo(_, _, q)) = els.last_mut() {
        *q = p;
    }
    *sub = BezPath::from_vec(els);
}

/// Close `sub` with its ends meeting halfway.
fn close_onto_start(sub: &mut BezPath) {
    let meet = start_of(sub).midpoint(end_of(sub));
    set_end(sub, meet);
    let mut els = sub.elements().to_vec();
    els[0] = PathEl::MoveTo(meet);
    els.push(PathEl::ClosePath);
    *sub = BezPath::from_vec(els);
}

/// The two open ends closest together within `tolerance`: (path, its end?, path, its
/// end?). A path's own two ends count only if it has more than one segment.
fn closest_ends(open: &[BezPath], tolerance: f64) -> Option<(usize, bool, usize, bool)> {
    let ends = |k: usize| [(false, start_of(&open[k])), (true, end_of(&open[k]))];
    let mut best: Option<(f64, (usize, bool, usize, bool))> = None;
    for a in 0..open.len() {
        for b in a..open.len() {
            if a == b && open[a].segments().count() < 2 {
                continue;
            }
            for (a_end, pa) in ends(a) {
                for (b_end, pb) in ends(b) {
                    if a == b && (a_end || !b_end) {
                        continue;
                    }
                    let d = (pa - pb).hypot();
                    if d <= tolerance && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, (a, a_end, b, b_end)));
                    }
                }
            }
        }
    }
    best.map(|(_, found)| found)
}

#[cfg(test)]
mod tests {
    use kurbo::Rect;

    use super::*;
    use crate::document::{Outline, Rgba};

    fn line(doc: &mut Document, pts: &[(f64, f64)]) -> ObjectId {
        let mut path = BezPath::new();
        path.move_to(pts[0]);
        for p in &pts[1..] {
            path.line_to(*p);
        }
        doc.add_object(Shape::Curve { path }, Style::outlined(Outline::new(Rgba::BLACK, 1.0)))
    }

    fn curve(doc: &Document, id: ObjectId) -> BezPath {
        match &doc.active_page().object(id).expect("exists").shape {
            Shape::Curve { path } => path.clone(),
            other => panic!("not a curve: {other:?}"),
        }
    }

    #[test]
    fn join_curves_links_nearby_ends_and_closes_a_loop() {
        let mut doc = Document::default();
        let a = line(&mut doc, &[(0.0, 0.0), (100.0, 0.0)]);
        // Drawn the other way round, with its end 0.5 pt short.
        let b = line(&mut doc, &[(100.0, 100.0), (100.0, 0.5)]);
        let c = line(&mut doc, &[(100.0, 100.0), (0.0, 0.0)]);
        assert!(doc.can_join_curves(&[a, b, c], JOIN_TOLERANCE));
        assert_eq!(doc.join_curves(&[a, b, c], a, JOIN_TOLERANCE), Some(a));
        assert_eq!(doc.all_object_ids(), vec![a], "one curve is left");
        let path = curve(&doc, a);
        assert_eq!(subpaths(&path).len(), 1, "{path:?}");
        assert!(is_closed(&path), "the loop closed: {path:?}");
        assert!((path.area().abs() - 5000.0).abs() < 30.0, "{}", path.area());
        assert!(path.iter().any(|e| e.end_point() == Some(Point::new(100.0, 0.25))), "ends meet halfway: {path:?}");
    }

    #[test]
    fn join_curves_leaves_far_ends_open() {
        let mut doc = Document::default();
        let a = line(&mut doc, &[(0.0, 0.0), (100.0, 0.0)]);
        let b = line(&mut doc, &[(0.0, 50.0), (100.0, 50.0)]);
        assert!(!doc.can_join_curves(&[a], JOIN_TOLERANCE), "one open line has nothing to join");
        doc.join_curves(&[a, b], b, JOIN_TOLERANCE);
        assert_eq!(doc.all_object_ids(), vec![b]);
        let path = curve(&doc, b);
        assert_eq!(subpaths(&path).len(), 2);
        assert!(!is_closed(&path));
    }

    #[test]
    fn convert_outline_makes_a_filled_ring_above_the_object() {
        let mut doc = Document::default();
        let mut style = Style::filled(Rgba::WHITE);
        style.outline = Some(Outline::new(Rgba::rgb(255, 0, 0), 10.0));
        style.transparency = 0.5;
        let r = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 100.0, 100.0), corner_radius: 0.0 }, style);
        assert!(doc.can_convert_outline(&[r]));
        let made = doc.convert_outline(&[r]);
        assert_eq!(made.len(), 1);
        assert_eq!(doc.all_object_ids(), vec![r, made[0]], "the ring sits above the object");
        let page = doc.active_page();
        assert_eq!(page.object(r).unwrap().style.outline, None, "the object keeps only its fill");
        let ring = page.object(made[0]).unwrap();
        assert_eq!(ring.style.fill, Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
        assert_eq!((ring.style.outline.as_ref(), ring.style.transparency), (None, 0.5));
        let area = ring.world_path().area().abs();
        assert!((area - (110.0 * 110.0 - 90.0 * 90.0)).abs() < 1.0, "{area}");
        assert_eq!(ring.world_bounds(), Rect::new(-5.0, -5.0, 105.0, 105.0));
    }

    #[test]
    fn convert_outline_replaces_an_unfilled_object_and_respects_behind_fill() {
        let mut doc = Document::default();
        let open = line(&mut doc, &[(0.0, 0.0), (100.0, 0.0)]);
        let mut style = Style::filled(Rgba::WHITE);
        let mut o = Outline::new(Rgba::BLACK, 4.0);
        o.behind_fill = true;
        style.outline = Some(o);
        let behind = doc.add_object(Shape::Ellipse { rect: Rect::new(0.0, 10.0, 50.0, 60.0), arc: None }, style);
        let made = doc.convert_outline(&[open, behind]);
        assert_eq!(made.len(), 2);
        assert_eq!(doc.all_object_ids(), vec![made[0], made[1], behind], "the line is replaced; the ellipse ring goes under it");
        let bar = doc.active_page().object(made[0]).unwrap().world_path();
        assert!((bar.area().abs() - 100.0).abs() < 0.5, "a 100 × 1 bar: {}", bar.area());
    }
}

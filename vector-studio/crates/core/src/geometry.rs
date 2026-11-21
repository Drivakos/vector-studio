//! Shape outlines, bounds and hit-testing.

use std::f64::consts::{FRAC_PI_2, TAU};

use kurbo::{
    Affine, BezPath, Ellipse, ParamCurveNearest, Point, Rect, RoundedRect, Shape as _, Vec2,
};

use crate::document::{ArcKind, EllipseArc, Object, ObjectId, Page, Shape, Style};

/// Flattening tolerance used when converting analytic shapes to Béziers, in points.
pub const PATH_TOLERANCE: f64 = 0.01;

impl Shape {
    /// The outline in local space.
    pub fn to_path(&self) -> BezPath {
        match self {
            Shape::Rectangle { rect, corner_radius } => {
                let r = corner_radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
                if r > 0.0 {
                    RoundedRect::from_rect(*rect, r).to_path(PATH_TOLERANCE)
                } else {
                    rect.to_path(PATH_TOLERANCE)
                }
            }
            Shape::Ellipse { rect, arc: None } => Ellipse::from_rect(*rect).to_path(PATH_TOLERANCE),
            Shape::Ellipse { rect, arc: Some(arc) } => arc_path(*rect, arc),
            Shape::Polygon { rect, sides } => polygon_path(*rect, (*sides).max(3), None),
            Shape::Star { rect, points, inner_ratio } => {
                polygon_path(*rect, (*points).max(3), Some(inner_ratio.clamp(0.01, 1.0)))
            }
            Shape::Curve { path } => path.clone(),
            Shape::Text(text) => text.to_path(),
            Shape::Image(b) => b.rect.to_path(PATH_TOLERANCE),
            Shape::Group { children } => {
                let mut path = BezPath::new();
                for child in children {
                    path.extend(child.world_path());
                }
                path
            }
        }
    }

    /// The rectangle analytic shapes are defined by; for curves, their bounds.
    pub fn frame(&self) -> Rect {
        match self {
            Shape::Rectangle { rect, .. }
            | Shape::Ellipse { rect, .. }
            | Shape::Polygon { rect, .. }
            | Shape::Star { rect, .. } => *rect,
            Shape::Curve { path } => path.bounding_box(),
            Shape::Text(text) => text.layout().bounds(),
            Shape::Image(b) => b.rect,
            Shape::Group { .. } => self.to_path().bounding_box(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Shape::Curve { path } => path
                .elements()
                .iter()
                .any(|el| matches!(el, kurbo::PathEl::ClosePath)),
            Shape::Ellipse { arc: Some(arc), .. } => arc.kind == ArcKind::Pie,
            _ => true,
        }
    }
}

/// Point on the ellipse inscribed in `rect` at `degrees`, counterclockwise on the page
/// from 3 o'clock.
pub fn ellipse_point(rect: Rect, degrees: f64) -> Point {
    let (c, a) = (rect.center(), degrees.to_radians());
    Point::new(c.x + rect.width() / 2.0 * a.cos(), c.y - rect.height() / 2.0 * a.sin())
}

fn arc_path(rect: Rect, arc: &EllipseArc) -> BezPath {
    let c = rect.center();
    // kurbo measures angles clockwise on the page (y points down).
    let curve = kurbo::Arc::new(
        c,
        (rect.width() / 2.0, rect.height() / 2.0),
        -arc.start.to_radians(),
        -arc.sweep().to_radians(),
        0.0,
    );
    let mut path = BezPath::new();
    match arc.kind {
        ArcKind::Pie => {
            path.move_to(c);
            path.line_to(ellipse_point(rect, arc.start));
            path.extend(curve.append_iter(PATH_TOLERANCE));
            path.close_path();
        }
        ArcKind::Arc => path.extend(curve.path_elements(PATH_TOLERANCE)),
    }
    path
}

/// Vertices on an ellipse inscribed in `rect`, starting at the top, clockwise on screen.
/// With `inner`, every other vertex sits at that fraction of the radius (a star).
pub fn polygon_path(rect: Rect, n: u32, inner: Option<f64>) -> BezPath {
    let c = rect.center();
    let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
    let count = if inner.is_some() { n * 2 } else { n };
    let mut path = BezPath::new();
    for i in 0..count {
        let angle = -FRAC_PI_2 + TAU * f64::from(i) / f64::from(count);
        let k = match inner {
            Some(ratio) if i % 2 == 1 => ratio,
            _ => 1.0,
        };
        let p = Point::new(c.x + rx * k * angle.cos(), c.y + ry * k * angle.sin());
        if i == 0 { path.move_to(p) } else { path.line_to(p) }
    }
    path.close_path();
    path
}

/// A drawable (non-group) object with its outline already in page space.
pub struct Leaf<'a> {
    pub path: BezPath,
    pub shape: &'a Shape,
    /// The object's style; a PowerClip container is split into a fill-only leaf under its
    /// contents and an outline-only leaf over them.
    pub style: std::borrow::Cow<'a, Style>,
    pub closed: bool,
    /// Local → page transform, groups included.
    pub transform: Affine,
    /// PowerClip outlines (page space, even-odd) this leaf is drawn inside, outermost first.
    pub clips: Vec<BezPath>,
    /// Drawn through a perspective: `path` and `clips` are projected, `transform` only
    /// places fills and outline widths roughly.
    pub projected: bool,
    /// A drop shadow's bitmap: drawn, but not hit, measured or shaped.
    pub shadow: bool,
    /// Drawn by an effect (contour, blend steps, extrusion, block shadow, mesh): hit with
    /// its object, but not measured or shaped.
    pub effect: bool,
}

impl Leaf<'_> {
    /// The fill, if the leaf is closed, has one and is not a bitmap.
    pub fn fill(&self) -> Option<&crate::Fill> {
        self.style.fill.as_ref().filter(|_| self.closed && !matches!(self.shape, Shape::Image(_)))
    }

    /// Whether `point` (page space) is inside every clip, so the leaf can show there.
    pub fn shows_at(&self, point: Point) -> bool {
        self.clips.iter().all(|c| c.winding(point) % 2 != 0)
    }
}

impl Object {
    /// Everything this object draws, bottom first: itself, or its group's members, with
    /// any PowerClip contents between a container's fill and its outline.
    pub fn leaves(&self) -> Vec<Leaf<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(Affine::IDENTITY, &[], &mut out);
        out.retain(|l| !l.shadow && !l.effect);
        out
    }

    /// [`Self::leaves`] with what effects draw: drop shadows and the rest.
    pub fn drawn_leaves(&self) -> Vec<Leaf<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(Affine::IDENTITY, &[], &mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, parent: Affine, clips: &[BezPath], out: &mut Vec<Leaf<'a>>) {
        if let Some(shape @ Shape::Image(b)) = self.shadow.as_ref().and_then(|s| s.shape()) {
            let mut path = b.rect.to_path(PATH_TOLERANCE);
            path.apply_affine(parent);
            let style = std::borrow::Cow::Owned(Style { fill: None, outline: None, ..Style::default() });
            out.push(Leaf { path, shape, style, closed: true, transform: parent, clips: clips.to_vec(), projected: false, shadow: true, effect: false });
        }
        collect_effect(self.effects_under(), parent, clips, out);
        let transform = parent * self.transform;
        let Some(warp) = self.warp() else {
            self.collect_plain(transform, clips, out);
            collect_effect(self.effects_over(), parent, clips, out);
            return;
        };
        // Everything in the object's own space, then through the reshaping effects.
        let start = out.len();
        self.collect_plain(Affine::IDENTITY, &[], out);
        let project = |path: &BezPath| {
            let mut p = warp.map_path(path);
            p.apply_affine(transform);
            p
        };
        for leaf in &mut out[start..] {
            leaf.path = project(&leaf.path);
            let own: Vec<BezPath> = leaf.clips.iter().map(project).collect();
            leaf.clips = clips.iter().cloned().chain(own).collect();
            leaf.transform = transform * leaf.transform;
            leaf.projected = true;
        }
        collect_effect(self.effects_over(), parent, clips, out);
    }

    /// [`Self::collect_leaves`] without this object's own perspective; `transform` is
    /// its local → page transform.
    fn collect_plain<'a>(&'a self, transform: Affine, clips: &[BezPath], out: &mut Vec<Leaf<'a>>) {
        use std::borrow::Cow;
        match &self.shape {
            Shape::Group { children } => {
                for at in self.member_placements(transform) {
                    for (i, c) in children.iter().enumerate() {
                        c.collect_leaves(at, clips, out);
                        // A blend's steps go between its two ends.
                        if i == 0 {
                            collect_effect(self.blend_steps().iter(), at, clips, out);
                        }
                    }
                }
            }
            shape => {
                // An Artistic Media stroke is drawn in place of its control curve.
                let (mut path, closed) = match self.media_outline() {
                    Some(stroke) => (stroke, true),
                    None => (shape.to_path(), shape.is_closed()),
                };
                path.apply_affine(transform);
                let leaf = |style: Cow<'a, Style>, path: BezPath| Leaf { path, shape, style, closed, transform, clips: clips.to_vec(), projected: false, shadow: false, effect: false };
                // A mesh fill is drawn as a bitmap inside the outline, in place of the fill.
                let mesh = self.effects.mesh.as_ref().and_then(|m| m.shape()).filter(|_| shape.is_closed() && !self.is_warped() && self.effects.media.is_none());
                if let (Some(image @ Shape::Image(b)), true) = (mesh, self.contents.is_empty()) {
                    let mut area = b.rect.to_path(PATH_TOLERANCE);
                    area.apply_affine(transform);
                    let mut inside = clips.to_vec();
                    inside.push(path.clone());
                    let style = Cow::Owned(Style { fill: None, outline: None, transparency: self.style.transparency, ..Style::default() });
                    out.push(Leaf { path: area, shape: image, style, closed: true, transform, clips: inside, projected: false, shadow: false, effect: true });
                    out.push(leaf(Cow::Owned(Style { fill: None, ..self.style.clone() }), path));
                    return;
                }
                if self.contents.is_empty() {
                    out.push(leaf(Cow::Borrowed(&self.style), path));
                    return;
                }
                out.push(leaf(Cow::Owned(Style { outline: None, ..self.style.clone() }), path.clone()));
                let mut inner = clips.to_vec();
                inner.push(path.clone());
                for content in &self.contents {
                    content.collect_leaves(transform, &inner, out);
                }
                if self.style.outline.is_some() {
                    out.push(leaf(Cow::Owned(Style { fill: None, ..self.style.clone() }), path));
                }
            }
        }
    }

    /// Where a group's members are drawn, given the group's own placement: once, plus
    /// each mirrored copy of a symmetry group.
    pub fn member_placements(&self, transform: Affine) -> Vec<Affine> {
        let copies = self.symmetry.map(|s| s.copies()).unwrap_or_default();
        std::iter::once(transform).chain(copies.into_iter().map(|m| transform * m)).collect()
    }

    /// The outline in page space.
    pub fn world_path(&self) -> BezPath {
        let mut path = self.shape.to_path();
        if let Some(w) = self.warp() {
            path = w.map_path(&path);
        }
        path.apply_affine(self.transform);
        path
    }

    /// Geometric bounds in page space (outline width not included). Text counts its
    /// whole line boxes, so empty text and spaces still have bounds.
    pub fn world_bounds(&self) -> Rect {
        self.bounds_in(Affine::IDENTITY)
    }

    fn bounds_in(&self, parent: Affine) -> Rect {
        let transform = parent * self.transform;
        if !self.is_warped() {
            return self.plain_bounds(transform);
        }
        let mut out = Vec::new();
        self.collect_leaves(parent, &[], &mut out);
        out.iter().map(|l| l.path.bounding_box()).reduce(|a, b| a.union(b)).unwrap_or_default()
    }

    /// Bounds without this object's own perspective, `transform` being its local → page
    /// transform.
    pub(crate) fn plain_bounds(&self, transform: Affine) -> Rect {
        match &self.shape {
            Shape::Group { children } => self
                .member_placements(transform)
                .into_iter()
                .flat_map(|at| children.iter().map(move |c| c.bounds_in(at)))
                .reduce(|a, b| a.union(b))
                .unwrap_or_default(),
            Shape::Text(text) => transform.transform_rect_bbox(text.layout().bounds()),
            shape => {
                let mut path = self.media_outline().unwrap_or_else(|| shape.to_path());
                path.apply_affine(transform);
                path.bounding_box()
            }
        }
    }

    /// Whether `point` (page space) hits the fill or comes within `tolerance` of the outline
    /// of this object or, for a group, of any object in it.
    pub fn hit(&self, point: Point, tolerance: f64) -> bool {
        self.hit_in(Affine::IDENTITY, point, tolerance)
    }

    /// [`Self::hit`] for an object placed by `parent` (its groups' transforms).
    pub fn hit_in(&self, parent: Affine, point: Point, tolerance: f64) -> bool {
        let mut leaves = Vec::new();
        self.collect_leaves(parent, &[], &mut leaves);
        leaves.iter().filter(|leaf| !leaf.shadow && leaf.shows_at(point)).any(|leaf| {
            let half = leaf.style.outline.as_ref().map_or(0.0, |o| o.page_width(leaf.transform) / 2.0);
            let reach = tolerance + half;
            // Text is picked anywhere in its line boxes, not just on the glyphs; bitmaps
            // anywhere on them.
            let solid = match leaf.shape {
                _ if leaf.projected => None,
                Shape::Text(text) => Some(text.layout().bounds()),
                Shape::Image(b) => Some(b.rect),
                _ => None,
            };
            if let Some(area) = solid {
                let local = leaf.transform.inverse() * point;
                let pad = tolerance / leaf.transform.determinant().abs().sqrt().max(1e-9);
                if area.inflate(pad, pad).contains(local) {
                    return true;
                }
            }
            if !leaf.path.bounding_box().inflate(reach, reach).contains(point) {
                return false;
            }
            // Even-odd, like the renderer.
            if leaf.fill().is_some() && leaf.path.winding(point) % 2 != 0 {
                return true;
            }
            leaf.path.segments().any(|seg| seg.nearest(point, 1e-6).distance_sq <= reach * reach)
        })
    }
}

/// Add the leaves of objects an effect draws (placed by `parent`), marked as such.
fn collect_effect<'a>(objects: impl Iterator<Item = &'a Object>, parent: Affine, clips: &[BezPath], out: &mut Vec<Leaf<'a>>) {
    let start = out.len();
    for o in objects {
        o.collect_leaves(parent, clips, out);
    }
    for leaf in &mut out[start..] {
        leaf.effect = true;
    }
}

/// The colour shown at a point, as the Color Eyedropper picks it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintHit {
    /// The top-level object that shows the colour.
    pub id: ObjectId,
    pub color: crate::Rgba,
    /// The colour is the outline's, not the fill's.
    pub outline: bool,
}

impl Object {
    /// A group whose members can be edited one by one in place: not a table, symbol,
    /// clone, symmetry group or a group reshaped by an effect (a perspective, envelope or
    /// distortion), whose members are laid out or drawn by something else.
    fn opens(&self) -> bool {
        matches!(self.shape, Shape::Group { .. })
            && self.table.is_none()
            && self.symbol.is_none()
            && self.clone_of.is_none()
            && self.symmetry.is_none()
            && !self.is_warped()
    }

    /// Object `id`: this one or one in it (see [`Self::opens`]), with the transform that
    /// places it, its groups' transforms put together; `parent` places this object.
    pub fn find(&self, id: ObjectId, parent: Affine) -> Option<(&Object, Affine)> {
        if self.id == id {
            return Some((self, parent));
        }
        match &self.shape {
            Shape::Group { children } if self.opens() => children.iter().find_map(|c| c.find(id, parent * self.transform)),
            _ => None,
        }
    }

    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        if self.id == id {
            return Some(self);
        }
        let opens = self.opens();
        match &mut self.shape {
            Shape::Group { children } if opens => children.iter_mut().find_map(|c| c.find_mut(id)),
            _ => None,
        }
    }

    /// The topmost object under `point` that is not a group this one opens into: this
    /// object, or one in it. `parent` places this object.
    pub fn member_at(&self, parent: Affine, point: Point, tolerance: f64) -> Option<ObjectId> {
        match &self.shape {
            Shape::Group { children } if self.opens() => {
                children.iter().rev().find_map(|c| c.member_at(parent * self.transform, point, tolerance))
            }
            _ => self.hit_in(parent, point, tolerance).then_some(self.id),
        }
    }
}

impl Page {
    /// The topmost fill or outline colour at `point`, within `tolerance` of outlines.
    pub fn paint_at(&self, point: Point, tolerance: f64) -> Option<PaintHit> {
        for obj in self.objects.iter().rev().filter(|o| self.is_selectable(o)) {
            for leaf in obj.leaves().iter().rev().filter(|l| l.shows_at(point)) {
                if let Some(o) = &leaf.style.outline {
                    let reach = tolerance + o.page_width(leaf.transform) / 2.0;
                    if leaf.path.segments().any(|seg| seg.nearest(point, 1e-6).distance_sq <= reach * reach) {
                        return Some(PaintHit { id: obj.id, color: o.color, outline: true });
                    }
                }
                if let Some(fill) = leaf.fill()
                    && leaf.path.winding(point) % 2 != 0
                {
                    let color = fill.color_at(leaf.transform.inverse() * point);
                    return Some(PaintHit { id: obj.id, color, outline: false });
                }
            }
        }
        None
    }

    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    /// Object `id`, at the top or inside groups, with the transform of the groups it is in.
    pub fn find(&self, id: ObjectId) -> Option<(&Object, Affine)> {
        self.objects.iter().find_map(|o| o.find(id, Affine::IDENTITY))
    }

    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find_map(|o| o.find_mut(id))
    }

    /// Topmost object under `point`.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<ObjectId> {
        self.objects.iter().rev().find(|o| self.is_selectable(o) && o.hit(point, tolerance)).map(|o| o.id)
    }

    /// Selectable objects whose bounds lie fully inside `rect`, bottom first.
    pub fn objects_inside(&self, rect: Rect) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| self.is_selectable(o))
            .filter(|o| {
                let b = o.world_bounds();
                rect.contains(b.origin()) && rect.contains(Point::new(b.x1, b.y1))
            })
            .map(|o| o.id)
            .collect()
    }

    /// Selectable objects drawn wholly inside the closed loop through `lasso` (page
    /// space), bottom first: every point of their outlines is inside it.
    pub fn objects_in_lasso(&self, lasso: &[Point]) -> Vec<ObjectId> {
        if lasso.len() < 3 {
            return Vec::new();
        }
        let mut loop_path = BezPath::new();
        loop_path.move_to(lasso[0]);
        lasso[1..].iter().for_each(|p| loop_path.line_to(*p));
        loop_path.close_path();
        let inside = |p: Point| kurbo::Shape::winding(&loop_path, p) != 0;
        self.objects
            .iter()
            .filter(|o| self.is_selectable(o))
            .filter(|o| {
                let mut points = Vec::new();
                for leaf in o.leaves() {
                    kurbo::flatten(&leaf.path, 0.25, |el| {
                        if let Some(p) = el.end_point() {
                            points.push(p);
                        }
                    });
                }
                if points.is_empty() {
                    // Nothing drawn (empty text): its box must be inside.
                    let b = o.world_bounds();
                    points = vec![b.origin(), Point::new(b.x1, b.y0), Point::new(b.x1, b.y1), Point::new(b.x0, b.y1)];
                }
                points.into_iter().all(inside)
            })
            .map(|o| o.id)
            .collect()
    }

    /// Union of the bounds of `ids`, if any of them exist.
    pub fn bounds_of(&self, ids: &[ObjectId]) -> Option<Rect> {
        ids.iter()
            .filter_map(|id| self.object(*id))
            .map(Object::world_bounds)
            .reduce(|a, b| a.union(b))
    }
}

/// Snap `delta` to the nearest axis (for constrained moves).
pub fn constrain_to_axis(delta: Vec2) -> Vec2 {
    if delta.x.abs() >= delta.y.abs() {
        Vec2::new(delta.x, 0.0)
    } else {
        Vec2::new(0.0, delta.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Outline, Rgba, Style};
    use kurbo::Affine;

    fn filled() -> Style {
        Style::filled(Rgba::BLACK)
    }

    #[test]
    fn objects_inside_groups_are_found_and_picked_with_their_groups_placement() {
        let square = |id, x| {
            let mut o = Object::new(ObjectId(id), Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 10.0), corner_radius: 0.0 }, filled());
            o.transform = Affine::translate((x, 0.0));
            o
        };
        let mut inner = Object::new(ObjectId(2), Shape::Group { children: vec![square(3, 0.0), square(4, 20.0)] }, Style::default());
        inner.transform = Affine::translate((0.0, 100.0));
        let mut outer = Object::new(ObjectId(1), Shape::Group { children: vec![inner, square(5, 40.0)] }, Style::default());
        outer.transform = Affine::translate((1000.0, 0.0));
        let mut doc = Document::default();
        doc.active_page_mut().objects.push(outer);
        let page = doc.active_page();

        let (found, parent) = page.find(ObjectId(4)).unwrap();
        assert_eq!(found.id, ObjectId(4));
        assert_eq!(parent * found.transform * Point::ZERO, Point::new(1020.0, 100.0), "both groups' moves");
        assert!(page.find(ObjectId(9)).is_none());

        let outer = page.object(ObjectId(1)).unwrap();
        assert_eq!(outer.member_at(Affine::IDENTITY, Point::new(1025.0, 105.0), 0.5), Some(ObjectId(4)));
        assert_eq!(outer.member_at(Affine::IDENTITY, Point::new(1045.0, 5.0), 0.5), Some(ObjectId(5)));
        assert_eq!(outer.member_at(Affine::IDENTITY, Point::new(25.0, 105.0), 0.5), None, "not where it would be unmoved");
    }

    #[test]
    fn symmetry_groups_are_not_opened() {
        let mut group = Object::new(ObjectId(1), Shape::Group { children: vec![Object::new(ObjectId(2), Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 10.0), corner_radius: 0.0 }, filled())] }, Style::default());
        group.symmetry = Some(crate::symmetry::Symmetry { lines: 2, center: Point::ZERO });
        assert!(group.find(ObjectId(2), Affine::IDENTITY).is_none());
        assert_eq!(group.member_at(Affine::IDENTITY, Point::new(5.0, 5.0), 0.5), Some(ObjectId(1)), "the group itself");
    }

    #[test]
    fn star_has_twice_the_vertices_and_starts_at_top() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let path = Shape::Star { rect, points: 5, inner_ratio: 0.5 }.to_path();
        let vertices = path.elements().iter().filter(|e| !matches!(e, kurbo::PathEl::ClosePath)).count();
        assert_eq!(vertices, 10);
        match path.elements()[0] {
            kurbo::PathEl::MoveTo(p) => assert!((p - Point::new(50.0, 0.0)).hypot() < 1e-9),
            _ => panic!("starts with MoveTo"),
        }
    }

    #[test]
    fn polygon_fits_its_rect() {
        let rect = Rect::new(10.0, 20.0, 110.0, 120.0);
        let b = Shape::Polygon { rect, sides: 4 }.to_path().bounding_box();
        assert!((b.y0 - 20.0).abs() < 1e-9 && (b.y1 - 120.0).abs() < 1e-9);
    }

    #[test]
    fn world_bounds_follow_transform() {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 20.0), corner_radius: 0.0 }, filled());
        let obj = doc.active_page_mut().object_mut(id).unwrap();
        obj.transform = Affine::translate((5.0, 5.0)) * Affine::scale(2.0);
        let b = obj.world_bounds();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (5.0, 5.0, 25.0, 45.0));
    }

    #[test]
    fn hit_test_uses_fill_for_filled_and_outline_for_unfilled() {
        let mut doc = Document::default();
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let filled_id = doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, filled());
        let page = doc.active_page();
        assert_eq!(page.hit_test(Point::new(50.0, 50.0), 1.0), Some(filled_id));

        let mut doc = Document::default();
        let outline = Style::outlined(Outline::new(Rgba::BLACK, 2.0));
        let id = doc.add_object(Shape::Ellipse { rect, arc: None }, outline);
        let page = doc.active_page();
        assert_eq!(page.hit_test(Point::new(50.0, 50.0), 1.0), None);
        assert_eq!(page.hit_test(Point::new(50.0, 1.5), 1.0), Some(id));
    }

    #[test]
    fn hit_test_returns_topmost() {
        let mut doc = Document::default();
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, filled());
        let top = doc.add_object(Shape::Ellipse { rect, arc: None }, filled());
        assert_eq!(doc.active_page().hit_test(Point::new(50.0, 50.0), 0.0), Some(top));
    }

    #[test]
    fn marquee_selects_only_fully_enclosed() {
        let mut doc = Document::default();
        let a = doc.add_object(Shape::Ellipse { rect: Rect::new(10.0, 10.0, 20.0, 20.0), arc: None }, filled());
        doc.add_object(Shape::Ellipse { rect: Rect::new(40.0, 40.0, 80.0, 80.0), arc: None }, filled());
        assert_eq!(doc.active_page().objects_inside(Rect::new(0.0, 0.0, 50.0, 50.0)), vec![a]);
    }

    #[test]
    fn paint_at_finds_fill_gradient_and_outline_colours() {
        use crate::{Fill, Gradient, GradientKind};
        let mut doc = Document::default();
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let red = Rgba::rgb(255, 0, 0);
        let mut style = Style::filled(red);
        style.outline = Some(Outline::new(Rgba::rgb(0, 0, 255), 4.0));
        let id = doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, style);
        let page = doc.active_page();
        assert_eq!(page.paint_at(Point::new(50.0, 50.0), 1.0), Some(PaintHit { id, color: red, outline: false }));
        assert_eq!(page.paint_at(Point::new(1.0, 50.0), 1.0).map(|h| h.outline), Some(true));
        assert_eq!(page.paint_at(Point::new(150.0, 50.0), 1.0), None);

        let g = Gradient::across(GradientKind::Linear, rect, Rgba::BLACK, Rgba::WHITE);
        doc.restyle_objects(&[id], |s| s.fill = Some(Fill::Gradient(g.clone())));
        doc.transform_objects(&[id], Affine::translate((100.0, 0.0)));
        let hit = doc.active_page().paint_at(Point::new(175.0, 50.0), 1.0).unwrap();
        assert_eq!(hit.color, Rgba::rgb(191, 191, 191), "three quarters along the local gradient");
    }

    #[test]
    fn corner_radius_is_clamped() {
        let rect = Rect::new(0.0, 0.0, 10.0, 40.0);
        let b = Shape::Rectangle { rect, corner_radius: 100.0 }.to_path().bounding_box();
        assert!((b.width() - 10.0).abs() < 1e-6 && (b.height() - 40.0).abs() < 1e-6);
    }

    #[test]
    fn a_lasso_takes_only_what_lies_wholly_inside() {
        let mut doc = crate::document::Document::default();
        let circle = doc.add_object(Shape::Ellipse { rect: Rect::new(10.0, 10.0, 50.0, 50.0), arc: None }, Style::new_object());
        let wide = doc.add_object(Shape::Rectangle { rect: Rect::new(60.0, 10.0, 200.0, 50.0), corner_radius: 0.0 }, Style::new_object());
        // A diamond around the circle: it holds the circle but not the circle's bounding box.
        let diamond = [Point::new(30.0, -2.0), Point::new(62.0, 30.0), Point::new(30.0, 62.0), Point::new(-2.0, 30.0)];
        let page = doc.active_page();
        assert_eq!(page.objects_in_lasso(&diamond), vec![circle]);
        assert!(page.objects_inside(Rect::new(-2.0, -2.0, 62.0, 62.0)).contains(&circle));
        let around_both = [Point::new(0.0, 0.0), Point::new(210.0, 0.0), Point::new(210.0, 60.0), Point::new(0.0, 60.0)];
        assert_eq!(page.objects_in_lasso(&around_both), vec![circle, wide]);
        assert!(page.objects_in_lasso(&around_both[..2]).is_empty(), "not a loop");
    }
}

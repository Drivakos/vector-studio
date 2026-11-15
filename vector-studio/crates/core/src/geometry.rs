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
    pub style: &'a Style,
    pub closed: bool,
    /// Local → page transform, groups included.
    pub transform: Affine,
}

impl Leaf<'_> {
    /// Fill colour, if the leaf is closed and has one.
    pub fn fill(&self) -> Option<crate::Rgba> {
        self.style.fill.filter(|_| self.closed)
    }
}

impl Object {
    /// Everything this object draws, bottom first: itself, or its group's members.
    pub fn leaves(&self) -> Vec<Leaf<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(Affine::IDENTITY, &mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, parent: Affine, out: &mut Vec<Leaf<'a>>) {
        let transform = parent * self.transform;
        match &self.shape {
            Shape::Group { children } => children.iter().for_each(|c| c.collect_leaves(transform, out)),
            shape => {
                let mut path = shape.to_path();
                path.apply_affine(transform);
                out.push(Leaf { path, shape, style: &self.style, closed: shape.is_closed(), transform });
            }
        }
    }

    /// The outline in page space.
    pub fn world_path(&self) -> BezPath {
        let mut path = self.shape.to_path();
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
        match &self.shape {
            Shape::Group { children } => children
                .iter()
                .map(|c| c.bounds_in(transform))
                .reduce(|a, b| a.union(b))
                .unwrap_or_default(),
            Shape::Text(text) => transform.transform_rect_bbox(text.layout().bounds()),
            shape => {
                let mut path = shape.to_path();
                path.apply_affine(transform);
                path.bounding_box()
            }
        }
    }

    /// Whether `point` (page space) hits the fill or comes within `tolerance` of the outline
    /// of this object or, for a group, of any object in it.
    pub fn hit(&self, point: Point, tolerance: f64) -> bool {
        self.leaves().iter().any(|leaf| {
            let half = leaf.style.outline.as_ref().map_or(0.0, |o| o.page_width(leaf.transform) / 2.0);
            let reach = tolerance + half;
            // Text is picked anywhere in its line boxes, not just on the glyphs.
            if let Shape::Text(text) = leaf.shape {
                let local = leaf.transform.inverse() * point;
                let pad = tolerance / leaf.transform.determinant().abs().sqrt().max(1e-9);
                if text.layout().bounds().inflate(pad, pad).contains(local) {
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

impl Page {
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    /// Topmost object under `point`.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<ObjectId> {
        self.objects.iter().rev().find(|o| o.hit(point, tolerance)).map(|o| o.id)
    }

    /// Objects whose bounds lie fully inside `rect`, bottom first.
    pub fn objects_inside(&self, rect: Rect) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| {
                let b = o.world_bounds();
                rect.contains(b.origin()) && rect.contains(Point::new(b.x1, b.y1))
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
        Style { fill: Some(Rgba::BLACK), outline: None }
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
        let outline = Style { fill: None, outline: Some(Outline::new(Rgba::BLACK, 2.0)) };
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
    fn corner_radius_is_clamped() {
        let rect = Rect::new(0.0, 0.0, 10.0, 40.0);
        let b = Shape::Rectangle { rect, corner_radius: 100.0 }.to_path().bounding_box();
        assert!((b.width() - 10.0).abs() < 1e-6 && (b.height() - 40.0).abs() < 1e-6);
    }
}

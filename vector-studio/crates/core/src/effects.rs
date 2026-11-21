//! Interactive effects that stay editable, as in CorelDRAW: each is kept as settings on
//! the object. What it draws (contour steps, blend steps, an extrusion, a block shadow, a
//! mesh) is made again from the object whenever the object or the settings change
//! ([`Document::sync_effects`]); envelopes and distortions reshape the object as it is
//! drawn, like a perspective.

use std::fmt;
use std::hash::{Hash, Hasher};

use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use serde::{Deserialize, Serialize};

use crate::blend::Blend;
use crate::block_shadow::BlockShadow;
use crate::contour::{Contour, ContourDirection};
use crate::distort::Distortion;
use crate::document::{Document, Object, ObjectId, Shape, Style};
use crate::envelope::Envelope;
use crate::extrude::Extrude;
use crate::mesh::MeshFill;

/// The effects on one object. A blend is on a group of its two ends.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contour: Option<Contour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<Blend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extrude: Option<Extrude>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_shadow: Option<BlockShadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distortion: Option<Distortion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<MeshFill>,
}

impl Effects {
    pub fn is_empty(&self) -> bool {
        *self == Effects::default()
    }
}

/// Objects an effect draws, as last made, and a fingerprint of what they were made from.
/// They are not saved, and every one is equal, so making them again is not an edit.
#[derive(Clone, Default)]
pub struct Generated {
    made: bool,
    key: u64,
    /// Where the source was when they were made: a move only moves them.
    origin: Point,
    objects: Vec<Object>,
}

impl Generated {
    /// The objects, placed in the space the object with the effect is placed in (its
    /// group's, for a blend), bottom first.
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    /// Whether the objects were made from a source with fingerprint `key`; if so they
    /// are moved to `origin`.
    fn current(&mut self, key: u64, origin: Point) -> bool {
        if !self.made || self.key != key {
            return false;
        }
        let d = origin - self.origin;
        if d != Vec2::ZERO {
            for o in &mut self.objects {
                o.transform = Affine::translate(d) * o.transform;
            }
            self.origin = origin;
        }
        true
    }

    fn set(&mut self, key: u64, origin: Point, objects: Vec<Object>) {
        *self = Generated { made: true, key, origin, objects };
    }
}

impl PartialEq for Generated {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl fmt::Debug for Generated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Generated({} objects)", self.objects.len())
    }
}

/// What an effect is made from: the object as drawn without its effects, in the space it
/// is placed in.
pub(crate) struct Source {
    pub parts: Vec<Part>,
    pub bounds: Rect,
}

pub(crate) struct Part {
    pub path: BezPath,
    pub closed: bool,
    /// The style as it shows here: outline widths fixed, fountain fills placed.
    pub style: Style,
}

impl Source {
    pub fn of(obj: &Object) -> Source {
        Self::from_leaves(obj.leaves())
    }

    fn from_leaves(leaves: Vec<crate::geometry::Leaf<'_>>) -> Source {
        let parts: Vec<Part> = leaves
            .into_iter()
            .map(|leaf| {
                let mut style = leaf.style.clone().into_owned();
                if let Some(o) = style.outline.as_mut() {
                    o.width = o.page_width(leaf.transform);
                    o.scale_with_object = false;
                }
                if let Some(fill) = style.fill.as_mut() {
                    fill.transform(leaf.transform);
                }
                let closed = leaf.fill().is_some() || (leaf.closed && !matches!(leaf.shape, Shape::Image(_)));
                Part { path: leaf.path, closed, style }
            })
            .collect();
        let bounds = parts.iter().map(|p| p.path.bounding_box()).reduce(|a, b| a.union(b)).unwrap_or_default();
        Source { parts, bounds }
    }

    /// A fingerprint of the source where it is, and of `settings`.
    fn key(&self, settings: &(impl fmt::Debug + ?Sized)) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        let o = self.bounds.origin().to_vec2();
        for part in &self.parts {
            hash_path(&mut h, &part.path, o);
            format!("{}{:?}", part.closed, part.style).hash(&mut h);
        }
        format!("{settings:?}").hash(&mut h);
        h.finish()
    }

    /// The area the source covers: its filled parts and its outlines, as non-overlapping
    /// contours.
    pub fn silhouette(&self) -> BezPath {
        let mut all = BezPath::new();
        for part in &self.parts {
            if part.closed && (part.style.fill.is_some() || part.style.outline.is_none()) {
                all.extend(crate::shaping::closed(&part.path).iter());
            }
            if let Some(o) = &part.style.outline {
                let stroked = kurbo::stroke(part.path.iter(), &o.stroke(o.width.max(0.1)), &kurbo::StrokeOpts::default(), 0.05);
                all.extend(stroked.iter());
            }
        }
        crate::curves::nonzero_region(&all).unwrap_or_default()
    }

    /// The area inside the closed parts (outlines not included), as non-overlapping
    /// contours.
    pub fn area(&self) -> BezPath {
        let mut all = BezPath::new();
        for part in self.parts.iter().filter(|p| p.closed) {
            // Even-odd, like the renderer: each part on its own first.
            if let Some(region) = crate::shaping::boolean(&crate::shaping::closed(&part.path), &BezPath::new(), linesweeper::BinaryOp::Union) {
                all.extend(region.iter());
            }
        }
        crate::curves::nonzero_region(&all).unwrap_or_default()
    }

    /// The first part's style, standing for the whole source.
    pub fn style(&self) -> Style {
        self.parts.first().map(|p| p.style.clone()).unwrap_or_default()
    }
}

pub(crate) fn hash_path(h: &mut impl Hasher, path: &BezPath, origin: Vec2) {
    let mut point = |p: Point| {
        (p.x - origin.x).to_bits().hash(h);
        (p.y - origin.y).to_bits().hash(h);
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => point(a),
            PathEl::QuadTo(a, b) => [a, b].into_iter().for_each(&mut point),
            PathEl::CurveTo(a, b, c) => [a, b, c].into_iter().for_each(&mut point),
            PathEl::ClosePath => point(Point::new(f64::NAN, 0.0)),
        }
    }
    point(Point::new(f64::INFINITY, 0.0));
}

/// An effect that draws objects made from its object.
pub(crate) trait Maker: fmt::Debug {
    fn generated(&mut self) -> &mut Generated;
    /// The objects for `source`, in the same space.
    fn make(&self, source: &Source) -> Vec<Object>;
}

fn refresh(effect: &mut dyn Maker, source: &Source) {
    let key = source.key(&*effect);
    let origin = source.bounds.origin();
    if !effect.generated().current(key, origin) {
        let objects = effect.make(source);
        effect.generated().set(key, origin, objects);
    }
}

/// A plain object made by an effect: `path` in the space it is drawn in.
pub(crate) fn piece(path: BezPath, style: Style) -> Object {
    Object::new(ObjectId(0), Shape::Curve { path }, style)
}

impl Object {
    /// Objects effects draw under this one, bottom first: block shadow, extrusion,
    /// outside contour. In the space this object is placed in.
    pub fn effects_under(&self) -> impl Iterator<Item = &Object> {
        let e = &self.effects;
        let contour = e.contour.as_ref().filter(|c| c.direction == ContourDirection::Outside);
        let block = e.block_shadow.iter().flat_map(|b| b.generated.objects());
        let extrude = e.extrude.iter().flat_map(|x| x.generated.objects());
        block.chain(extrude).chain(contour.into_iter().flat_map(|c| c.generated.objects()))
    }

    /// Objects effects draw over this one: an inside contour.
    pub fn effects_over(&self) -> impl Iterator<Item = &Object> {
        self.effects.contour.iter().filter(|c| c.direction != ContourDirection::Outside).flat_map(|c| c.generated.objects())
    }

    /// A blend group's steps, drawn between its first member and the rest, in the group's
    /// own space.
    pub fn blend_steps(&self) -> &[Object] {
        self.effects.blend.as_ref().map_or(&[], |b| b.generated.objects())
    }

    /// The bounds of the object as drawn without its effects, in the space it is placed
    /// in: what an extrusion's vanishing point is measured from.
    pub fn effect_bounds(&self) -> Rect {
        Source::of(self).bounds
    }

    /// Whether a mesh fill can go on this object: an unlocked closed shape (not text, a
    /// group, a bitmap or a PowerClip) that no effect reshapes.
    pub fn can_take_mesh(&self) -> bool {
        !self.locked
            && self.contents.is_empty()
            && !self.is_warped()
            && self.shape.is_closed()
            && !matches!(self.shape, Shape::Text(_) | Shape::Group { .. } | Shape::Image(_))
    }

    /// Whether the object has an effect Object > Clear Effect removes.
    pub fn has_effect(&self) -> bool {
        self.perspective.is_some() || self.shadow.is_some() || !self.effects.is_empty()
    }

    fn sync_effects(&mut self) {
        if let Shape::Group { children } = &mut self.shape {
            children.iter_mut().for_each(Object::sync_effects);
        }
        self.contents.iter_mut().for_each(Object::sync_effects);
        if let Some(mut mesh) = self.effects.mesh.take() {
            mesh.sync();
            self.effects.mesh = Some(mesh);
        }
        if let Some(mut blend) = self.effects.blend.take() {
            blend.sync(self);
            self.effects.blend = Some(blend);
        }
        let e = &self.effects;
        if e.contour.is_none() && e.extrude.is_none() && e.block_shadow.is_none() {
            return;
        }
        let source = Source::of(self);
        let e = &mut self.effects;
        let makers: [Option<&mut dyn Maker>; 3] = [
            e.contour.as_mut().map(|m| m as &mut dyn Maker),
            e.extrude.as_mut().map(|m| m as &mut dyn Maker),
            e.block_shadow.as_mut().map(|m| m as &mut dyn Maker),
        ];
        for maker in makers.into_iter().flatten() {
            refresh(maker, &source);
        }
    }
}

impl Blend {
    fn sync(&mut self, group: &Object) {
        let Shape::Group { children } = &group.shape else {
            self.generated.set(0, Point::ZERO, Vec::new());
            return;
        };
        let ends: Vec<Source> = children.iter().map(Source::of).collect();
        let mut h = std::hash::DefaultHasher::new();
        for end in &ends {
            end.key(&()).hash(&mut h);
            for v in [end.bounds.x0, end.bounds.y0] {
                v.to_bits().hash(&mut h);
            }
        }
        format!("{self:?}").hash(&mut h);
        let key = h.finish();
        if !self.generated.current(key, Point::ZERO) {
            let objects = self.make(&ends);
            self.generated.set(key, Point::ZERO, objects);
        }
    }
}

impl Document {
    /// Make again what each effect draws whose object or settings changed.
    pub fn sync_effects(&mut self) {
        for page in &mut self.pages {
            page.objects.iter_mut().for_each(Object::sync_effects);
        }
    }

    /// The unlocked objects among `ids` (on the active page) that `has` an effect.
    pub fn with_effect(&self, ids: &[ObjectId], has: impl Fn(&Object) -> bool) -> Vec<ObjectId> {
        let page = self.active_page();
        ids.iter().copied().filter(|id| page.object(*id).is_some_and(|o| !o.locked && has(o))).collect()
    }

    /// Change the effects of each unlocked object among `ids`. Returns those changed.
    pub fn edit_effects(&mut self, ids: &[ObjectId], mut edit: impl FnMut(&mut Object)) -> Vec<ObjectId> {
        let page = self.active_page_mut();
        let mut out = Vec::new();
        for id in ids {
            if let Some(obj) = page.object_mut(*id).filter(|o| !o.locked) {
                let before = obj.clone();
                edit(obj);
                if *obj != before {
                    out.push(*id);
                }
            }
        }
        out
    }

    /// Object > Clear Effect: remove every effect from the unlocked objects among `ids`.
    /// Returns those changed.
    pub fn clear_effects(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        self.edit_effects(ids, |o| {
            o.perspective = None;
            o.shadow = None;
            o.effects = Effects::default();
        })
    }
}

/// `rgba` (straight alpha, row by row) as a PNG.
pub(crate) fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    Some(data)
}

/// A path's subpaths as cubic segments (lines and quadratics raised), each with whether
/// it is closed. A closed subpath ends where it starts.
pub(crate) fn cubic_subpaths(path: &BezPath) -> Vec<(Vec<kurbo::CubicBez>, bool)> {
    use kurbo::{CubicBez, QuadBez};
    let line = |a: Point, b: Point| CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b);
    let mut out: Vec<(Vec<CubicBez>, bool)> = Vec::new();
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    let mut segs: Vec<CubicBez> = Vec::new();
    let flush = |segs: &mut Vec<CubicBez>, closed: bool, out: &mut Vec<(Vec<CubicBez>, bool)>| {
        if !segs.is_empty() {
            out.push((std::mem::take(segs), closed));
        }
    };
    // A subpath that comes back to its start without closing is closed too.
    let back = |start: Point, last: Point| (last - start).hypot() < 1e-9;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut segs, back(start, last), &mut out);
                (start, last) = (p, p);
            }
            PathEl::LineTo(p) => {
                segs.push(line(last, p));
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                segs.push(QuadBez::new(last, c, p).raise());
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                segs.push(CubicBez::new(last, a, b, p));
                last = p;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-9 {
                    segs.push(line(last, start));
                }
                flush(&mut segs, true, &mut out);
                last = start;
            }
        }
    }
    flush(&mut segs, back(start, last), &mut out);
    out
}

/// The cubic segments back as a path.
pub(crate) fn cubic_path(segs: &[kurbo::CubicBez], closed: bool, out: &mut BezPath) {
    let Some(first) = segs.first() else { return };
    out.move_to(first.p0);
    for s in segs {
        out.curve_to(s.p1, s.p2, s.p3);
    }
    if closed {
        out.close_path();
    }
}

#[cfg(test)]
mod tests {
    use kurbo::{Point, Rect, Vec2};

    use super::*;
    use crate::{Blend, BlockShadow, Distortion, DistortionKind, Envelope, EnvelopeMode, Extrude, ExtrudeShading, Fill, MeshFill, Rgba};

    const RED: Rgba = Rgba::rgb(255, 0, 0);
    const BLUE: Rgba = Rgba::rgb(0, 0, 255);

    fn square(doc: &mut Document, r: Rect, c: Rgba) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, Style::filled(c))
    }

    fn obj(doc: &Document, id: ObjectId) -> &Object {
        doc.active_page().object(id).unwrap()
    }

    fn bounds(objects: &[Object]) -> Vec<Rect> {
        objects.iter().map(Object::world_bounds).collect()
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn near_rect(a: Rect, b: Rect, tol: f64) -> bool {
        near(a.x0, b.x0, tol) && near(a.y0, b.y0, tol) && near(a.x1, b.x1, tol) && near(a.y1, b.y1, tol)
    }

    #[test]
    fn contours_step_out_or_in_and_fade_to_their_colours() {
        let mut doc = Document::default();
        let id = square(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0), RED);
        let mut c = Contour::new(ContourDirection::Outside);
        (c.steps, c.offset, c.fill) = (3, 5.0, BLUE);
        doc.edit_effects(&[id], |o| o.effects.contour = Some(c.clone()));
        doc.sync_effects();
        let made = obj(&doc, id).effects.contour.as_ref().unwrap().generated.objects().to_vec();
        let b = bounds(&made);
        assert_eq!(b.len(), 3);
        assert!(near_rect(b[0], Rect::new(-15.0, -15.0, 115.0, 115.0), 0.1), "largest first, at the bottom: {:?}", b[0]);
        assert!(near_rect(b[2], Rect::new(-5.0, -5.0, 105.0, 105.0), 0.1), "{:?}", b[2]);
        assert_eq!(made[0].style.fill, Some(Fill::Solid(BLUE)), "the last copy takes the contour's colour");
        let o = obj(&doc, id);
        assert!(o.hit(Point::new(-10.0, 50.0), 0.0), "the contour is hit with its object");
        assert_eq!(o.leaves().len(), 1, "but is not part of its shape");
        assert_eq!(o.drawn_leaves().len(), 4);
        assert_eq!(o.effects_under().count(), 3);

        // Moving the object moves the copies without making them again.
        doc.move_objects(&[id], Vec2::new(50.0, 0.0));
        doc.sync_effects();
        let b = bounds(obj(&doc, id).effects.contour.as_ref().unwrap().generated.objects());
        assert!(near_rect(b[0], Rect::new(35.0, -15.0, 165.0, 115.0), 0.1), "{:?}", b[0]);

        // Inside, and To Center until nothing is left.
        doc.edit_effects(&[id], |o| o.effects.contour.as_mut().unwrap().direction = ContourDirection::Inside);
        doc.sync_effects();
        let o = obj(&doc, id);
        let b = bounds(o.effects.contour.as_ref().unwrap().generated.objects());
        assert!(near_rect(b[0], Rect::new(55.0, 5.0, 145.0, 95.0), 0.1) && near_rect(b[2], Rect::new(65.0, 15.0, 135.0, 85.0), 0.1), "{b:?}");
        assert_eq!(o.effects_over().count(), 3, "inside copies are drawn over the object");
        doc.edit_effects(&[id], |o| {
            let c = o.effects.contour.as_mut().unwrap();
            (c.direction, c.offset) = (ContourDirection::ToCenter, 12.0);
        });
        doc.sync_effects();
        assert_eq!(obj(&doc, id).effects.contour.as_ref().unwrap().generated.objects().len(), 4, "12, 24, 36 and 48 in from the sides");
    }

    #[test]
    fn a_blend_steps_shape_and_colour_and_follows_its_ends() {
        let mut doc = Document::default();
        let a = square(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), RED);
        let b = doc.add_object(Shape::Ellipse { rect: Rect::new(200.0, 0.0, 240.0, 40.0), arc: None }, Style::filled(BLUE));
        assert!(!doc.can_blend(a, a));
        let g = doc.blend_objects(a, b, 3).unwrap();
        doc.sync_effects();
        let group = obj(&doc, g);
        assert!(group.is_blend());
        let steps = group.blend_steps();
        assert_eq!(steps.len(), 3);
        let mid = steps[1].world_bounds();
        assert!(near(mid.center().x, 115.0, 0.5) && near(mid.width(), 30.0, 0.5), "halfway in place and size: {mid:?}");
        assert_eq!(steps[1].style.fill, Some(Fill::Solid(Rgba::rgb(128, 0, 128))));
        assert!(steps[1].shape.is_closed());
        // Drawn between the ends: start, three steps, end.
        assert_eq!(group.drawn_leaves().len(), 5);
        assert!(group.hit(Point::new(115.0, 15.0), 0.0), "a step is hit with the blend");

        // Moving an end makes the steps again.
        if let Some(Shape::Group { children }) = doc.active_page_mut().object_mut(g).map(|o| &mut o.shape) {
            children[1].transform = kurbo::Affine::translate((0.0, 100.0)) * children[1].transform;
        }
        doc.sync_effects();
        assert!(obj(&doc, g).blend_steps()[1].world_bounds().center().y > 40.0);

        // Saved with the drawing.
        let json = serde_json::to_string(&doc).unwrap();
        let mut back: Document = serde_json::from_str(&json).unwrap();
        back.sync_effects();
        assert_eq!(obj(&back, g).effects.blend.as_ref().unwrap().steps, 3);
        assert_eq!(obj(&back, g).blend_steps().len(), 3, "made again on open");

        // Ungroup keeps the steps as a group between the ends.
        let out = doc.ungroup_objects(&[g]);
        assert_eq!(out.len(), 3);
        let page = doc.active_page();
        assert!(matches!(&page.objects[1].shape, Shape::Group { children } if children.len() == 3));
        assert!(page.objects.iter().all(|o| o.effects.blend.is_none()));
    }

    #[test]
    fn an_extrusion_runs_back_to_its_vanishing_point_and_a_block_shadow_sweeps() {
        let mut doc = Document::default();
        let id = square(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0), RED);
        let mut x = Extrude::new(Vec2::new(250.0, 0.0));
        x.depth = 50.0;
        doc.edit_effects(&[id], |o| o.effects.extrude = Some(x.clone()));
        doc.sync_effects();
        let o = obj(&doc, id);
        let made = o.effects.extrude.as_ref().unwrap().generated.objects();
        assert_eq!(made.len(), 5, "the back and four sides");
        let all = bounds(made).into_iter().reduce(|a, b| a.union(b)).unwrap();
        // Halfway to (300, 50): the back face spans 150..200 across.
        assert!(near(all.x1, 200.0, 1e-6) && near(all.x0, 0.0, 1e-6), "{all:?}");
        let fills: Vec<_> = made.iter().map(|m| m.style.fill.clone()).collect();
        assert_ne!(fills[1], fills[4], "sides facing the light are lighter");
        doc.edit_effects(&[id], |o| o.effects.extrude.as_mut().unwrap().shading = ExtrudeShading::ObjectFill);
        doc.sync_effects();
        assert!(obj(&doc, id).effects.extrude.as_ref().unwrap().generated.objects().iter().all(|m| m.style.fill == Some(Fill::Solid(RED))));

        let id = square(&mut doc, Rect::new(0.0, 300.0, 100.0, 400.0), RED);
        doc.edit_effects(&[id], |o| o.effects.block_shadow = Some(BlockShadow::new(Vec2::new(20.0, 30.0))));
        doc.sync_effects();
        let made = obj(&doc, id).effects.block_shadow.as_ref().unwrap().generated.objects();
        assert_eq!(made.len(), 1);
        let Shape::Curve { path } = &made[0].shape else { panic!("a curve") };
        assert!(near(path.area().abs(), 15000.0, 1.0), "square plus its sweep: {}", path.area());
        assert_eq!(made[0].style.fill, Some(Fill::Solid(Rgba::BLACK)));
    }

    #[test]
    fn an_envelope_and_a_distortion_reshape_until_cleared_or_baked() {
        let mut doc = Document::default();
        let frame = Rect::new(0.0, 0.0, 100.0, 100.0);
        let id = square(&mut doc, frame, RED);
        assert!(doc.can_warp(id));
        let mut e = Envelope::around(frame, EnvelopeMode::Straight);
        assert!((e.map(frame, Point::new(30.0, 70.0)) - Point::new(30.0, 70.0)).hypot() < 1e-9, "starts as no change");
        e.move_node(1, Point::new(50.0, -20.0));
        doc.edit_effects(&[id], |o| o.effects.envelope = Some(e));
        let b = obj(&doc, id).world_bounds();
        assert!(near(b.y0, -20.0, 1e-6) && near(b.y1, 100.0, 1e-6), "the top peaks at the dragged node: {b:?}");
        assert!(obj(&doc, id).hit(Point::new(50.0, -10.0), 0.0) && !obj(&doc, id).hit(Point::new(5.0, -10.0), 0.0));
        e.mode = EnvelopeMode::Arc;
        doc.edit_effects(&[id], |o| o.effects.envelope = Some(e));
        assert!(near(obj(&doc, id).world_bounds().y0, -20.0, 0.5), "the arc passes through the node");
        // Moving a corner takes the middles beside it half as far.
        let mut f = Envelope::around(frame, EnvelopeMode::Straight);
        f.move_node(0, Point::new(-20.0, 0.0));
        assert_eq!(f.nodes[1], Point::new(40.0, 0.0));
        assert_eq!(f.nodes[7], Point::new(-10.0, 50.0));

        // Baked by Convert to Curves; cleared by Clear Effect.
        let mut baked = doc.clone();
        baked.convert_to_curves(&[id]);
        let o = obj(&baked, id);
        assert!(o.effects.envelope.is_none() && matches!(o.shape, Shape::Curve { .. }));
        assert!(near(o.world_bounds().y0, -20.0, 0.5));
        assert_eq!(doc.clear_effects(&[id]), vec![id]);
        assert!(near_rect(obj(&doc, id).world_bounds(), frame, 1e-9));

        // Push and Pull: nodes out half as far again.
        let mut d = Distortion::new(DistortionKind::PushPull, frame.center());
        d.amplitude = 50.0;
        doc.edit_effects(&[id], |o| o.effects.distortion = Some(d));
        let b = obj(&doc, id).world_bounds();
        assert!(near(b.x0, -25.0, 1e-6) && near(b.x1, 125.0, 1e-6), "{b:?}");
        // Zipper: zigzags out to the amplitude either side.
        (d.kind, d.amplitude, d.frequency) = (DistortionKind::Zipper, 20.0, 4);
        doc.edit_effects(&[id], |o| o.effects.distortion = Some(d));
        let b = obj(&doc, id).world_bounds();
        assert!(near(b.x0, -10.0, 1e-6) && near(b.x1, 110.0, 1e-6), "20% of half the side: {b:?}");
        // Twister: the corners turn a quarter turn, a wide rectangle stands up.
        let wide = square(&mut doc, Rect::new(0.0, 300.0, 200.0, 340.0), RED);
        let mut t = Distortion::new(DistortionKind::Twister, Point::new(100.0, 320.0));
        t.angle = 90.0;
        doc.edit_effects(&[wide], |o| o.effects.distortion = Some(t));
        let b = obj(&doc, wide).world_bounds();
        assert!(b.height() > 150.0, "{b:?}");
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(obj(&back, wide).effects.distortion, Some(t), "saved");
    }

    #[test]
    fn a_mesh_fill_is_drawn_inside_the_object_from_its_node_colours() {
        let mut doc = Document::default();
        let frame = Rect::new(0.0, 0.0, 100.0, 100.0);
        let id = square(&mut doc, frame, RED);
        let text = doc.add_object(Shape::Text(crate::Text::new("Hi")), Style::filled(RED));
        assert!(obj(&doc, id).can_take_mesh() && !obj(&doc, text).can_take_mesh());
        let mut mesh = MeshFill::new(frame, 2, 2, BLUE);
        assert_eq!(mesh.points.len(), 9);
        let corner = mesh.index(0, 0);
        mesh.colors[corner] = RED;
        doc.edit_effects(&[id], |o| o.effects.mesh = Some(mesh.clone()));
        doc.sync_effects();
        let o = obj(&doc, id);
        let Some(Shape::Image(b)) = o.effects.mesh.as_ref().unwrap().shape() else { panic!("a bitmap") };
        let mut reader = png::Decoder::new(std::io::Cursor::new(b.data.as_slice())).read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut data).unwrap();
        let px = |x: u32, y: u32| {
            let i = ((y * b.width + x) * 4) as usize;
            [data[i], data[i + 1], data[i + 2]]
        };
        let (corner, far) = (px(0, 0), px(b.width - 1, b.height - 1));
        assert!(corner[0] > 245 && corner[2] < 10, "red at its node: {corner:?}");
        assert!(far[2] > 245 && far[0] < 10, "blue far from it: {far:?}");
        let leaves = o.drawn_leaves();
        assert_eq!(leaves.len(), 2, "the mesh bitmap, then the object without its fill");
        assert!(leaves[0].effect && leaves[0].clips.len() == 1 && leaves[1].style.fill.is_none());
        assert!(o.hit(Point::new(50.0, 50.0), 0.0));
        let regrid = mesh.regrid(frame, 4, 4);
        assert_eq!(regrid.colors[0], RED);
        assert_eq!(regrid.colors.len(), 25);
    }

    #[test]
    fn clear_effect_removes_every_effect() {
        let mut doc = Document::default();
        let id = square(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0), RED);
        assert!(!obj(&doc, id).has_effect());
        doc.edit_effects(&[id], |o| {
            o.effects.contour = Some(Contour::new(ContourDirection::Outside));
            o.effects.extrude = Some(Extrude::new(Vec2::ZERO));
        });
        assert!(obj(&doc, id).has_effect());
        assert_eq!(doc.with_effect(&[id], Object::has_effect), vec![id]);
        doc.clear_effects(&[id]);
        assert!(!obj(&doc, id).has_effect());
        let _ = Blend::new(1);
    }
}

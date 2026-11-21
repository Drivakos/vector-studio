//! Effects that reshape an object as it is drawn, in its own space: a distortion, then an
//! envelope, then a perspective. The object keeps its own shape, so each stays editable
//! until it is cleared or baked in (Convert to Curves).

use kurbo::{Affine, BezPath, CubicBez, ParamCurve, Point, Rect};

use crate::distort::Distortion;
use crate::document::{Object, Shape};
use crate::effects::{cubic_path, cubic_subpaths};
use crate::envelope::Envelope;
use crate::perspective::Perspective;

/// Pieces each segment is cut into before a curved map, so the result stays close.
const PIECES: usize = 8;

/// The reshaping effects of one object.
#[derive(Clone, Copy, Debug)]
pub struct Warp {
    /// The object's bounds in its own space, before any of them.
    pub frame: Rect,
    pub distortion: Option<Distortion>,
    pub envelope: Option<Envelope>,
    pub perspective: Option<Perspective>,
}

impl Warp {
    /// `path` (in the object's own space) as drawn.
    pub fn map_path(&self, path: &BezPath) -> BezPath {
        let mut p = match &self.distortion {
            Some(d) => d.map_path(self.frame, path),
            None => path.clone(),
        };
        if let Some(e) = &self.envelope {
            p = map_curved(&p, |q| e.map(self.frame, q));
        }
        match &self.perspective {
            Some(h) => h.map_path(&p),
            None => p,
        }
    }

    /// The map without the perspective (which bakes on its own).
    fn shape_only(self) -> Option<Warp> {
        (self.distortion.is_some() || self.envelope.is_some()).then_some(Warp { perspective: None, ..self })
    }
}

/// `path` through `f`, every segment cut into pieces first and lines bent too.
pub(crate) fn map_curved(path: &BezPath, f: impl Fn(Point) -> Point) -> BezPath {
    let mut out = BezPath::new();
    for (segs, closed) in cubic_subpaths(path) {
        let mapped: Vec<CubicBez> = segs
            .iter()
            .flat_map(|s| (0..PIECES).map(move |i| s.subsegment(i as f64 / PIECES as f64..(i + 1) as f64 / PIECES as f64)))
            .map(|s| CubicBez::new(f(s.p0), f(s.p1), f(s.p2), f(s.p3)))
            .collect();
        cubic_path(&mapped, closed, &mut out);
    }
    out
}

impl Object {
    /// Whether an effect reshapes this object as it is drawn.
    pub fn is_warped(&self) -> bool {
        self.perspective.is_some() || self.effects.envelope.is_some() || self.effects.distortion.is_some()
    }

    /// The effects that reshape this object, if any.
    pub fn warp(&self) -> Option<Warp> {
        if !self.is_warped() {
            return None;
        }
        let shaped = self.effects.envelope.is_some() || self.effects.distortion.is_some();
        let frame = if shaped { self.plain_bounds(Affine::IDENTITY) } else { Rect::ZERO };
        Some(Warp { frame, distortion: self.effects.distortion, envelope: self.effects.envelope, perspective: self.perspective })
    }

    /// The bounds an envelope or distortion is set on: the object's own, in its space.
    pub fn warp_frame(&self) -> Rect {
        self.plain_bounds(Affine::IDENTITY)
    }

    /// Make every reshaping effect part of the geometry: shapes and text become curves,
    /// and a group passes them on to its members. Bitmaps keep their shape.
    pub fn bake_warps(&mut self) {
        if let Some(w) = self.warp().and_then(Warp::shape_only) {
            self.effects.envelope = None;
            self.effects.distortion = None;
            self.reshape(&|p| w.map_path(p), Affine::IDENTITY);
        }
        self.bake_perspective();
    }

    /// Only the envelope and distortion of [`Self::bake_warps`]; a perspective stays.
    pub(crate) fn bake_shape_warps(&mut self) {
        if let Some(w) = self.warp().and_then(Warp::shape_only) {
            self.effects.envelope = None;
            self.effects.distortion = None;
            self.reshape(&|p| w.map_path(p), Affine::IDENTITY);
        }
    }

    /// Reshape everything in this object by `map`, which works in the space `to_map`
    /// takes this object's own space to.
    fn reshape(&mut self, map: &dyn Fn(&BezPath) -> BezPath, to_map: Affine) {
        for c in &mut self.contents {
            c.bake_warps();
            c.reshape(map, to_map * c.transform);
        }
        match &mut self.shape {
            Shape::Group { children } => {
                for c in children {
                    c.bake_warps();
                    c.reshape(map, to_map * c.transform);
                }
            }
            Shape::Image(_) => {}
            shape => {
                let mut path = shape.to_path();
                path.apply_affine(to_map);
                let mut path = map(&path);
                path.apply_affine(to_map.inverse());
                *shape = Shape::Curve { path };
            }
        }
    }
}

impl crate::document::Document {
    /// Whether the active page's object `id` can take an envelope or a distortion: an
    /// unlocked top-level object with an area and no bitmap in it.
    pub fn can_warp(&self, id: crate::document::ObjectId) -> bool {
        self.active_page().object(id).is_some_and(|o| {
            let f = o.warp_frame();
            !o.locked && !o.has_bitmap() && f.width() > 0.0 && f.height() > 0.0
        })
    }
}

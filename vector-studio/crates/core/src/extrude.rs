//! Extrude: an object given depth, its sides running back toward a vanishing point. The
//! sides are drawn under the object, the far ones first, filled with the object's colour,
//! a colour, or shaded as if lit from the top left.

use kurbo::{BezPath, ParamCurve, Shape as _, Vec2};
use serde::{Deserialize, Serialize};

use crate::document::{Object, Rgba, Style};
use crate::effects::{Generated, Maker, Source, cubic_subpaths, piece};
use crate::fill::Fill;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtrudeShading {
    /// The object's own fill colour.
    #[default]
    ObjectFill,
    /// One colour.
    Solid,
    /// From the light colour on sides facing the top left to the dark one.
    Shade,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Extrude {
    /// The vanishing point, from the middle of the object's bounds: it moves with the
    /// object.
    pub vanishing_point: Vec2,
    /// How far back the extrusion goes, as a percentage of the way to the vanishing
    /// point (1 to 99).
    pub depth: f64,
    pub shading: ExtrudeShading,
    /// The colour for Solid; the light colour for Shade.
    pub color: Rgba,
    /// The dark colour for Shade.
    pub shade: Rgba,
    #[serde(skip)]
    pub generated: Generated,
}

impl Extrude {
    /// CorelDRAW's starting depth.
    pub const DEPTH: f64 = 20.0;
    pub const MAX_DEPTH: f64 = 99.0;

    pub fn new(vanishing_point: Vec2) -> Self {
        Self {
            vanishing_point,
            depth: Self::DEPTH,
            shading: ExtrudeShading::Shade,
            color: Rgba::rgb(0xC0, 0xC0, 0xC0),
            shade: Rgba::rgb(0x40, 0x40, 0x40),
            generated: Generated::default(),
        }
    }
}

impl Maker for Extrude {
    fn generated(&mut self) -> &mut Generated {
        &mut self.generated
    }

    fn make(&self, source: &Source) -> Vec<Object> {
        let k = self.depth.clamp(0.0, Self::MAX_DEPTH) / 100.0;
        if k <= 0.0 || source.parts.is_empty() {
            return Vec::new();
        }
        let vp = source.bounds.center() + self.vanishing_point;
        let back = |p: kurbo::Point| p.lerp(vp, k);
        let style = source.style();
        let own = style.fill.as_ref().map_or(self.color, Fill::color);
        let base = match self.shading {
            ExtrudeShading::ObjectFill => own,
            ExtrudeShading::Solid => self.color,
            ExtrudeShading::Shade => self.shade,
        };
        let light = Vec2::new(-1.0, -1.0).normalize();
        let paint = |c: Rgba| Style { fill: Some(Fill::Solid(c)), outline: style.outline.clone(), transparency: style.transparency, ..Style::default() };

        let mut back_face = BezPath::new();
        let mut faces: Vec<(f64, BezPath, Rgba)> = Vec::new();
        for part in &source.parts {
            for (segs, closed) in cubic_subpaths(&part.path) {
                let mut sub = BezPath::new();
                crate::effects::cubic_path(&segs, closed, &mut sub);
                let turn = sub.area().signum();
                if closed && part.closed {
                    let mut far = sub.clone();
                    far.apply_affine(kurbo::Affine::translate(vp.to_vec2()) * kurbo::Affine::scale(1.0 - k) * kurbo::Affine::translate(-vp.to_vec2()));
                    back_face.extend(far.iter());
                }
                for s in segs {
                    let mut face = BezPath::new();
                    face.move_to(s.p0);
                    face.curve_to(s.p1, s.p2, s.p3);
                    face.line_to(back(s.p3));
                    face.curve_to(back(s.p2), back(s.p1), back(s.p0));
                    face.close_path();
                    let d = s.p3 - s.p0;
                    let normal = if d.hypot() > 1e-9 { Vec2::new(d.y, -d.x).normalize() * if turn < 0.0 { -1.0 } else { 1.0 } } else { Vec2::ZERO };
                    let color = match self.shading {
                        ExtrudeShading::Shade => self.shade.mix(self.color, (1.0 + normal.dot(light)) / 2.0),
                        _ => base,
                    };
                    faces.push(((s.eval(0.5) - vp).hypot(), face, color));
                }
            }
        }
        // Far sides first, so nearer ones cover them.
        faces.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out = Vec::with_capacity(faces.len() + 1);
        if !back_face.is_empty() {
            out.push(piece(back_face, paint(base)));
        }
        out.extend(faces.into_iter().map(|(_, path, c)| piece(path, paint(c))));
        out
    }
}

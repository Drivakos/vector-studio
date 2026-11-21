//! Blend: a series of shapes that change step by step from one object into another, in
//! shape and colour. The two ends make a group that keeps the blend; editing either end
//! makes the steps again.

use kurbo::{BezPath, CubicBez, ParamCurve, Point, Shape as _, Vec2};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, ObjectId, Outline, Shape, Style};
use crate::effects::{Generated, Source, cubic_path, cubic_subpaths, piece};
use crate::fill::Fill;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Blend {
    /// Shapes between the two ends.
    pub steps: u32,
    #[serde(skip)]
    pub generated: Generated,
}

impl Blend {
    /// CorelDRAW's starting number of steps.
    pub const STEPS: u32 = 20;
    pub const MAX_STEPS: u32 = 999;

    pub fn new(steps: u32) -> Self {
        Self { steps: steps.clamp(1, Self::MAX_STEPS), generated: Generated::default() }
    }

    /// The steps between the first and the last of `ends`, in their space.
    pub(crate) fn make(&self, ends: &[Source]) -> Vec<Object> {
        let (Some(a), Some(b)) = (ends.first(), ends.last()) else { return Vec::new() };
        if ends.len() < 2 {
            return Vec::new();
        }
        let path = |s: &Source| {
            let mut p = BezPath::new();
            s.parts.iter().for_each(|part| p.extend(part.path.iter()));
            p
        };
        let pairs = pair_up(&path(a), &path(b));
        let (sa, sb) = (a.style(), b.style());
        let n = self.steps.clamp(1, Self::MAX_STEPS);
        (1..=n)
            .map(|i| {
                let t = f64::from(i) / f64::from(n + 1);
                let mut out = BezPath::new();
                for (x, y, closed) in &pairs {
                    let segs: Vec<CubicBez> = x.iter().zip(y).map(|(p, q)| lerp_cubic(p, q, t)).collect();
                    cubic_path(&segs, *closed, &mut out);
                }
                piece(out, mix_style(&sa, &sb, t))
            })
            .collect()
    }
}

fn lerp_cubic(a: &CubicBez, b: &CubicBez, t: f64) -> CubicBez {
    CubicBez::new(a.p0.lerp(b.p0, t), a.p1.lerp(b.p1, t), a.p2.lerp(b.p2, t), a.p3.lerp(b.p3, t))
}

type Sub = (Vec<CubicBez>, bool);

/// The subpaths of `a` and `b` paired up, each pair with as many segments on both sides,
/// starting at matching nodes and going the same way round. A subpath with no partner
/// grows from a point at the middle of the other end.
fn pair_up(a: &BezPath, b: &BezPath) -> Vec<(Vec<CubicBez>, Vec<CubicBez>, bool)> {
    let (sa, sb) = (cubic_subpaths(a), cubic_subpaths(b));
    let (ca, cb) = (a.bounding_box().center(), b.bounding_box().center());
    let point = |c: Point, like: &Sub| -> Sub { (vec![CubicBez::new(c, c, c, c)], like.1) };
    (0..sa.len().max(sb.len()))
        .map(|i| {
            let x = sa.get(i).cloned().unwrap_or_else(|| point(ca, &sb[i]));
            let y = sb.get(i).cloned().unwrap_or_else(|| point(cb, &sa[i]));
            let closed = x.1 && y.1;
            let (mut x, mut y) = (x.0, y.0);
            if closed && x.len() > 1 && y.len() > 1 {
                if signed_area(&x).signum() != signed_area(&y).signum() {
                    y = reverse(&y);
                }
                // Start `y` at the node placed most like `x`'s start, around each centre.
                let want = x[0].p0 - centre(&x);
                let cy = centre(&y);
                let k = (0..y.len()).min_by(|i, j| ((y[*i].p0 - cy) - want).hypot2().total_cmp(&((y[*j].p0 - cy) - want).hypot2())).unwrap_or(0);
                y.rotate_left(k);
            }
            let n = x.len().max(y.len());
            split_to(&mut x, n);
            split_to(&mut y, n);
            (x, y, closed)
        })
        .collect()
}

fn signed_area(segs: &[CubicBez]) -> f64 {
    let mut p = BezPath::new();
    cubic_path(segs, true, &mut p);
    p.area()
}

fn centre(segs: &[CubicBez]) -> Vec2 {
    let sum = segs.iter().fold(Vec2::ZERO, |s, c| s + c.p0.to_vec2());
    sum / segs.len().max(1) as f64
}

fn reverse(segs: &[CubicBez]) -> Vec<CubicBez> {
    segs.iter().rev().map(|c| CubicBez::new(c.p3, c.p2, c.p1, c.p0)).collect()
}

/// Cut the longest segments in half until there are `n`.
fn split_to(segs: &mut Vec<CubicBez>, n: usize) {
    let len = |c: &CubicBez| (c.p1 - c.p0).hypot() + (c.p2 - c.p1).hypot() + (c.p3 - c.p2).hypot();
    while segs.len() < n {
        let Some(i) = (0..segs.len()).max_by(|a, b| len(&segs[*a]).total_cmp(&len(&segs[*b]))) else { return };
        let (l, r) = segs[i].subdivide();
        segs[i] = l;
        segs.insert(i + 1, r);
    }
}

/// The style `t` of the way from `a` to `b`: colours and widths mixed; a fill or outline
/// only one end has shows for the half nearer it.
fn mix_style(a: &Style, b: &Style, t: f64) -> Style {
    let near = |x: Option<Fill>, y: Option<Fill>| if t < 0.5 { x } else { y };
    let fill = match (&a.fill, &b.fill) {
        (Some(x), Some(y)) => Some(Fill::Solid(x.color().mix(y.color(), t))),
        (x, y) => near(x.clone(), y.clone()),
    };
    let outline = match (&a.outline, &b.outline) {
        (Some(x), Some(y)) => Some(Outline { color: x.color.mix(y.color, t), width: x.width + (y.width - x.width) * t, ..x.clone() }),
        (x, y) => if t < 0.5 { x.clone() } else { y.clone() },
    };
    Style { fill, outline, transparency: a.transparency + (b.transparency - a.transparency) * t, ..Style::default() }
}

impl Object {
    /// Whether this is a blend group (its two ends, with steps drawn between them).
    pub fn is_blend(&self) -> bool {
        self.effects.blend.is_some() && matches!(&self.shape, Shape::Group { children } if children.len() == 2)
    }
}

impl Document {
    /// Whether `a` and `b` can be blended: two different unlocked top-level objects on the
    /// active page, neither a bitmap.
    pub fn can_blend(&self, a: ObjectId, b: ObjectId) -> bool {
        let page = self.active_page();
        let ok = |id| page.object(id).is_some_and(|o: &Object| !o.locked && !matches!(o.shape, Shape::Image(_)));
        a != b && ok(a) && ok(b)
    }

    /// Blend `a` into `b` with `steps` steps: the two become a blend group where the upper
    /// of them was, the lower one first. Returns the group.
    pub fn blend_objects(&mut self, a: ObjectId, b: ObjectId, steps: u32) -> Option<ObjectId> {
        if !self.can_blend(a, b) {
            return None;
        }
        let group = self.group_objects(&[a, b])?;
        let obj = self.active_page_mut().object_mut(group)?;
        obj.effects.blend = Some(Blend::new(steps));
        Some(group)
    }

    /// Break Blend Apart, before Ungroup: each unlocked blend group among `ids` keeps its
    /// steps, as a group of curves between its two ends, and is a plain group.
    pub(crate) fn break_blends(&mut self, ids: &[ObjectId]) {
        let targets: Vec<ObjectId> = self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && !o.locked && o.is_blend()).map(|o| o.id).collect();
        for id in targets {
            let Some(blend) = self.active_page_mut().object_mut(id).and_then(|o| o.effects.blend.take()) else { continue };
            let mut steps = blend.generated.objects().to_vec();
            for s in &mut steps {
                s.id = self.alloc_object_id();
            }
            let group = Object::new(self.alloc_object_id(), Shape::Group { children: steps }, Style::default());
            if let Some(Shape::Group { children }) = self.active_page_mut().object_mut(id).map(|o| &mut o.shape) {
                children.insert(1, group);
            }
        }
    }
}

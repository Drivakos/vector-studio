//! Artistic Media: a stroke of varying width drawn along a curve, kept as a setting on
//! the curve. The curve stays the control path the Shape tool edits; what is drawn is the
//! stroke's outline, filled with the object's fill and outlined with its outline.
//!
//! The outline is the union of small convex pieces laid along the curve (two triangles
//! per piece of it, and a disc at each corner and round end), so tight turns and loops
//! come out clean.

use std::hash::{Hash, Hasher};

use kurbo::{BezPath, PathEl, Point, Shape as _, Vec2};
use linesweeper::{BinaryOp, FillRule, binary_op};
use serde::{Deserialize, Serialize};

use crate::document::{Object, Shape};
use crate::effects::hash_path;

/// The shapes a preset stroke can take along its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaPreset {
    /// The same width all along, with round ends.
    Even,
    /// Thin at both ends, full in the middle.
    TaperBoth,
    /// Thin at the start, full with a round end.
    TaperStart,
    /// Full with a round start, thin at the end.
    TaperEnd,
    /// Narrow round ends, full in the middle.
    Swell,
}

impl MediaPreset {
    pub const ALL: [MediaPreset; 5] = [MediaPreset::Even, MediaPreset::TaperBoth, MediaPreset::TaperStart, MediaPreset::TaperEnd, MediaPreset::Swell];

    pub fn label(self) -> &'static str {
        match self {
            MediaPreset::Even => "Even",
            MediaPreset::TaperBoth => "Tapered both ends",
            MediaPreset::TaperStart => "Tapered start",
            MediaPreset::TaperEnd => "Tapered end",
            MediaPreset::Swell => "Swelling",
        }
    }

    /// The width at `t` (0 at the start of the curve, 1 at its end), as a share of the
    /// full width.
    pub fn profile(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            MediaPreset::Even => 1.0,
            MediaPreset::TaperBoth => (std::f64::consts::PI * t).sin().max(0.0).powf(0.6),
            MediaPreset::TaperStart => t.powf(0.6),
            MediaPreset::TaperEnd => (1.0 - t).powf(0.6),
            MediaPreset::Swell => 0.3 + 0.7 * (std::f64::consts::PI * t).sin().max(0.0),
        }
    }
}

/// How the width along the curve is set.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaKind {
    /// A preset shape, its width a share of the stroke width.
    Preset { preset: MediaPreset },
    /// A flat nib held at `angle` degrees (counterclockwise from the right): wide where
    /// the curve runs across the nib, thin where it runs along it.
    Calligraphic { angle: f64 },
}

/// An Artistic Media stroke on an object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtisticMedia {
    pub kind: MediaKind,
    /// The full width of the stroke, in the object's own space (points).
    pub width: f64,
    #[serde(skip)]
    pub(crate) drawn: MediaOutline,
}

/// The outline as last made and a fingerprint of what it was made from. Every one is
/// equal, so making it again is not an edit.
#[derive(Clone, Default)]
pub(crate) struct MediaOutline {
    key: u64,
    path: Option<BezPath>,
}

impl PartialEq for MediaOutline {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for MediaOutline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MediaOutline({})", self.path.is_some())
    }
}

/// The thinnest a calligraphic stroke gets, as a share of its width, so a curve that runs
/// along the nib still shows.
const NIB_EDGE: f64 = 0.06;
/// Turns sharper than this (radians) between two pieces of the curve get a round join.
const CORNER: f64 = 0.26;
/// How far the outline may stray from the exact stroke (points).
const ACCURACY: f64 = 0.02;
/// The most pieces one stroke is cut into.
const MAX_PIECES: f64 = 6000.0;

impl ArtisticMedia {
    pub const WIDTH: f64 = 10.0;
    pub const MAX_WIDTH: f64 = 500.0;
    pub const ANGLE: f64 = 45.0;

    pub fn preset(preset: MediaPreset, width: f64) -> Self {
        Self { kind: MediaKind::Preset { preset }, width, drawn: MediaOutline::default() }
    }

    pub fn calligraphic(angle: f64, width: f64) -> Self {
        Self { kind: MediaKind::Calligraphic { angle }, width, drawn: MediaOutline::default() }
    }

    fn key(&self, path: &BezPath) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        hash_path(&mut h, path, Vec2::ZERO);
        self.width.to_bits().hash(&mut h);
        match self.kind {
            MediaKind::Preset { preset } => (0u8, preset).hash(&mut h),
            MediaKind::Calligraphic { angle } => (1u8, angle.to_bits()).hash(&mut h),
        }
        h.finish()
    }

    /// The stroke's outline along `path` (both in the object's own space): made now,
    /// unless it was made for this path already.
    pub fn outline(&self, path: &BezPath) -> BezPath {
        match &self.drawn.path {
            Some(p) if self.drawn.key == self.key(path) => p.clone(),
            _ => self.make(path),
        }
    }

    pub(crate) fn sync(&mut self, path: &BezPath) {
        let key = self.key(path);
        if self.drawn.key != key || self.drawn.path.is_none() {
            self.drawn = MediaOutline { key, path: Some(self.make(path)) };
        }
    }

    /// The half-width at `t` along the curve, for a preset.
    fn half(&self, t: f64) -> f64 {
        match self.kind {
            MediaKind::Preset { preset } => self.width / 2.0 * preset.profile(t),
            MediaKind::Calligraphic { .. } => self.width * NIB_EDGE / 2.0,
        }
    }

    fn make(&self, path: &BezPath) -> BezPath {
        if !(self.width > 0.0 && self.width.is_finite()) {
            return BezPath::new();
        }
        let mut pieces = BezPath::new();
        for (points, closed) in polylines(path, self.width) {
            self.lay(&points, closed, &mut pieces);
        }
        if pieces.elements().is_empty() {
            return BezPath::new();
        }
        let Ok(contours) = binary_op(&pieces, &BezPath::new(), FillRule::NonZero, BinaryOp::Union) else {
            return BezPath::new();
        };
        // The pieces are fine enough that the union's polygon is within the accuracy;
        // fitting curves to it is not worth the stray control points it can make.
        let mut out = BezPath::new();
        for contour in contours.contours() {
            if contour.path.area().abs() < 1e-6 {
                continue;
            }
            let mut tidy = crate::shaping::tidy(&contour.path);
            if !matches!(tidy.elements().last(), Some(PathEl::ClosePath)) {
                tidy.close_path();
            }
            out.extend(tidy.iter());
        }
        out
    }

    /// The pieces of the stroke along one subpath.
    fn lay(&self, p: &[Point], closed: bool, out: &mut BezPath) {
        let n = p.len();
        if n < 2 {
            if let Some(&c) = p.first() {
                disc(c, self.half(0.5).max(self.width * NIB_EDGE / 2.0), out);
            }
            return;
        }
        let segs = if closed { n } else { n - 1 };
        let seg = |j: usize| (p[j], p[(j + 1) % n]);
        let normal = |j: usize| {
            let (a, b) = seg(j);
            let d = b - a;
            Vec2::new(-d.y, d.x) / d.hypot()
        };
        // Arc length along the curve, for the preset's profile.
        let mut s = vec![0.0; n + 1];
        for j in 0..segs {
            let (a, b) = seg(j);
            s[j + 1] = s[j] + (b - a).hypot();
        }
        let total = s[segs].max(1e-9);
        let half = |i: usize| self.half(s[i] / total);
        // Where two pieces meet at a corner, or (smoothly) the normal they share.
        let joint = |i: usize| -> Option<Vec2> {
            let (before, after) = match (closed, i) {
                (false, 0) => return Some(normal(0)),
                (false, i) if i == n - 1 => return Some(normal(segs - 1)),
                (_, 0) => (normal(segs - 1), normal(0)),
                (_, i) => (normal(i - 1), normal(i)),
            };
            let turn = before.cross(after).atan2(before.dot(after)).abs();
            (turn < CORNER).then(|| (before + after).normalize())
        };
        let joints: Vec<Option<Vec2>> = (0..n).map(joint).collect();
        let nib = match self.kind {
            MediaKind::Calligraphic { angle } => {
                let a = angle.to_radians();
                Some(Vec2::new(a.cos(), -a.sin()) * self.width / 2.0)
            }
            MediaKind::Preset { .. } => None,
        };
        for j in 0..segs {
            let (i, k) = (j, (j + 1) % n);
            let (a, b) = seg(j);
            let own = normal(j);
            let (ha, hb) = (half(i), if k == 0 { half(segs) } else { half(k) });
            let (na, nb) = (joints[i].unwrap_or(own) * ha, joints[k].unwrap_or(own) * hb);
            quad([a + na, b + nb, b - nb, a - na], out);
            if let Some(o) = nib {
                quad([a + o, b + o, b - o, a - o], out);
            }
        }
        for i in 0..n {
            if joints[i].is_none() {
                disc(p[i], half(i), out);
            }
        }
        if !closed && nib.is_none() {
            disc(p[0], half(0), out);
            disc(p[n - 1], half(segs), out);
        }
    }
}

/// `path` as polylines (and whether each is closed), with no piece longer than a share
/// of the stroke `width`, so the width changes smoothly along it.
fn polylines(path: &BezPath, width: f64) -> Vec<(Vec<Point>, bool)> {
    let mut lines: Vec<(Vec<Point>, bool)> = Vec::new();
    kurbo::flatten(path.iter(), ACCURACY / 2.0, |el| match el {
        PathEl::MoveTo(p) => lines.push((vec![p], false)),
        PathEl::LineTo(p) => {
            if let Some((l, _)) = lines.last_mut() {
                l.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some((_, c)) = lines.last_mut() {
                *c = true;
            }
        }
        _ => {}
    });
    let length: f64 = lines.iter().flat_map(|(l, _)| l.windows(2).map(|w| (w[1] - w[0]).hypot())).sum();
    let step = (width / 6.0).clamp(0.25, 4.0).max(length / MAX_PIECES);
    lines
        .into_iter()
        .map(|(l, closed)| {
            let mut out: Vec<Point> = Vec::with_capacity(l.len());
            for q in l {
                let Some(&last) = out.last() else {
                    out.push(q);
                    continue;
                };
                let d = (q - last).hypot();
                if d < 1e-9 {
                    continue;
                }
                let cuts = (d / step).ceil() as usize;
                for c in 1..=cuts {
                    out.push(last.lerp(q, c as f64 / cuts as f64));
                }
            }
            if closed && out.len() > 2 && (out[0] - out[out.len() - 1]).hypot() < 1e-9 {
                out.pop();
            }
            (out, closed)
        })
        .collect()
}

/// A convex polygon, counterclockwise, so the union fills it whatever way it was given.
fn convex(points: &[Point], out: &mut BezPath) {
    let area: f64 = (0..points.len()).map(|i| points[i].to_vec2().cross(points[(i + 1) % points.len()].to_vec2())).sum();
    if area.abs() < 1e-12 {
        return;
    }
    let mut it: Box<dyn Iterator<Item = &Point>> = if area > 0.0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
    if let Some(first) = it.next() {
        out.move_to(*first);
        for q in it {
            out.line_to(*q);
        }
        out.close_path();
    }
}

/// A quadrilateral as two triangles, each convex even if the four corners cross.
fn quad(c: [Point; 4], out: &mut BezPath) {
    convex(&[c[0], c[1], c[2]], out);
    convex(&[c[0], c[2], c[3]], out);
}

fn disc(c: Point, r: f64, out: &mut BezPath) {
    if r < 1e-6 {
        return;
    }
    let sides = (std::f64::consts::PI / (1.0 - (ACCURACY / r).min(1.0)).acos()).ceil().clamp(8.0, 128.0) as usize;
    let points: Vec<Point> = (0..sides)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / sides as f64;
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    convex(&points, out);
}

impl Object {
    /// Whether an Artistic Media stroke can go on this object: an unlocked shape or curve
    /// (not text, a group, a bitmap or a PowerClip).
    pub fn can_take_media(&self) -> bool {
        !self.locked && self.can_take_media_shape()
    }

    /// The Artistic Media stroke drawn for this object's own shape, in its own space.
    pub fn media_outline(&self) -> Option<BezPath> {
        let media = self.effects.media.as_ref()?;
        self.can_take_media_shape().then(|| media.outline(&self.shape.to_path()))
    }

    fn can_take_media_shape(&self) -> bool {
        self.contents.is_empty() && !matches!(self.shape, Shape::Text(_) | Shape::Group { .. } | Shape::Image(_))
    }

    pub(crate) fn sync_media(&mut self) {
        if !self.can_take_media_shape() {
            return;
        }
        let path = self.shape.to_path();
        if let Some(media) = self.effects.media.as_mut() {
            media.sync(&path);
        }
    }

    /// Make the stroke the object's own shape: a closed curve, with no stroke on it.
    pub(crate) fn bake_media(&mut self) {
        if let Some(path) = self.media_outline() {
            self.shape = Shape::Curve { path };
        }
        self.effects.media = None;
    }

    /// Object > Break Artistic Media Apart: the control curve, outlined and unfilled, and
    /// the stroke as a closed curve with the object's style. `None` without a stroke.
    pub fn split_media(&self) -> Option<(Object, Object)> {
        let path = self.media_outline()?;
        let mut stroke = self.clone();
        stroke.effects.media = None;
        stroke.shape = Shape::Curve { path };
        let mut control = self.clone();
        control.effects.media = None;
        control.style = crate::document::Style::new_object();
        Some((control, stroke))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Rgba, Style};
    use kurbo::Rect;

    fn line(a: Point, b: Point) -> BezPath {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        p
    }

    fn area(p: &BezPath) -> f64 {
        p.area().abs()
    }

    #[test]
    fn an_even_stroke_is_its_width_wide_with_round_ends() {
        let m = ArtisticMedia::preset(MediaPreset::Even, 10.0);
        let out = m.outline(&line(Point::new(0.0, 0.0), Point::new(100.0, 0.0)));
        let b = out.bounding_box();
        assert!((b.x0 + 5.0).abs() < 0.1 && (b.x1 - 105.0).abs() < 0.1, "round ends reach half the width past the ends: {b:?}");
        assert!((b.y0 + 5.0).abs() < 0.1 && (b.y1 - 5.0).abs() < 0.1, "{b:?}");
        let want = 100.0 * 10.0 + std::f64::consts::PI * 25.0;
        assert!((area(&out) - want).abs() < want * 0.01, "{} vs {want}", area(&out));
        assert!(out.contains(Point::new(50.0, 4.0)) && !out.contains(Point::new(50.0, 6.0)));
    }

    #[test]
    fn presets_follow_their_profile() {
        let path = line(Point::new(0.0, 0.0), Point::new(200.0, 0.0));
        let width_at = |m: &ArtisticMedia, x: f64| {
            let out = m.outline(&path);
            (0..=400).map(|i| -10.0 + i as f64 * 0.05).filter(|y| out.contains(Point::new(x, *y))).count() as f64 * 0.05
        };
        let both = ArtisticMedia::preset(MediaPreset::TaperBoth, 20.0);
        assert!((width_at(&both, 100.0) - 20.0).abs() < 0.3, "full in the middle");
        assert!(width_at(&both, 2.0) < 6.0, "thin at the start: {}", width_at(&both, 2.0));
        assert!(width_at(&both, 198.0) < 6.0, "and the end");
        let start = ArtisticMedia::preset(MediaPreset::TaperStart, 20.0);
        assert!(width_at(&start, 2.0) < 6.0 && (width_at(&start, 199.0) - 20.0).abs() < 0.5);
        let end = ArtisticMedia::preset(MediaPreset::TaperEnd, 20.0);
        assert!((width_at(&end, 1.0) - 20.0).abs() < 0.5 && width_at(&end, 198.0) < 6.0);
        let swell = ArtisticMedia::preset(MediaPreset::Swell, 20.0);
        assert!(width_at(&swell, 100.0) > 19.5 && width_at(&swell, 1.0) < 8.0);
    }

    #[test]
    fn a_calligraphic_nib_is_wide_across_and_thin_along() {
        let m = ArtisticMedia::calligraphic(0.0, 20.0);
        // The nib lies flat: a vertical stroke shows its whole width, a flat one hardly any.
        let down = m.outline(&line(Point::new(0.0, 0.0), Point::new(0.0, 100.0))).bounding_box();
        assert!((down.width() - 20.0).abs() < 0.1, "{down:?}");
        let across = m.outline(&line(Point::new(0.0, 0.0), Point::new(100.0, 0.0))).bounding_box();
        assert!(across.height() < 2.0 && across.height() > 0.5, "thin but there: {across:?}");
        let tilted = ArtisticMedia::calligraphic(90.0, 20.0).outline(&line(Point::new(0.0, 0.0), Point::new(100.0, 0.0))).bounding_box();
        assert!((tilted.height() - 20.0).abs() < 0.1, "a standing nib makes a flat stroke wide: {tilted:?}");
    }

    #[test]
    fn loops_and_corners_fill_without_holes() {
        // A tight zigzag and a loop crossing itself: every point on the curve is inside.
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        for (x, y) in [(10.0, 30.0), (20.0, 0.0), (30.0, 30.0), (80.0, 30.0), (60.0, -10.0), (40.0, 40.0)] {
            path.line_to((x, y));
        }
        for m in [ArtisticMedia::preset(MediaPreset::Even, 12.0), ArtisticMedia::preset(MediaPreset::Swell, 16.0), ArtisticMedia::calligraphic(30.0, 14.0)] {
            let out = m.outline(&path);
            for seg in path.segments() {
                for i in 1..20 {
                    let q = kurbo::ParamCurve::eval(&seg, i as f64 / 20.0);
                    assert!(out.winding(q) != 0, "{m:?}: {q:?} is outside");
                    assert!(out.winding(q).abs() == 1, "no overlapping contours: {q:?}");
                }
            }
        }
    }

    #[test]
    fn a_closed_curve_gives_a_ring() {
        let ring = ArtisticMedia::preset(MediaPreset::Even, 10.0).outline(&Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1));
        assert!(ring.contains(Point::new(0.0, 50.0)) && ring.contains(Point::new(103.0, 50.0)));
        assert!(!ring.contains(Point::new(50.0, 50.0)), "hollow");
    }

    #[test]
    fn the_object_draws_its_stroke_and_bakes_it() {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Curve { path: line(Point::new(10.0, 10.0), Point::new(110.0, 10.0)) }, Style::filled(Rgba::BLACK));
        let obj = doc.active_page_mut().object_mut(id).unwrap();
        obj.effects.media = Some(ArtisticMedia::preset(MediaPreset::Even, 8.0));
        doc.sync_effects();
        let obj = doc.active_page().object(id).unwrap();
        let leaves = obj.leaves();
        assert_eq!(leaves.len(), 1);
        assert!(leaves[0].closed && leaves[0].fill().is_some(), "the stroke is filled");
        assert!(obj.hit(Point::new(60.0, 13.0), 0.0), "inside the stroke");
        assert!(!obj.hit(Point::new(60.0, 16.0), 0.5));
        let b = obj.world_bounds();
        assert!((b.y0 - 6.0).abs() < 0.1 && (b.x1 - 114.0).abs() < 0.1, "bounds take in the stroke: {b:?}");
        assert!(obj.can_convert_to_curves());

        let (control, stroke) = obj.split_media().unwrap();
        assert_eq!(control.shape, obj.shape, "the control curve");
        assert!(control.style.fill.is_none() && control.effects.media.is_none());
        assert!(matches!(&stroke.shape, Shape::Curve { path } if (path.bounding_box().height() - 8.0).abs() < 0.1));

        doc.convert_to_curves(&[id]);
        let obj = doc.active_page().object(id).unwrap();
        assert!(obj.effects.media.is_none());
        assert!((obj.world_bounds().height() - 8.0).abs() < 0.1, "the stroke became the curve");
    }

    #[test]
    fn break_apart_splits_the_stroke_from_its_control_curve() {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Curve { path: line(Point::new(10.0, 10.0), Point::new(110.0, 10.0)) }, Style::filled(Rgba::BLACK));
        doc.active_page_mut().object_mut(id).unwrap().effects.media = Some(ArtisticMedia::preset(MediaPreset::Even, 8.0));
        assert!(doc.can_break_apart(&[id]));
        let made = doc.break_apart(&[id]);
        assert_eq!(made.len(), 2);
        assert_eq!(made[0], id, "the stroke keeps the id");
        let objects = &doc.active_page().objects;
        assert_eq!(objects.iter().map(|o| o.id).collect::<Vec<_>>(), made, "the control curve on top");
        assert!(objects.iter().all(|o| o.effects.media.is_none()));
        assert!(objects[0].shape.is_closed() && objects[0].style.fill.is_some());
        assert!(!objects[1].shape.is_closed() && objects[1].style.outline.is_some());
    }

    #[test]
    fn media_is_saved_with_the_drawing() {
        let m = ArtisticMedia::calligraphic(30.0, 12.5);
        let json = serde_json::to_string(&m).unwrap();
        let back: ArtisticMedia = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.kind, MediaKind::Calligraphic { angle: 30.0 });
        let p: ArtisticMedia = serde_json::from_str(&serde_json::to_string(&ArtisticMedia::preset(MediaPreset::TaperEnd, 4.0)).unwrap()).unwrap();
        assert_eq!(p.kind, MediaKind::Preset { preset: MediaPreset::TaperEnd });
    }
}

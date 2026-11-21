//! Distort: Push and Pull, Zipper and Twister distortions, kept as settings and applied as
//! the object is drawn.

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveDeriv, Point, Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::effects::{cubic_path, cubic_subpaths};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistortionKind {
    /// Nodes pushed in toward the centre (petals) or pulled out (spikes).
    #[default]
    PushPull,
    /// Every segment turned into a zigzag.
    Zipper,
    /// Turned about the centre, more the farther out.
    Twister,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Distortion {
    pub kind: DistortionKind,
    /// In the object's own space.
    pub center: Point,
    /// Push and Pull: -200 (push) to 200 (pull), in percent. Zipper: 0 to 100, as a
    /// percentage of half the smaller side of the object.
    pub amplitude: f64,
    /// Zipper: zigzags per segment, 0 to 100.
    pub frequency: u32,
    /// Twister: how far the outer edge turns, in degrees, clockwise.
    pub angle: f64,
}

/// Pieces each segment is cut into for maps that bend straight lines.
const PIECES: usize = 8;

impl Distortion {
    pub const MAX_PUSH_PULL: f64 = 200.0;
    pub const MAX_ZIPPER: f64 = 100.0;
    pub const MAX_FREQUENCY: u32 = 100;
    pub const MAX_ANGLE: f64 = 3600.0;

    pub fn new(kind: DistortionKind, center: Point) -> Self {
        Self { kind, center, amplitude: 0.0, frequency: 5, angle: 0.0 }
    }

    /// `path` distorted; `frame` is the object's bounds before it.
    pub fn map_path(&self, frame: Rect, path: &BezPath) -> BezPath {
        let c = self.center;
        let mut out = BezPath::new();
        for (segs, closed) in cubic_subpaths(path) {
            let segs: Vec<CubicBez> = match self.kind {
                DistortionKind::PushPull => {
                    let k = self.amplitude.clamp(-Self::MAX_PUSH_PULL, Self::MAX_PUSH_PULL) / 100.0;
                    let node = |p: Point| c + (p - c) * (1.0 + k);
                    let handle = |p: Point| c + (p - c) * (1.0 - k);
                    segs.iter().map(|s| CubicBez::new(node(s.p0), handle(s.p1), handle(s.p2), node(s.p3))).collect()
                }
                DistortionKind::Zipper => {
                    let amp = self.amplitude.clamp(0.0, Self::MAX_ZIPPER) / 100.0 * frame.width().min(frame.height()) / 2.0;
                    let f = self.frequency.min(Self::MAX_FREQUENCY) as usize;
                    if f == 0 || amp == 0.0 {
                        segs
                    } else {
                        segs.iter().flat_map(|s| zigzag(s, f, amp)).collect()
                    }
                }
                DistortionKind::Twister => {
                    let reach = [frame.origin(), Point::new(frame.x1, frame.y0), Point::new(frame.x1, frame.y1), Point::new(frame.x0, frame.y1)]
                        .into_iter()
                        .map(|p| (p - c).hypot())
                        .fold(0.0, f64::max)
                        .max(1e-9);
                    let turn = self.angle.clamp(-Self::MAX_ANGLE, Self::MAX_ANGLE).to_radians();
                    let twist = |p: Point| {
                        let d = p - c;
                        let a = turn * d.hypot() / reach;
                        c + Vec2::new(d.x * a.cos() - d.y * a.sin(), d.x * a.sin() + d.y * a.cos())
                    };
                    segs.iter().flat_map(pieces).map(|s| CubicBez::new(twist(s.p0), twist(s.p1), twist(s.p2), twist(s.p3))).collect()
                }
            };
            cubic_path(&segs, closed, &mut out);
        }
        out
    }
}

fn pieces(s: &CubicBez) -> impl Iterator<Item = CubicBez> + '_ {
    (0..PIECES).map(move |i| s.subsegment(i as f64 / PIECES as f64..(i + 1) as f64 / PIECES as f64))
}

/// Segment `s` as `f` zigzags `amp` either side of it, as straight lines.
fn zigzag(s: &CubicBez, f: usize, amp: f64) -> Vec<CubicBez> {
    let deriv = s.deriv();
    let n = 2 * f;
    let mut points = vec![s.p0];
    for j in 0..n {
        let t = (j as f64 + 0.5) / n as f64;
        let d = deriv.eval(t).to_vec2();
        let normal = if d.hypot() > 1e-12 { Vec2::new(-d.y, d.x).normalize() } else { Vec2::ZERO };
        let side = if j % 2 == 0 { 1.0 } else { -1.0 };
        points.push(s.eval(t) + normal * amp * side);
    }
    points.push(s.p3);
    points
        .windows(2)
        .map(|w| CubicBez::new(w[0], w[0].lerp(w[1], 1.0 / 3.0), w[0].lerp(w[1], 2.0 / 3.0), w[1]))
        .collect()
}

//! Envelope: an object reshaped to fit a frame whose sides are bent. The frame starts as
//! the object's bounds, with a node at each corner and in the middle of each side; the
//! object is stretched to follow the nodes wherever they are dragged.

use kurbo::{BezPath, Point, Rect, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeMode {
    /// Each side is two straight lines through its middle node.
    #[default]
    Straight,
    /// Each side is a smooth arc through its middle node.
    Arc,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// In the object's own space, clockwise from the top left: corner, middle of the top,
    /// corner, middle of the right side, and so on.
    pub nodes: [Point; 8],
    pub mode: EnvelopeMode,
}

impl Envelope {
    /// The envelope that leaves an object whose bounds are `frame` as it is.
    pub fn around(frame: Rect, mode: EnvelopeMode) -> Self {
        let (x0, x1, y0, y1, cx, cy) = (frame.x0, frame.x1, frame.y0, frame.y1, frame.center().x, frame.center().y);
        let nodes = [(x0, y0), (cx, y0), (x1, y0), (x1, cy), (x1, y1), (cx, y1), (x0, y1), (x0, cy)].map(Point::from);
        Self { nodes, mode }
    }

    /// Whether node `i` is a corner (the others are the middles of the sides).
    pub fn is_corner(i: usize) -> bool {
        i.is_multiple_of(2)
    }

    /// Move node `i` to `to`. A corner takes the middle nodes beside it half as far, so
    /// straight sides stay straight.
    pub fn move_node(&mut self, i: usize, to: Point) {
        let Some(node) = self.nodes.get(i).copied() else { return };
        let d = to - node;
        self.nodes[i] = to;
        if Self::is_corner(i) {
            self.nodes[(i + 1) % 8] += d / 2.0;
            self.nodes[(i + 7) % 8] += d / 2.0;
        }
    }

    /// The point on the side from `a` through `m` to `b`, `t` of the way along.
    fn side(&self, a: Point, m: Point, b: Point, t: f64) -> Vec2 {
        match self.mode {
            EnvelopeMode::Straight if t < 0.5 => a.lerp(m, 2.0 * t).to_vec2(),
            EnvelopeMode::Straight => m.lerp(b, 2.0 * t - 1.0).to_vec2(),
            EnvelopeMode::Arc => {
                let c = 2.0 * m.to_vec2() - (a.to_vec2() + b.to_vec2()) / 2.0;
                let s = 1.0 - t;
                s * s * a.to_vec2() + 2.0 * s * t * c + t * t * b.to_vec2()
            }
        }
    }

    /// Where `p` (in `frame`, the object's bounds before the envelope) goes: a Coons patch
    /// between the four sides.
    pub fn map(&self, frame: Rect, p: Point) -> Point {
        let along = |v: f64, lo: f64, len: f64| if len.abs() > 1e-9 { (v - lo) / len } else { 0.5 };
        let (u, v) = (along(p.x, frame.x0, frame.width()), along(p.y, frame.y0, frame.height()));
        let [tl, t, tr, r, br, b, bl, l] = self.nodes;
        let top = self.side(tl, t, tr, u);
        let bottom = self.side(bl, b, br, u);
        let left = self.side(tl, l, bl, v);
        let right = self.side(tr, r, br, v);
        let corners = (1.0 - u) * (1.0 - v) * tl.to_vec2() + u * (1.0 - v) * tr.to_vec2() + (1.0 - u) * v * bl.to_vec2() + u * v * br.to_vec2();
        ((1.0 - v) * top + v * bottom + (1.0 - u) * left + u * right - corners).to_point()
    }

    /// The envelope's frame, in the object's own space.
    pub fn outline(&self, frame: Rect) -> BezPath {
        let mut out = BezPath::new();
        let steps = 32;
        let edge = |i: usize| -> Vec<Point> {
            (0..steps).map(|k| {
                let t = k as f64 / steps as f64;
                match i {
                    0 => Point::new(frame.x0 + t * frame.width(), frame.y0),
                    1 => Point::new(frame.x1, frame.y0 + t * frame.height()),
                    2 => Point::new(frame.x1 - t * frame.width(), frame.y1),
                    _ => Point::new(frame.x0, frame.y1 - t * frame.height()),
                }
            }).collect()
        };
        for (n, p) in (0..4).flat_map(edge).enumerate() {
            let q = self.map(frame, p);
            if n == 0 { out.move_to(q) } else { out.line_to(q) }
        }
        out.close_path();
        out
    }
}

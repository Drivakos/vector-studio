//! Block Shadow: a solid shadow, the object's silhouette swept along a direction, drawn
//! under it in one colour (as for screen printing or vinyl cutting).

use kurbo::{Affine, BezPath, PathEl, Point, Vec2};
use linesweeper::BinaryOp;
use serde::{Deserialize, Serialize};

use crate::document::{Object, Rgba, Style};
use crate::effects::{Generated, Maker, Source, piece};
use crate::shaping::boolean;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockShadow {
    /// From the object to the far end of the shadow, in points, in the space the object
    /// is placed in.
    pub offset: Vec2,
    pub color: Rgba,
    #[serde(skip)]
    pub generated: Generated,
}

impl BlockShadow {
    pub fn new(offset: Vec2) -> Self {
        Self { offset, color: Rgba::BLACK, generated: Generated::default() }
    }
}

impl Maker for BlockShadow {
    fn generated(&mut self) -> &mut Generated {
        &mut self.generated
    }

    fn make(&self, source: &Source) -> Vec<Object> {
        let v = self.offset;
        let base = source.silhouette();
        if base.is_empty() || v.hypot() < 1e-6 {
            return Vec::new();
        }
        // The silhouette's edges swept along `v`: one quad per (flattened) edge, all
        // turning the same way so the non-zero rule unites them.
        let mut quads = BezPath::new();
        let mut edge = |a: Point, b: Point| {
            let q = [a, b, b + v, a + v];
            let area: f64 = (0..4).map(|i| q[i].to_vec2().cross(q[(i + 1) % 4].to_vec2())).sum();
            if area.abs() < 1e-9 {
                return;
            }
            let q = if area > 0.0 { q } else { [a + v, b + v, b, a] };
            quads.move_to(q[0]);
            q[1..].iter().for_each(|p| quads.line_to(*p));
            quads.close_path();
        };
        let (mut start, mut last) = (Point::ZERO, Point::ZERO);
        kurbo::flatten(&base, 0.1, |el| match el {
            PathEl::MoveTo(p) => (start, last) = (p, p),
            PathEl::LineTo(p) => {
                edge(last, p);
                last = p;
            }
            PathEl::ClosePath => {
                edge(last, start);
                last = start;
            }
            _ => {}
        });
        let mut moved = base.clone();
        moved.apply_affine(Affine::translate(v));
        let swept = crate::curves::nonzero_region(&quads).unwrap_or_default();
        let Some(ends) = boolean(&base, &moved, BinaryOp::Union) else { return Vec::new() };
        let Some(path) = boolean(&ends, &swept, BinaryOp::Union) else { return Vec::new() };
        vec![piece(path, Style::filled(self.color))]
    }
}

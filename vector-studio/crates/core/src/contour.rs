//! Contour: evenly spaced copies of an object's outline, inside or outside it, whose fill
//! and outline colours run from the object's to the contour's own.

use kurbo::{BezPath, Shape as _};
use linesweeper::BinaryOp;
use serde::{Deserialize, Serialize};

use crate::document::{Object, Rgba, Style};
use crate::effects::{Generated, Maker, Source, piece};
use crate::fill::Fill;
use crate::shaping::boolean;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContourDirection {
    /// Inside, step after step until nothing is left.
    ToCenter,
    Inside,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    pub direction: ContourDirection,
    /// Copies made (inside and outside).
    pub steps: u32,
    /// Distance between copies, in points.
    pub offset: f64,
    /// The last copy's fill colour; copies between go from the object's to it.
    pub fill: Rgba,
    /// The last copy's outline colour.
    pub outline: Rgba,
    #[serde(skip)]
    pub generated: Generated,
}

impl Contour {
    /// This app's starting settings, close to CorelDRAW's.
    pub const STEPS: u32 = 3;
    pub const OFFSET: f64 = 5.0;
    pub const MAX_STEPS: u32 = 999;
    /// Most copies To Center makes, so a tiny offset stays quick.
    const MAX_TO_CENTER: u32 = 200;

    pub fn new(direction: ContourDirection) -> Self {
        Self { direction, steps: Self::STEPS, offset: Self::OFFSET, fill: Rgba::WHITE, outline: Rgba::BLACK, generated: Generated::default() }
    }

    /// The copies' outlines in the space of `area`, the first copy first.
    fn outlines(&self, area: &BezPath) -> Vec<BezPath> {
        let offset = self.offset.max(0.01);
        let n = match self.direction {
            ContourDirection::ToCenter => Self::MAX_TO_CENTER,
            _ => self.steps.clamp(1, Self::MAX_STEPS),
        };
        let mut out = Vec::new();
        for i in 1..=n {
            let d = offset * f64::from(i);
            let stroke = kurbo::Stroke::new(2.0 * d).with_join(kurbo::Join::Round);
            let ring = kurbo::stroke(area.iter(), &stroke, &kurbo::StrokeOpts::default(), 0.05);
            let Some(ring) = crate::curves::nonzero_region(&ring) else { break };
            let op = if self.direction == ContourDirection::Outside { BinaryOp::Union } else { BinaryOp::Difference };
            match boolean(area, &ring, op) {
                Some(path) if path.area().abs() > 1e-3 => out.push(path),
                _ => break,
            }
        }
        out
    }
}

impl Maker for Contour {
    fn generated(&mut self) -> &mut Generated {
        &mut self.generated
    }

    fn make(&self, source: &Source) -> Vec<Object> {
        let area = source.area();
        if area.is_empty() {
            return Vec::new();
        }
        let style = source.style();
        let outlines = self.outlines(&area);
        let n = outlines.len() as f64;
        let mut out: Vec<Object> = outlines
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                let t = (i + 1) as f64 / n;
                let fill = style.fill.as_ref().map(|f| Fill::Solid(f.color().mix(self.fill, t)));
                let outline = style.outline.clone().map(|o| crate::Outline { color: o.color.mix(self.outline, t), ..o });
                piece(path, Style { fill, outline, transparency: style.transparency, ..Style::default() })
            })
            .collect();
        if self.direction == ContourDirection::Outside {
            // The largest at the bottom.
            out.reverse();
        }
        out
    }
}

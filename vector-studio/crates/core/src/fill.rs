//! Fills: uniform colour or a fountain (gradient) fill.

use kurbo::{Point, Rect};
use serde::{Deserialize, Serialize};

use crate::document::Rgba;

/// How the colours of a fountain fill spread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    /// Colour changes along the line from `start` to `end`.
    #[default]
    Linear,
    /// Colour changes outward from `start`; `end` is on the outer circle.
    /// Also known as elliptical.
    Radial,
}

impl GradientKind {
    pub fn label(self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Elliptical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// 0 at `start`, 1 at `end`.
    pub offset: f64,
    pub color: Rgba,
}

/// A fountain fill. `start` and `end` are in the object's local space, so the fill moves,
/// scales and turns with the object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    pub start: Point,
    pub end: Point,
    /// Sorted by offset; at least two.
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// From `from` to `to`, across `bounds` left to right (linear) or from its centre out
    /// to its right edge (radial).
    pub fn across(kind: GradientKind, bounds: Rect, from: Rgba, to: Rgba) -> Self {
        let (start, end) = match kind {
            GradientKind::Linear => (Point::new(bounds.x0, bounds.center().y), Point::new(bounds.x1, bounds.center().y)),
            GradientKind::Radial => (bounds.center(), Point::new(bounds.x1, bounds.center().y)),
        };
        Self { kind, start, end, stops: vec![GradientStop { offset: 0.0, color: from }, GradientStop { offset: 1.0, color: to }] }
    }

    /// Position along the fill at `p` (local space), clamped to 0..=1.
    pub fn offset_at(&self, p: Point) -> f64 {
        let d = self.end - self.start;
        let t = match self.kind {
            GradientKind::Linear => {
                let len2 = d.hypot2();
                if len2 < 1e-18 { 0.0 } else { (p - self.start).dot(d) / len2 }
            }
            GradientKind::Radial => {
                let r = d.hypot();
                if r < 1e-9 { 0.0 } else { (p - self.start).hypot() / r }
            }
        };
        t.clamp(0.0, 1.0)
    }

    /// The colour the fill shows at `p` (local space).
    pub fn color_at(&self, p: Point) -> Rgba {
        let t = self.offset_at(p);
        let (Some(first), Some(last)) = (self.stops.first(), self.stops.last()) else { return Rgba::BLACK };
        if t <= first.offset {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t <= b.offset {
                let k = if b.offset - a.offset < 1e-12 { 1.0 } else { (t - a.offset) / (b.offset - a.offset) };
                return mix(a.color, b.color, k);
            }
        }
        last.color
    }
}

fn mix(a: Rgba, b: Rgba, k: f64) -> Rgba {
    let l = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * k).round() as u8;
    Rgba { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }
}

/// What fills a closed object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "FillFile")]
pub enum Fill {
    /// Uniform fill.
    Solid(Rgba),
    /// Fountain fill.
    Gradient(Gradient),
}

impl From<Rgba> for Fill {
    fn from(c: Rgba) -> Self {
        Fill::Solid(c)
    }
}

impl Fill {
    /// The uniform colour, if this is a uniform fill.
    pub fn solid(&self) -> Option<Rgba> {
        match self {
            Fill::Solid(c) => Some(*c),
            Fill::Gradient(_) => None,
        }
    }

    /// One colour standing for the fill: the uniform colour or the first stop.
    pub fn color(&self) -> Rgba {
        match self {
            Fill::Solid(c) => *c,
            Fill::Gradient(g) => g.stops.first().map_or(Rgba::BLACK, |s| s.color),
        }
    }

    /// The colour shown at `p` (local space).
    pub fn color_at(&self, p: Point) -> Rgba {
        match self {
            Fill::Solid(c) => *c,
            Fill::Gradient(g) => g.color_at(p),
        }
    }
}

/// Files from before fountain fills stored a fill as a bare colour.
#[derive(Deserialize)]
#[serde(untagged)]
enum FillFile {
    Color(Rgba),
    Tagged(FillTagged),
}

#[derive(Deserialize)]
enum FillTagged {
    Solid(Rgba),
    Gradient(Gradient),
}

impl From<FillFile> for Fill {
    fn from(f: FillFile) -> Self {
        match f {
            FillFile::Color(c) | FillFile::Tagged(FillTagged::Solid(c)) => Fill::Solid(c),
            FillFile::Tagged(FillTagged::Gradient(g)) => Fill::Gradient(g),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::rgb(255, 0, 0);
    const BLUE: Rgba = Rgba::rgb(0, 0, 255);

    #[test]
    fn linear_and_radial_colours_along_the_fill() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        let g = Gradient::across(GradientKind::Linear, bounds, RED, BLUE);
        assert_eq!(g.color_at(Point::new(-10.0, 0.0)), RED, "before the start: first colour");
        assert_eq!(g.color_at(Point::new(50.0, 40.0)), Rgba::rgb(128, 0, 128));
        assert_eq!(g.color_at(Point::new(200.0, 0.0)), BLUE);
        let r = Gradient::across(GradientKind::Radial, bounds, RED, BLUE);
        assert_eq!(r.color_at(Point::new(50.0, 25.0)), RED, "centre");
        assert_eq!(r.color_at(Point::new(50.0, 75.0)), BLUE, "50 out: the outer circle");
    }

    #[test]
    fn old_bare_colours_and_new_fills_load() {
        let old: Fill = serde_json::from_str(r#"{"r":1,"g":2,"b":3,"a":255}"#).unwrap();
        assert_eq!(old, Fill::Solid(Rgba::rgb(1, 2, 3)));
        let g = Fill::Gradient(Gradient::across(GradientKind::Radial, Rect::new(0.0, 0.0, 10.0, 10.0), RED, BLUE));
        let back: Fill = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        let solid: Fill = serde_json::from_str(&serde_json::to_string(&Fill::Solid(RED)).unwrap()).unwrap();
        assert_eq!(solid, Fill::Solid(RED));
    }
}

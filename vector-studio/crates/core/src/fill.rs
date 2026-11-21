//! Fills: uniform colour, a fountain (gradient) fill or a pattern fill.

use kurbo::{Affine, Point, Rect, Size};
use serde::{Deserialize, Serialize};

use crate::bitmap::Bitmap;
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
                return a.color.mix(b.color, k);
            }
        }
        last.color
    }
}

/// What fills a closed object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "FillFile")]
pub enum Fill {
    /// Uniform fill.
    Solid(Rgba),
    /// Fountain fill.
    Gradient(Gradient),
    /// Two-color or bitmap pattern fill.
    Pattern(Pattern),
}

/// The two-color patterns offered, by name. This app's own set.
pub const TWO_COLOR_PATTERNS: &[(&str, [u8; 8])] = &[
    ("Checkerboard", [0xF0, 0xF0, 0xF0, 0xF0, 0x0F, 0x0F, 0x0F, 0x0F]),
    ("Fine checkerboard", [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55]),
    ("Horizontal stripes", [0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00]),
    ("Vertical stripes", [0xF0; 8]),
    ("Diagonal stripes", [0xC0, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x03, 0x81]),
    ("Reverse diagonal stripes", [0x03, 0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0x81]),
    ("Grid", [0xFF, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]),
    ("Cross-hatch", [0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81]),
    ("Dots", [0x00, 0x00, 0x18, 0x3C, 0x3C, 0x18, 0x00, 0x00]),
    ("Bricks", [0xFF, 0x80, 0x80, 0x80, 0xFF, 0x08, 0x08, 0x08]),
    ("Diamonds", [0x10, 0x38, 0x7C, 0xFE, 0x7C, 0x38, 0x10, 0x00]),
    ("Zigzag", [0x00, 0x81, 0x42, 0x24, 0x18, 0x00, 0x00, 0x00]),
];

/// What a pattern tile holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PatternKind {
    /// 8 × 8 cells: a byte per row from the top, its highest bit leftmost. A set bit shows
    /// `fore`, a clear one `back`.
    TwoColor { cells: [u8; 8], fore: Rgba, back: Rgba },
    /// A bitmap stretched over the tile; `average` stands for it where one colour is
    /// needed (blends, contours, the eyedropper).
    Bitmap { bitmap: Bitmap, average: Rgba },
}

/// A pattern fill: one tile repeated across and down the object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    /// Takes the unit square (one tile) to the object's local space, so the pattern
    /// moves, scales and turns with the object.
    pub transform: Affine,
}

impl Pattern {
    /// Tile size of a new pattern fill, in points (this app's choice).
    pub const DEFAULT_TILE: f64 = 18.0;

    /// Tiles of `size` with one corner at `origin`, square to the page.
    pub fn new(kind: PatternKind, origin: Point, size: Size) -> Self {
        Self { kind, transform: Affine::translate(origin.to_vec2()) * Affine::scale_non_uniform(size.width, size.height) }
    }

    /// The size of one tile in the object's local space.
    pub fn tile_size(&self) -> Size {
        let [a, b, c, d, ..] = self.transform.as_coeffs();
        Size::new(a.hypot(b), c.hypot(d))
    }

    /// Make the tiles `size`, keeping where they start and how they are turned.
    pub fn set_tile_size(&mut self, size: Size) {
        let now = self.tile_size();
        if now.width < 1e-12 || now.height < 1e-12 || size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let [a, b, c, d, e, f] = self.transform.as_coeffs();
        let (kx, ky) = (size.width / now.width, size.height / now.height);
        self.transform = Affine::new([a * kx, b * kx, c * ky, d * ky, e, f]);
    }

    /// The same tiles started from the top-left corner of `frame` (another object's
    /// local bounds).
    pub fn placed_on(&self, frame: Rect) -> Self {
        Self { kind: self.kind.clone(), transform: self.transform.with_translation(frame.origin().to_vec2()) }
    }

    /// One colour standing for the pattern.
    pub fn color(&self) -> Rgba {
        match &self.kind {
            PatternKind::TwoColor { fore, .. } => *fore,
            PatternKind::Bitmap { average, .. } => *average,
        }
    }

    /// The colour shown at `p` (local space). A bitmap pattern gives its average.
    pub fn color_at(&self, p: Point) -> Rgba {
        match &self.kind {
            PatternKind::TwoColor { cells, fore, back } => {
                if self.transform.determinant().abs() < 1e-18 {
                    return *back;
                }
                let q = self.transform.inverse() * p;
                let cell = |v: f64| ((v - v.floor()) * 8.0).floor().clamp(0.0, 7.0) as usize;
                let (col, row) = (cell(q.x), cell(q.y));
                if cells[row] & (0x80 >> col) != 0 { *fore } else { *back }
            }
            PatternKind::Bitmap { average, .. } => *average,
        }
    }
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
            Fill::Gradient(_) | Fill::Pattern(_) => None,
        }
    }

    /// One colour standing for the fill: the uniform colour, the first stop, or the
    /// pattern's foreground (a bitmap's average).
    pub fn color(&self) -> Rgba {
        match self {
            Fill::Solid(c) => *c,
            Fill::Gradient(g) => g.stops.first().map_or(Rgba::BLACK, |s| s.color),
            Fill::Pattern(p) => p.color(),
        }
    }

    /// The colour shown at `p` (local space).
    pub fn color_at(&self, p: Point) -> Rgba {
        match self {
            Fill::Solid(c) => *c,
            Fill::Gradient(g) => g.color_at(p),
            Fill::Pattern(pattern) => pattern.color_at(p),
        }
    }

    /// The fill for another object whose local bounds are `frame`: a fountain is laid
    /// across it and a pattern starts from its top-left corner; colours and tiles stay.
    pub fn placed_on(&self, frame: Rect) -> Fill {
        match self {
            Fill::Solid(c) => Fill::Solid(*c),
            Fill::Gradient(g) => {
                let fresh = Gradient::across(g.kind, frame, Rgba::BLACK, Rgba::WHITE);
                Fill::Gradient(Gradient { start: fresh.start, end: fresh.end, ..g.clone() })
            }
            Fill::Pattern(p) => Fill::Pattern(p.placed_on(frame)),
        }
    }

    /// Move the fill's placement by `t`: for an object whose local space changes by `t`
    /// (a transform baked into its curves).
    pub fn transform(&mut self, t: Affine) {
        match self {
            Fill::Solid(_) => {}
            Fill::Gradient(g) => {
                g.start = t * g.start;
                g.end = t * g.end;
            }
            Fill::Pattern(p) => p.transform = t * p.transform,
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
    Pattern(Pattern),
}

impl From<FillFile> for Fill {
    fn from(f: FillFile) -> Self {
        match f {
            FillFile::Color(c) | FillFile::Tagged(FillTagged::Solid(c)) => Fill::Solid(c),
            FillFile::Tagged(FillTagged::Gradient(g)) => Fill::Gradient(g),
            FillFile::Tagged(FillTagged::Pattern(p)) => Fill::Pattern(p),
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
        let p = Fill::Pattern(Pattern::new(PatternKind::TwoColor { cells: TWO_COLOR_PATTERNS[0].1, fore: RED, back: BLUE }, Point::new(5.0, 5.0), Size::new(16.0, 16.0)));
        let back: Fill = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn two_color_patterns_repeat_their_cells_and_follow_their_placement() {
        // Checkerboard: the top-left quarter of each tile is the foreground.
        let mut p = Pattern::new(PatternKind::TwoColor { cells: TWO_COLOR_PATTERNS[0].1, fore: RED, back: BLUE }, Point::new(10.0, 0.0), Size::new(16.0, 16.0));
        assert_eq!(p.color_at(Point::new(11.0, 1.0)), RED);
        assert_eq!(p.color_at(Point::new(19.0, 1.0)), BLUE, "right half of the top");
        assert_eq!(p.color_at(Point::new(27.0, 17.0)), RED, "the next tile down and across");
        assert_eq!(p.color_at(Point::new(9.0, 1.0)), BLUE, "the tile before the origin");
        assert_eq!(p.color(), RED);

        p.set_tile_size(Size::new(32.0, 8.0));
        assert_eq!(p.tile_size(), Size::new(32.0, 8.0));
        assert_eq!(p.transform.translation(), kurbo::Vec2::new(10.0, 0.0), "starts where it did");
        assert_eq!(p.color_at(Point::new(35.0, 1.0)), BLUE);

        let mut f = Fill::Pattern(p.clone());
        f.transform(Affine::translate((100.0, 0.0)));
        assert_eq!(f.color_at(Point::new(111.0, 1.0)), RED, "baked moves take the tiles along");
        let placed = p.placed_on(Rect::new(-4.0, -4.0, 40.0, 40.0));
        assert_eq!((placed.transform.translation(), placed.tile_size()), (kurbo::Vec2::new(-4.0, -4.0), p.tile_size()));
    }
}

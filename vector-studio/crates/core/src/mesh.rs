//! Mesh fill: a grid of nodes over the object, each with a colour, the colours running
//! smoothly between them. Dragging a node bends the grid and the colours with it. Drawn
//! as a bitmap clipped to the object, made again when the mesh changes.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kurbo::{Point, Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::bitmap::{Bitmap, BitmapFormat};
use crate::document::{Rgba, Shape};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshFill {
    /// Cells across and down.
    pub columns: u32,
    pub rows: u32,
    /// The grid's nodes, row by row from the top left: (rows + 1) × (columns + 1), in the
    /// object's own space.
    pub points: Vec<Point>,
    pub colors: Vec<Rgba>,
    #[serde(skip)]
    pub image: MeshImage,
}

/// The mesh as drawn and a fingerprint of what it was made from. Every one is equal, so
/// making it again is not an edit.
#[derive(Clone, Default)]
pub struct MeshImage {
    key: u64,
    /// A [`Shape::Image`].
    shape: Option<Shape>,
}

impl PartialEq for MeshImage {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl fmt::Debug for MeshImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MeshImage")
    }
}

/// Longest side of the bitmap, in pixels, and most pixels per point.
const MAX_SIDE: f64 = 768.0;
const MAX_SCALE: f64 = 3.0;
/// Each cell is drawn as this many sub-cells across and down.
const SUB: usize = 8;

impl MeshFill {
    /// CorelDRAW's starting grid.
    pub const COLUMNS: u32 = 3;
    pub const ROWS: u32 = 3;
    pub const MAX_CELLS: u32 = 20;

    /// An even grid over `frame`, every node `color`.
    pub fn new(frame: Rect, columns: u32, rows: u32, color: Rgba) -> Self {
        let (columns, rows) = (columns.clamp(1, Self::MAX_CELLS), rows.clamp(1, Self::MAX_CELLS));
        let mut points = Vec::new();
        for r in 0..=rows {
            for c in 0..=columns {
                let (u, v) = (f64::from(c) / f64::from(columns), f64::from(r) / f64::from(rows));
                points.push(Point::new(frame.x0 + u * frame.width(), frame.y0 + v * frame.height()));
            }
        }
        let colors = vec![color; points.len()];
        Self { columns, rows, points, colors, image: MeshImage::default() }
    }

    /// An even grid of a new size over `frame`, each node taking the colour of the old
    /// node nearest it on the grid.
    pub fn regrid(&self, frame: Rect, columns: u32, rows: u32) -> Self {
        let mut out = Self::new(frame, columns, rows, Rgba::WHITE);
        let w = out.columns as usize + 1;
        for (i, color) in out.colors.iter_mut().enumerate() {
            let (u, v) = ((i % w) as f64 / f64::from(out.columns), (i / w) as f64 / f64::from(out.rows));
            let c = (u * f64::from(self.columns)).round() as usize;
            let r = (v * f64::from(self.rows)).round() as usize;
            *color = self.colors.get(r * (self.columns as usize + 1) + c).copied().unwrap_or(Rgba::WHITE);
        }
        out
    }

    /// The node at `(column, row)`.
    pub fn index(&self, column: u32, row: u32) -> usize {
        (row * (self.columns + 1) + column) as usize
    }

    /// The bitmap drawn for the mesh, in the object's own space, once synced: a
    /// [`Shape::Image`].
    pub fn shape(&self) -> Option<&Shape> {
        self.image.shape.as_ref()
    }

    pub(crate) fn sync(&mut self) {
        let mut h = std::hash::DefaultHasher::new();
        (self.columns, self.rows).hash(&mut h);
        for p in &self.points {
            (p.x.to_bits(), p.y.to_bits()).hash(&mut h);
        }
        for c in &self.colors {
            [c.r, c.g, c.b, c.a].hash(&mut h);
        }
        let key = h.finish();
        if self.image.key == key && self.image.shape.is_some() {
            return;
        }
        self.image = MeshImage { key, shape: self.render().map(Shape::Image) };
    }

    fn valid(&self) -> bool {
        let n = (self.columns as usize + 1) * (self.rows as usize + 1);
        self.columns >= 1 && self.rows >= 1 && self.points.len() == n && self.colors.len() == n
    }

    fn render(&self) -> Option<Bitmap> {
        if !self.valid() {
            return None;
        }
        let area = self.points.iter().fold(Rect::from_points(self.points[0], self.points[0]), |r, p| r.union_pt(*p));
        if !(area.width() > 0.0 && area.height() > 0.0 && area.width().is_finite() && area.height().is_finite()) {
            return None;
        }
        let scale = (MAX_SIDE / area.width().max(area.height())).min(MAX_SCALE);
        let (width, height) = ((area.width() * scale).ceil().max(1.0) as usize, (area.height() * scale).ceil().max(1.0) as usize);
        let px = |p: Point| Vec2::new((p.x - area.x0) * scale, (p.y - area.y0) * scale);
        let mut rgba = vec![0u8; width * height * 4];
        let w = self.columns as usize + 1;
        let color = |i: usize| {
            let c = self.colors[i];
            [f64::from(c.r), f64::from(c.g), f64::from(c.b), f64::from(c.a)]
        };
        for r in 0..self.rows as usize {
            for c in 0..self.columns as usize {
                let corners = [r * w + c, r * w + c + 1, (r + 1) * w + c, (r + 1) * w + c + 1];
                let [p00, p10, p01, p11] = corners.map(|i| px(self.points[i]));
                let [c00, c10, c01, c11] = corners.map(color);
                let at = |u: f64, v: f64| {
                    let p = (p00 * (1.0 - u) + p10 * u) * (1.0 - v) + (p01 * (1.0 - u) + p11 * u) * v;
                    let mut col = [0.0; 4];
                    for k in 0..4 {
                        col[k] = (c00[k] * (1.0 - u) + c10[k] * u) * (1.0 - v) + (c01[k] * (1.0 - u) + c11[k] * u) * v;
                    }
                    (p, col)
                };
                for sv in 0..SUB {
                    for su in 0..SUB {
                        let (u0, u1) = (su as f64 / SUB as f64, (su + 1) as f64 / SUB as f64);
                        let (v0, v1) = (sv as f64 / SUB as f64, (sv + 1) as f64 / SUB as f64);
                        let (a, b, cc, d) = (at(u0, v0), at(u1, v0), at(u0, v1), at(u1, v1));
                        triangle(&mut rgba, width, height, [a, b, d]);
                        triangle(&mut rgba, width, height, [a, d, cc]);
                    }
                }
            }
        }
        let data = crate::effects::encode_png(width, height, &rgba)?;
        let rect = Rect::from_origin_size(area.origin(), (width as f64 / scale, height as f64 / scale));
        Some(Bitmap { format: BitmapFormat::Png, width: width as u32, height: height as u32, rect, data: Arc::new(data) })
    }
}

/// Fill the pixels whose centres are in the triangle, mixing its corners' colours.
fn triangle(rgba: &mut [u8], width: usize, height: usize, t: [(Vec2, [f64; 4]); 3]) {
    let [(a, ca), (b, cb), (c, cc)] = t;
    let det = (b - a).cross(c - a);
    if det.abs() < 1e-12 {
        return;
    }
    let lo = |f: fn(f64, f64) -> f64, x: f64, y: f64, z: f64| f(f(x, y), z);
    let x0 = lo(f64::min, a.x, b.x, c.x).floor().max(0.0) as usize;
    let x1 = (lo(f64::max, a.x, b.x, c.x).ceil() as usize).min(width);
    let y0 = lo(f64::min, a.y, b.y, c.y).floor().max(0.0) as usize;
    let y1 = (lo(f64::max, a.y, b.y, c.y).ceil() as usize).min(height);
    // A little slack so neighbouring triangles leave no gaps between them.
    let eps = -1e-3;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = Vec2::new(x as f64 + 0.5, y as f64 + 0.5);
            let wb = (p - a).cross(c - a) / det;
            let wc = (b - a).cross(p - a) / det;
            let wa = 1.0 - wb - wc;
            if wa < eps || wb < eps || wc < eps {
                continue;
            }
            let i = (y * width + x) * 4;
            for k in 0..4 {
                rgba[i + k] = (ca[k] * wa + cb[k] * wb + cc[k] * wc).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

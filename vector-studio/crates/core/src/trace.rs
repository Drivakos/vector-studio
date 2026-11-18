//! Bitmaps > Trace Bitmap: a bitmap turned into curves. Outline Trace fills a curve round
//! each area of one colour; Centerline Trace draws a line along the middle of each stroke.
//! Works on decoded pixels; what comes out is in pixel space (a unit is a pixel, the
//! origin is the image's top-left corner).
//!
//! The steps: the colours are reduced to the number asked for (median cut, then a few
//! rounds of k-means), the background is taken away, areas too small for the detail asked
//! for join their neighbours, and then each colour is traced. Outline Trace follows the
//! pixel edges round each colour and fits Bézier curves to them; Centerline Trace thins
//! each colour down to one pixel and fits curves along what is left.

use std::collections::{HashMap, HashSet, VecDeque};

use kurbo::{Affine, BezPath, CubicBez, ParamCurve, ParamCurveDeriv, Point, Vec2};

use crate::raster::Pixels;
use crate::{Document, LineCap, LineJoin, Object, ObjectId, Outline, Rgba, Shape, Style};

/// How a bitmap is traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraceMethod {
    /// Filled curves round areas of colour.
    Outline,
    /// Lines along the middle of strokes.
    Centerline,
}

impl TraceMethod {
    pub fn label(self) -> &'static str {
        match self {
            TraceMethod::Outline => "Outline Trace",
            TraceMethod::Centerline => "Centerline Trace",
        }
    }

    /// The kinds of image this method has settings for.
    pub fn presets(self) -> &'static [TracePreset] {
        use TracePreset::*;
        match self {
            TraceMethod::Outline => &[LineArt, Logo, DetailedLogo, Clipart, LowQualityImage, HighQualityImage],
            TraceMethod::Centerline => &[TechnicalIllustration, LineDrawing],
        }
    }

    /// The kind of image a trace window starts with. This app's choice.
    pub fn default_preset(self) -> TracePreset {
        match self {
            TraceMethod::Outline => TracePreset::Clipart,
            TraceMethod::Centerline => TracePreset::LineDrawing,
        }
    }
}

/// The kinds of image the trace windows have settings for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TracePreset {
    LineArt,
    Logo,
    DetailedLogo,
    Clipart,
    LowQualityImage,
    HighQualityImage,
    TechnicalIllustration,
    LineDrawing,
}

impl TracePreset {
    pub fn label(self) -> &'static str {
        match self {
            TracePreset::LineArt => "Line art",
            TracePreset::Logo => "Logo",
            TracePreset::DetailedLogo => "Detailed logo",
            TracePreset::Clipart => "Clipart",
            TracePreset::LowQualityImage => "Low quality image",
            TracePreset::HighQualityImage => "High quality image",
            TracePreset::TechnicalIllustration => "Technical illustration",
            TracePreset::LineDrawing => "Line drawing",
        }
    }

    pub fn method(self) -> TraceMethod {
        match self {
            TracePreset::TechnicalIllustration | TracePreset::LineDrawing => TraceMethod::Centerline,
            _ => TraceMethod::Outline,
        }
    }

    /// The settings this kind of image starts with. The values are this app's choice.
    pub fn settings(self) -> TraceSettings {
        let (detail, smoothing, corner_smoothness, colors) = match self {
            TracePreset::LineArt => (40.0, 50.0, 25.0, 2),
            TracePreset::Logo => (50.0, 40.0, 25.0, 4),
            TracePreset::DetailedLogo => (70.0, 30.0, 25.0, 8),
            TracePreset::Clipart => (60.0, 40.0, 40.0, 12),
            TracePreset::LowQualityImage => (30.0, 70.0, 70.0, 12),
            TracePreset::HighQualityImage => (85.0, 25.0, 50.0, 48),
            TracePreset::TechnicalIllustration => (60.0, 25.0, 10.0, 2),
            TracePreset::LineDrawing => (50.0, 60.0, 50.0, 2),
        };
        TraceSettings {
            preset: self,
            detail,
            smoothing,
            corner_smoothness,
            colors,
            remove_background: true,
            remove_entire: false,
            remove_overlap: false,
        }
    }
}

/// The most colours a bitmap can be reduced to.
pub const MAX_COLORS: u32 = 256;

/// How to trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSettings {
    /// The kind of image; it also says which method traces.
    pub preset: TracePreset,
    /// 0 to 100: higher keeps smaller areas.
    pub detail: f64,
    /// 0 to 100: higher smooths the curves more and gives them fewer nodes.
    pub smoothing: f64,
    /// 0 to 100: higher rounds more of the corners.
    pub corner_smoothness: f64,
    /// The colours the bitmap is reduced to before tracing, the background's included.
    pub colors: u32,
    /// Leave out the colour round the edge of the image.
    pub remove_background: bool,
    /// Leave out that colour everywhere, not only where it reaches the edge.
    pub remove_entire: bool,
    /// Outline Trace: each colour's curve only covers that colour. Otherwise curves reach
    /// under the colours traced above them, so no gaps show between colours.
    pub remove_overlap: bool,
}

impl Default for TraceSettings {
    fn default() -> Self {
        TraceMethod::Outline.default_preset().settings()
    }
}

impl TraceSettings {
    pub fn method(&self) -> TraceMethod {
        self.preset.method()
    }
}

/// One traced curve, in pixel space.
#[derive(Clone, Debug, PartialEq)]
pub struct Traced {
    pub color: [u8; 3],
    pub path: BezPath,
    /// Centerline Trace: the outline's width in pixels. Outline Trace curves are filled.
    pub line_width: Option<f64>,
}

/// Trace `pixels`: one curve a colour, the one to go at the bottom of the stack first.
/// Pixels less than half opaque are not traced.
pub fn trace(pixels: &Pixels, s: &TraceSettings) -> Vec<Traced> {
    let mut labels = Labels::quantize(pixels, s.colors.clamp(1, MAX_COLORS) as usize);
    if s.remove_background {
        labels.remove_background(s.remove_entire);
    }
    labels.despeckle(min_area(s.detail, labels.at.len()));
    let fit = Fit::new(s);
    let order = labels.by_area();
    match s.method() {
        TraceMethod::Outline => {
            let mut rank = vec![usize::MAX; labels.palette.len()];
            for (k, &label) in order.iter().enumerate() {
                rank[label as usize] = k;
            }
            order
                .iter()
                .enumerate()
                .filter_map(|(k, &label)| {
                    let mask: Vec<bool> = if s.remove_overlap {
                        labels.at.iter().map(|&l| l == label).collect()
                    } else {
                        labels.at.iter().map(|&l| l != NONE && rank[l as usize] >= k).collect()
                    };
                    let mut path = BezPath::new();
                    for ring in contours(labels.w, labels.h, &mask) {
                        fit.ring(&ring, &mut path);
                    }
                    (!path.elements().is_empty()).then(|| Traced { color: labels.palette[label as usize], path, line_width: None })
                })
                .collect()
        }
        TraceMethod::Centerline => order.iter().filter_map(|&label| centerline(&labels, label, &fit, s.detail)).collect(),
    }
}

/// The style of a traced curve, where a pixel is `pixel` points wide on the page:
/// Centerline Trace lines get round ends and joins.
pub fn traced_style(t: &Traced, pixel: f64) -> Style {
    let color = Rgba::rgb(t.color[0], t.color[1], t.color[2]);
    match t.line_width {
        None => Style::filled(color),
        Some(width) => Style::outlined(Outline { join: LineJoin::Round, cap: LineCap::Round, ..Outline::new(color, width * pixel) }),
    }
}

/// Traced curves as objects in a bitmap's local space: `pixels` is the bitmap decoded
/// (or shrunk), placed at `rect`; a pixel is `page_pixel` points wide on the page per
/// point of local space.
fn traced_objects(doc: &mut Document, traced: &[Traced], to_local: Affine, page_scale: f64) -> Vec<Object> {
    let pixel = (to_local.determinant().abs()).sqrt() * page_scale;
    traced.iter().map(|t| Object::new(doc.alloc_object_id(), Shape::Curve { path: to_local * t.path.clone() }, traced_style(t, pixel))).collect()
}

impl Document {
    /// Trace bitmap `id` on the active page, from its decoded `pixels` (which may be
    /// shrunk). The curves go just above the bitmap, grouped when more than one, where the
    /// bitmap is and with its transform; the bitmap goes if `delete_original`. Returns the
    /// new object, or `None` when the bitmap is locked or nothing was traced.
    pub fn trace_bitmap(&mut self, id: ObjectId, pixels: &Pixels, settings: &TraceSettings, delete_original: bool) -> Option<ObjectId> {
        let page = self.active_page();
        let index = page.objects.iter().position(|o| o.id == id)?;
        let obj = &page.objects[index];
        let Shape::Image(b) = &obj.shape else { return None };
        if obj.locked || pixels.width == 0 || pixels.height == 0 {
            return None;
        }
        let to_local = Affine::translate(b.rect.origin().to_vec2())
            * Affine::scale_non_uniform(b.rect.width() / f64::from(pixels.width), b.rect.height() / f64::from(pixels.height));
        let (transform, layer, perspective) = (obj.transform, obj.layer, obj.perspective);
        let traced = trace(pixels, settings);
        let mut children = traced_objects(self, &traced, to_local, transform.determinant().abs().sqrt());
        children.iter_mut().for_each(|c| c.layer = layer);
        let mut top = match children.len() {
            0 => return None,
            1 => children.remove(0),
            _ => Object::new(self.alloc_object_id(), Shape::Group { children }, Style::default()),
        };
        (top.transform, top.layer, top.perspective) = (transform, layer, perspective);
        let new = top.id;
        let objects = &mut self.active_page_mut().objects;
        objects.insert(index + 1, top);
        if delete_original {
            objects.remove(index);
        }
        Some(new)
    }
}

/// Trace `pixels` and lay the result out as objects in pixel space, for a preview.
pub fn preview_objects(pixels: &Pixels, settings: &TraceSettings) -> Vec<Object> {
    let mut doc = Document::default();
    traced_objects(&mut doc, &trace(pixels, settings), Affine::IDENTITY, 1.0)
}

/// The smallest area, in pixels, kept apart from its neighbours at `detail`: 1 at 100, a
/// 2000th of the image at 0. This app's choice.
fn min_area(detail: f64, pixels: usize) -> usize {
    let coarse = (100.0 - detail.clamp(0.0, 100.0)) / 100.0;
    1 + (coarse * coarse * pixels as f64 * 0.0005) as usize
}

/// No colour: a transparent or removed pixel.
const NONE: u16 = u16::MAX;

/// The image with each pixel given the index of its colour in a palette.
struct Labels {
    w: usize,
    h: usize,
    at: Vec<u16>,
    palette: Vec<[u8; 3]>,
}

/// Pixels of one colour (or, for photos, of nearby colours) and their mean.
struct Bin {
    key: [u8; 3],
    count: f64,
    sum: [f64; 3],
}

impl Bin {
    fn mean(&self) -> [f64; 3] {
        self.sum.map(|v| v / self.count)
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn nearest(palette: &[[f64; 3]], c: [f64; 3]) -> usize {
    (0..palette.len()).min_by(|&a, &b| distance(palette[a], c).total_cmp(&distance(palette[b], c))).unwrap_or(0)
}

fn mean_of(bins: &[&Bin]) -> [f64; 3] {
    let count: f64 = bins.iter().map(|b| b.count).sum();
    let mut sum = [0.0; 3];
    for b in bins {
        (0..3).for_each(|i| sum[i] += b.sum[i]);
    }
    sum.map(|v| v / count)
}

/// Split the colours into `n` groups, each time halving the group whose colours spread
/// furthest, weighted by how many pixels it has. Returns each group's mean.
fn median_cut(bins: &[Bin], n: usize) -> Vec<[f64; 3]> {
    if bins.is_empty() {
        return Vec::new();
    }
    let spread = |b: &[&Bin]| -> (usize, f64) {
        (0..3)
            .map(|ch| {
                let (lo, hi) = b.iter().fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x.mean()[ch]), hi.max(x.mean()[ch])));
                (ch, hi - lo)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 0.0))
    };
    let weight = |b: &[&Bin]| b.iter().map(|x| x.count).sum::<f64>();
    let mut boxes: Vec<Vec<&Bin>> = vec![bins.iter().collect()];
    while boxes.len() < n {
        let best = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| (i, spread(b).1 * weight(b)))
            .filter(|(_, score)| *score > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((i, _)) = best else { break };
        let mut b = boxes.swap_remove(i);
        let ch = spread(&b).0;
        b.sort_by(|x, y| x.mean()[ch].total_cmp(&y.mean()[ch]));
        let half = weight(&b) / 2.0;
        let (mut acc, mut k) = (0.0, 0);
        while k < b.len() && acc + b[k].count <= half {
            acc += b[k].count;
            k += 1;
        }
        let rest = b.split_off(k.clamp(1, b.len() - 1));
        boxes.push(b);
        boxes.push(rest);
    }
    boxes.iter().map(|b| mean_of(b)).collect()
}

/// Move each palette colour to the mean of the colours nearest it, `rounds` times.
fn refine(bins: &[Bin], palette: &mut [[f64; 3]], rounds: usize) {
    for _ in 0..rounds {
        let mut sums = vec![([0.0; 3], 0.0); palette.len()];
        for b in bins {
            let m = b.mean();
            let (sum, count) = &mut sums[nearest(palette, m)];
            (0..3).for_each(|i| sum[i] += m[i] * b.count);
            *count += b.count;
        }
        for (p, (sum, count)) in palette.iter_mut().zip(sums) {
            if count > 0.0 {
                *p = sum.map(|v| v / count);
            }
        }
    }
}

impl Labels {
    /// `pixels` reduced to at most `colors` colours.
    fn quantize(pixels: &Pixels, colors: usize) -> Self {
        let (w, h) = (pixels.width as usize, pixels.height as usize);
        let opaque = |p: &[u8; 4]| p[3] >= 128;
        let mut exact: HashMap<[u8; 3], u32> = HashMap::new();
        let px = pixels.data.as_chunks::<4>().0;
        for p in px.iter().filter(|p| opaque(p)) {
            *exact.entry([p[0], p[1], p[2]]).or_default() += 1;
        }
        // A photo has too many colours to sort through one by one: they are binned 5 bits
        // a channel.
        let coarse = exact.len() > 16_384;
        let key = |c: [u8; 3]| if coarse { c.map(|v| v & 0xF8) } else { c };
        let mut binned: HashMap<[u8; 3], Bin> = HashMap::new();
        for (c, n) in exact {
            let b = binned.entry(key(c)).or_insert(Bin { key: key(c), count: 0.0, sum: [0.0; 3] });
            b.count += f64::from(n);
            (0..3).for_each(|i| b.sum[i] += f64::from(c[i]) * f64::from(n));
        }
        let mut bins: Vec<Bin> = binned.into_values().collect();
        bins.sort_by_key(|b| b.key);
        let mut palette = median_cut(&bins, colors);
        refine(&bins, &mut palette, 6);
        let label_of: HashMap<[u8; 3], u16> = bins.iter().map(|b| (b.key, nearest(&palette, b.mean()) as u16)).collect();
        let at = px.iter().map(|p| if opaque(p) { label_of[&key([p[0], p[1], p[2]])] } else { NONE }).collect();
        let palette = palette.iter().map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect();
        Self { w, h, at, palette }
    }

    /// The pixels next to pixel `i` across an edge.
    fn sides(&self, i: usize) -> impl Iterator<Item = usize> {
        let (w, h) = (self.w, self.h);
        let (x, y) = (i % w, i / w);
        [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)].into_iter().flatten()
    }

    fn border(&self) -> impl Iterator<Item = usize> {
        let (w, h) = (self.w, self.h);
        let rows = (0..w).flat_map(move |x| [x, (h - 1) * w + x]);
        let cols = (0..h).flat_map(move |y| [y * w, y * w + w - 1]);
        rows.chain(cols).filter(move |_| w > 0 && h > 0)
    }

    /// The colour most of the image's edge has.
    fn background(&self) -> Option<u16> {
        let mut counts: HashMap<u16, usize> = HashMap::new();
        for i in self.border() {
            if self.at[i] != NONE {
                *counts.entry(self.at[i]).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|&(l, n)| (n, std::cmp::Reverse(l))).map(|(l, _)| l)
    }

    /// Take away the background colour: where it reaches the edge, or everywhere.
    fn remove_background(&mut self, entire: bool) {
        let Some(bg) = self.background() else { return };
        if entire {
            self.at.iter_mut().filter(|l| **l == bg).for_each(|l| *l = NONE);
            return;
        }
        let mut queue: VecDeque<usize> = VecDeque::new();
        for i in self.border().collect::<Vec<_>>() {
            if self.at[i] == bg {
                self.at[i] = NONE;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            for j in self.sides(i).collect::<Vec<_>>() {
                if self.at[j] == bg {
                    self.at[j] = NONE;
                    queue.push_back(j);
                }
            }
        }
    }

    /// Give each area of fewer than `min_area` pixels the colour most of its edge touches,
    /// smallest areas first.
    fn despeckle(&mut self, min_area: usize) {
        if min_area <= 1 {
            return;
        }
        let n = self.at.len();
        let mut seen = vec![false; n];
        let mut small: Vec<Vec<usize>> = Vec::new();
        for start in 0..n {
            if seen[start] {
                continue;
            }
            let label = self.at[start];
            seen[start] = true;
            let mut members = vec![start];
            let mut k = 0;
            while k < members.len() {
                for j in self.sides(members[k]) {
                    if !seen[j] && self.at[j] == label {
                        seen[j] = true;
                        members.push(j);
                    }
                }
                k += 1;
            }
            if members.len() < min_area {
                small.push(members);
            }
        }
        small.sort_by_key(Vec::len);
        for members in small {
            let own = self.at[members[0]];
            let mut votes: HashMap<u16, usize> = HashMap::new();
            for &i in &members {
                for j in self.sides(i) {
                    if self.at[j] != own {
                        *votes.entry(self.at[j]).or_default() += 1;
                    }
                }
            }
            if let Some((label, _)) = votes.into_iter().max_by_key(|&(l, n)| (n, std::cmp::Reverse(l))) {
                members.iter().for_each(|&i| self.at[i] = label);
            }
        }
    }

    /// The colours in use, the one with the most pixels first.
    fn by_area(&self) -> Vec<u16> {
        let mut counts = vec![0usize; self.palette.len()];
        for &l in &self.at {
            if l != NONE {
                counts[l as usize] += 1;
            }
        }
        let mut order: Vec<u16> = (0..self.palette.len() as u16).filter(|&l| counts[l as usize] > 0).collect();
        order.sort_by_key(|&l| (std::cmp::Reverse(counts[l as usize]), l));
        order
    }
}

/// Steps along pixel edges: right, down, left, up.
const STEP: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The outlines of the pixels in `mask`, along their edges, as the pixel corners each
/// passes. Each goes round with the inside on its right; diagonal neighbours count as
/// apart.
fn contours(w: usize, h: usize, mask: &[bool]) -> Vec<Vec<(i32, i32)>> {
    let cw = w + 1;
    let inside = |x: isize, y: isize| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && mask[y as usize * w + x as usize];
    // The edges leaving each corner, a bit for each step.
    let mut out = vec![0u8; cw * (h + 1)];
    for y in 0..h {
        for x in 0..w {
            if !mask[y * w + x] {
                continue;
            }
            let (xi, yi) = (x as isize, y as isize);
            if !inside(xi, yi - 1) {
                out[y * cw + x] |= 1;
            }
            if !inside(xi + 1, yi) {
                out[y * cw + x + 1] |= 2;
            }
            if !inside(xi, yi + 1) {
                out[(y + 1) * cw + x + 1] |= 4;
            }
            if !inside(xi - 1, yi) {
                out[(y + 1) * cw + x] |= 8;
            }
        }
    }
    let mut rings = Vec::new();
    for start in 0..out.len() {
        while out[start] != 0 {
            let (sx, sy) = ((start % cw) as i32, (start / cw) as i32);
            let mut d = out[start].trailing_zeros() as usize;
            let (mut x, mut y) = (sx, sy);
            let mut ring = vec![(sx, sy)];
            loop {
                out[y as usize * cw + x as usize] &= !(1 << d);
                (x, y) = (x + STEP[d].0, y + STEP[d].1);
                if (x, y) == (sx, sy) {
                    break;
                }
                ring.push((x, y));
                let bits = out[y as usize * cw + x as usize];
                // Turning right first keeps diagonal neighbours apart.
                let Some(next) = [(d + 1) % 4, d, (d + 3) % 4].into_iter().find(|&e| bits & (1 << e) != 0) else { break };
                d = next;
            }
            rings.push(ring);
        }
    }
    rings
}

/// The angle the way turns through at `b`, coming from `a` and going on to `c`.
fn turn(a: Point, b: Point, c: Point) -> f64 {
    let (u, v) = (b - a, c - b);
    if u.hypot2() == 0.0 || v.hypot2() == 0.0 {
        return 0.0;
    }
    u.cross(v).atan2(u.dot(v)).abs()
}

/// Which of `turns` are corners: sharper than `limit` and the sharpest within `k` either
/// side.
fn peaks(turns: &[f64], limit: f64, k: usize, closed: bool) -> Vec<bool> {
    let n = turns.len();
    (0..n)
        .map(|j| {
            turns[j] > limit
                && (1..=k).all(|d| {
                    let ahead = if closed { Some((j + d) % n) } else { (j + d < n).then_some(j + d) };
                    let behind = if closed { Some((j + n - d % n) % n) } else { j.checked_sub(d) };
                    ahead.is_none_or(|a| turns[j] >= turns[a]) && behind.is_none_or(|b| turns[j] > turns[b])
                })
        })
        .collect()
}

/// Taubin smoothing: each point eased towards its neighbours and back a little, so curves
/// smooth without shrinking. `fixed` points stay put.
fn smooth(points: &mut [Point], fixed: &[bool], closed: bool, rounds: usize) {
    let n = points.len();
    if n < 3 {
        return;
    }
    for _ in 0..rounds {
        for factor in [0.5, -0.53] {
            let prev = points.to_vec();
            for i in 0..n {
                if fixed[i] || (!closed && (i == 0 || i == n - 1)) {
                    continue;
                }
                let mid = prev[(i + n - 1) % n].midpoint(prev[(i + 1) % n]);
                points[i] = prev[i] + (mid - prev[i]) * factor;
            }
        }
    }
}

/// How traced outlines become curves.
struct Fit {
    /// How far curves may stray from the smoothed outline, in pixels.
    accuracy: f64,
    /// Rounds of smoothing.
    rounds: usize,
    /// Turns sharper than this, in radians, are corners.
    corner: f64,
}

impl Fit {
    /// The values are this app's choice.
    fn new(s: &TraceSettings) -> Self {
        let smoothing = s.smoothing.clamp(0.0, 100.0) / 100.0;
        let corner = 40.0 + s.corner_smoothness.clamp(0.0, 100.0) * 1.1;
        Self { accuracy: 0.3 + smoothing * 1.2, rounds: 1 + (smoothing * 8.0).round() as usize, corner: corner.to_radians() }
    }

    /// Add a closed outline through pixel corners to `path`.
    fn ring(&self, corners: &[(i32, i32)], path: &mut BezPath) {
        let n = corners.len();
        let c = |i: usize| Point::new(f64::from(corners[i % n].0), f64::from(corners[i % n].1));
        if n < 12 {
            // Too small to smooth: the pixels' own outline.
            let bends: Vec<Point> = (0..n).filter(|&j| turn(c(j + n - 1), c(j), c(j + 1)) > 0.0).map(c).collect();
            if let Some((first, rest)) = bends.split_first() {
                path.move_to(*first);
                rest.iter().for_each(|p| path.line_to(*p));
                path.close_path();
            }
            return;
        }
        // Corners are judged over K steps either side, so the stairs of a slanted edge
        // don't count.
        const K: usize = 4;
        let turns: Vec<f64> = (0..n).map(|j| turn(c(j + n - K), c(j), c(j + K))).collect();
        let is_corner = peaks(&turns, self.corner, K, true);
        // The middle of each pixel edge, and the corners themselves.
        let mut points = Vec::with_capacity(n * 2);
        let mut fixed = Vec::with_capacity(n * 2);
        for (j, &corner) in is_corner.iter().enumerate() {
            if corner {
                points.push(c(j));
                fixed.push(true);
            }
            points.push(c(j).midpoint(c(j + 1)));
            fixed.push(false);
        }
        self.fit(points, fixed, true, path);
    }

    /// Add the curve through `points` to `path`, smoothed and fitted; `corners` stay sharp.
    fn fit(&self, mut points: Vec<Point>, corners: Vec<bool>, closed: bool, path: &mut BezPath) {
        let n = points.len();
        if n == 0 {
            return;
        }
        smooth(&mut points, &corners, closed, self.rounds);
        let mut breaks: Vec<usize> = (0..n).filter(|&i| corners[i]).collect();
        if closed {
            // A loop without corners is fitted in two halves that meet smoothly.
            let seamless = breaks.is_empty();
            if seamless {
                breaks = vec![0, n / 2];
            }
            let through = |i: usize| unit(points[(i + 1) % n] - points[(i + n - 1) % n]);
            path.move_to(points[breaks[0]]);
            for (k, &b) in breaks.iter().enumerate() {
                let end = breaks.get(k + 1).copied().unwrap_or(breaks[0] + n);
                let run: Vec<Point> = (b..=end).map(|i| points[i % n]).collect();
                let (start, finish) = if seamless { (through(b), through(end % n).map(|t| -t)) } else { (None, None) };
                self.run(&run, start, finish, path);
            }
            path.close_path();
        } else {
            breaks.retain(|&i| i != 0 && i != n - 1);
            breaks.insert(0, 0);
            breaks.push(n - 1);
            path.move_to(points[0]);
            for pair in breaks.windows(2) {
                self.run(&points[pair[0]..=pair[1]], None, None, path);
            }
        }
    }

    /// Add Bézier curves through `run` (from the path's current point) to `path`, leaving
    /// along `start` and arriving back along `finish` when given, else along the run.
    fn run(&self, run: &[Point], start: Option<Vec2>, finish: Option<Vec2>, path: &mut BezPath) {
        let n = run.len();
        let reach = 3.min(n.saturating_sub(1));
        let t0 = start.or_else(|| unit(run[reach] - run[0]));
        let t1 = finish.or_else(|| unit(run[n - 1 - reach] - run[n - 1]));
        let (Some(t0), Some(t1)) = (t0, t1) else {
            run.iter().skip(1).for_each(|p| path.line_to(*p));
            return;
        };
        let mut cubics = Vec::new();
        fit_cubics(run, t0, t1, self.accuracy, 0, &mut cubics);
        for c in cubics {
            path.curve_to(c.p1, c.p2, c.p3);
        }
    }
}

/// `v` scaled to length 1, unless it has no length.
fn unit(v: Vec2) -> Option<Vec2> {
    let len = v.hypot();
    (len > 1e-9).then(|| v / len)
}

/// Cubic Béziers through `points`, within `accuracy` of each (Schneider's algorithm): one
/// cubic leaving along `t0` and arriving back along `t1` where one fits, else the points
/// are split where they stray furthest and each part fitted, meeting smoothly.
fn fit_cubics(points: &[Point], t0: Vec2, t1: Vec2, accuracy: f64, depth: u32, out: &mut Vec<CubicBez>) {
    let n = points.len();
    let (first, last) = (points[0], points[n - 1]);
    if n == 2 {
        let third = first.distance(last) / 3.0;
        out.push(CubicBez::new(first, first + t0 * third, last + t1 * third, last));
        return;
    }
    let tolerance = accuracy * accuracy;
    let mut u = chord_lengths(points);
    let mut bez = handles(points, &u, t0, t1);
    let (mut error, mut split) = max_error(points, &bez, &u);
    if error > tolerance && error <= tolerance * 4.0 {
        for _ in 0..4 {
            u = u.iter().zip(points).map(|(&t, &p)| newton(&bez, p, t)).collect();
            bez = handles(points, &u, t0, t1);
            (error, split) = max_error(points, &bez, &u);
            if error <= tolerance {
                break;
            }
        }
    }
    if error <= tolerance || depth > 24 {
        out.push(bez);
        return;
    }
    let split = split.clamp(1, n - 2);
    let centre = unit(points[split - 1] - points[split + 1]).or_else(|| unit(points[split - 1] - points[split])).unwrap_or(-t0);
    fit_cubics(&points[..=split], t0, centre, accuracy, depth + 1, out);
    fit_cubics(&points[split..], -centre, t1, accuracy, depth + 1, out);
}

/// How far along `points` each is, from 0 to 1, by the distance between them.
fn chord_lengths(points: &[Point]) -> Vec<f64> {
    let mut u = vec![0.0];
    for w in points.windows(2) {
        u.push(u.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *u.last().unwrap();
    let n = u.len();
    u.iter().enumerate().map(|(i, d)| if total > 0.0 { d / total } else { i as f64 / (n - 1) as f64 }).collect()
}

/// The cubic from the first point to the last, along `t0` and back along `t1`, whose
/// handle lengths fit `points` (at parameters `u`) best by least squares. Handles that
/// come out backwards or overlong are a third of the way along instead.
fn handles(points: &[Point], u: &[f64], t0: Vec2, t1: Vec2) -> CubicBez {
    let (first, last) = (points[0], points[points.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, &t) in points.iter().zip(u) {
        let s = 1.0 - t;
        let (b0, b1, b2, b3) = (s * s * s, 3.0 * t * s * s, 3.0 * t * t * s, t * t * t);
        let (a1, a2) = (t0 * b1, t1 * b2);
        c00 += a1.dot(a1);
        c01 += a1.dot(a2);
        c11 += a2.dot(a2);
        let rest = p.to_vec2() - (first.to_vec2() * (b0 + b1) + last.to_vec2() * (b2 + b3));
        x0 += a1.dot(rest);
        x1 += a2.dot(rest);
    }
    let det = c00 * c11 - c01 * c01;
    let chord = first.distance(last);
    let (mut left, mut right) = if det.abs() > 1e-12 { ((x0 * c11 - c01 * x1) / det, (c00 * x1 - c01 * x0) / det) } else { (0.0, 0.0) };
    let fine = |a: f64| a > 1e-6 * chord && a < 2.0 * chord;
    if !fine(left) || !fine(right) {
        (left, right) = (chord / 3.0, chord / 3.0);
    }
    CubicBez::new(first, first + t0 * left, last + t1 * right, last)
}

/// The squared distance of the point furthest from `bez`, and which point it is.
fn max_error(points: &[Point], bez: &CubicBez, u: &[f64]) -> (f64, usize) {
    let mut worst = (0.0, points.len() / 2);
    for i in 1..points.len() - 1 {
        let d = (bez.eval(u[i]) - points[i]).hypot2();
        if d > worst.0 {
            worst = (d, i);
        }
    }
    worst
}

/// A better parameter for point `p` on `bez` than `t`: one Newton step towards the
/// nearest point.
fn newton(bez: &CubicBez, p: Point, t: f64) -> f64 {
    let d1 = bez.deriv();
    let d2 = d1.deriv();
    let off = bez.eval(t) - p;
    let (q1, q2) = (d1.eval(t).to_vec2(), d2.eval(t).to_vec2());
    let den = q1.dot(q1) + off.dot(q2);
    if den.abs() < 1e-12 { t } else { (t - off.dot(q1) / den).clamp(0.0, 1.0) }
}

/// Centerline Trace of the pixels of colour `label`: lines along the middle of them, as
/// wide as they are on average.
fn centerline(labels: &Labels, label: u16, fit: &Fit, detail: f64) -> Option<Traced> {
    let (w, h) = (labels.w, labels.h);
    let mask: Vec<bool> = labels.at.iter().map(|&l| l == label).collect();
    let area = mask.iter().filter(|m| **m).count() as f64;
    let skeleton = thin(w, h, mask);
    let lines = skeleton_lines(w, h, &skeleton);
    let centre = |i: usize| Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
    let length = |line: &SkeletonLine| line.pixels.windows(2).map(|p| centre(p[0]).distance(centre(p[1]))).sum::<f64>();
    let total: f64 = lines.iter().map(length).sum();
    if total == 0.0 {
        return None;
    }
    let width = (area / total).max(1.0);
    // Short branches off a line, left by its ends and bulges, go. This app's choice.
    let spur = 1.5 * width + (100.0 - detail.clamp(0.0, 100.0)) / 20.0;
    let mut path = BezPath::new();
    for line in &lines {
        if line.spur && length(line) < spur {
            continue;
        }
        let points: Vec<Point> = line.pixels.iter().map(|&i| centre(i)).collect();
        let n = points.len();
        const K: usize = 3;
        let turns: Vec<f64> = (0..n)
            .map(|j| match line.closed {
                true => turn(points[(j + n - K % n) % n], points[j], points[(j + K) % n]),
                false if j >= K && j + K < n => turn(points[j - K], points[j], points[j + K]),
                false => 0.0,
            })
            .collect();
        let corners = peaks(&turns, fit.corner, K, line.closed);
        fit.fit(points, corners, line.closed, &mut path);
    }
    (!path.elements().is_empty()).then(|| Traced { color: labels.palette[label as usize], path, line_width: Some(width) })
}

/// Zhang–Suen thinning: `img` worn down from both sides to lines one pixel wide, then
/// every pixel its neighbours stay joined without taken away.
fn thin(w: usize, h: usize, mut img: Vec<bool>) -> Vec<bool> {
    let ring = |img: &[bool], i: usize| -> [bool; 8] {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        // North, then clockwise.
        [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)].map(|(dx, dy): (isize, isize)| {
            let (nx, ny) = (x + dx, y + dy);
            nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && img[ny as usize * w + nx as usize]
        })
    };
    loop {
        let mut changed = false;
        for step in 0..2 {
            let gone: Vec<usize> = (0..img.len())
                .filter(|&i| img[i])
                .filter(|&i| {
                    let p = ring(&img, i);
                    let count = p.iter().filter(|b| **b).count();
                    let rises = (0..8).filter(|&k| !p[k] && p[(k + 1) % 8]).count();
                    let (n, e, s, west) = (p[0], p[2], p[4], p[6]);
                    let keep = if step == 0 { (n && e && s) || (e && s && west) } else { (n && e && west) || (n && s && west) };
                    (2..=6).contains(&count) && rises == 1 && !keep
                })
                .collect();
            changed |= !gone.is_empty();
            gone.into_iter().for_each(|i| img[i] = false);
        }
        if !changed {
            break;
        }
    }
    for i in 0..img.len() {
        if img[i] && redundant(ring(&img, i)) {
            img[i] = false;
        }
    }
    img
}

/// Whether a skeleton pixel with neighbours `p` (north, then clockwise) can go: it has
/// two or more, they touch each other without it, and it is not inside the shape.
fn redundant(p: [bool; 8]) -> bool {
    let count = p.iter().filter(|b| **b).count();
    if count < 2 || (p[0] && p[2] && p[4] && p[6]) {
        return false;
    }
    // Neighbours touch when next to each other round the ring, or when both are straight
    // neighbours with a diagonal one between them.
    let mut group: [usize; 8] = std::array::from_fn(|k| k);
    let find = |group: &[usize; 8], mut k: usize| {
        while group[k] != k {
            k = group[k];
        }
        k
    };
    for k in 0..8 {
        let next = [(k + 1) % 8, (k + 2) % 8];
        for (m, j) in next.into_iter().enumerate() {
            if p[k] && p[j] && (m == 0 || k % 2 == 0) {
                let (a, b) = (find(&group, k), find(&group, j));
                group[a] = b;
            }
        }
    }
    let roots: HashSet<usize> = (0..8).filter(|&k| p[k]).map(|k| find(&group, k)).collect();
    roots.len() == 1
}

/// A line of the skeleton: the pixels along it.
struct SkeletonLine {
    pixels: Vec<usize>,
    closed: bool,
    /// One end is free and the other joins other lines.
    spur: bool,
}

/// The skeleton's lines, each from an end or a junction to the next; loops without
/// either come whole.
fn skeleton_lines(w: usize, h: usize, sk: &[bool]) -> Vec<SkeletonLine> {
    let neighbours = |i: usize| {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)].into_iter().filter_map(move |(dx, dy): (isize, isize)| {
            let (nx, ny) = (x + dx, y + dy);
            (nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && sk[ny as usize * w + nx as usize]).then_some(ny as usize * w + nx as usize)
        })
    };
    let degree: Vec<usize> = (0..sk.len()).map(|i| if sk[i] { neighbours(i).count() } else { 0 }).collect();
    let edge = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut used: HashSet<(usize, usize)> = HashSet::new();
    let mut on_line = vec![false; sk.len()];
    let mut lines = Vec::new();
    for p in (0..sk.len()).filter(|&i| sk[i] && degree[i] != 2) {
        for q in neighbours(p).collect::<Vec<_>>() {
            if !used.insert(edge(p, q)) {
                continue;
            }
            let mut pixels = vec![p, q];
            let (mut prev, mut cur) = (p, q);
            while degree[cur] == 2 {
                let Some(next) = neighbours(cur).find(|&r| r != prev) else { break };
                if !used.insert(edge(cur, next)) {
                    break;
                }
                pixels.push(next);
                (prev, cur) = (cur, next);
            }
            pixels.iter().for_each(|&i| on_line[i] = true);
            let free = |i: usize| degree[i] == 1;
            let spur = free(p) != free(cur);
            lines.push(SkeletonLine { pixels, closed: false, spur });
        }
    }
    for p in 0..sk.len() {
        if !sk[p] || on_line[p] || degree[p] != 2 {
            continue;
        }
        on_line[p] = true;
        let mut pixels = vec![p];
        let Some(mut cur) = neighbours(p).next() else { continue };
        let mut prev = p;
        while cur != p && !on_line[cur] && pixels.len() <= sk.len() {
            on_line[cur] = true;
            pixels.push(cur);
            let Some(next) = neighbours(cur).find(|&r| r != prev) else { break };
            (prev, cur) = (cur, next);
        }
        lines.push(SkeletonLine { closed: pixels.len() > 2, pixels, spur: false });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{PathEl, Shape as _};

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn image(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Pixels {
        let mut p = Pixels::filled(w, h, WHITE);
        for y in 0..h {
            for x in 0..w {
                p.set_pixel(x, y, paint(x, y));
            }
        }
        p
    }

    fn settings(preset: TracePreset, colors: u32) -> TraceSettings {
        TraceSettings { colors, ..preset.settings() }
    }

    fn square(x0: u32, y0: u32, x1: u32, y1: u32, inside: [u8; 4]) -> impl Fn(u32, u32) -> [u8; 4] {
        move |x, y| if (x0..x1).contains(&x) && (y0..y1).contains(&y) { inside } else { WHITE }
    }

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn a_square_traces_as_one_square_curve_without_the_background() {
        let out = trace(&image(40, 40, square(10, 10, 30, 30, BLACK)), &settings(TracePreset::Logo, 2));
        assert_eq!(out.len(), 1, "the white background is removed");
        assert_eq!(out[0].color, [0, 0, 0]);
        assert_eq!(out[0].line_width, None, "filled");
        let b = out[0].path.bounding_box();
        assert!([b.x0, b.y0, b.x1, b.y1].iter().zip([10.0, 10.0, 30.0, 30.0]).all(|(a, e)| near(*a, e, 0.6)), "{b:?}");
        assert!(near(out[0].path.area().abs(), 400.0, 8.0), "area {}", out[0].path.area());
        // Kept sharp: the corner points themselves are on the curve.
        let ends: Vec<Point> = out[0].path.segments().map(|s| s.end()).collect();
        for c in [Point::new(10.0, 10.0), Point::new(30.0, 30.0)] {
            assert!(ends.iter().any(|p| p.distance(c) < 0.01), "corner {c:?} kept: {ends:?}");
        }
    }

    #[test]
    fn rounded_corners_stay_inside_the_shape() {
        let img = image(80, 80, square(44, 20, 72, 60, BLACK));
        for corner_smoothness in [0.0, 40.0, 70.0, 100.0] {
            for smoothing in [0.0, 50.0, 100.0] {
                let out = trace(&img, &TraceSettings { corner_smoothness, smoothing, ..settings(TracePreset::Logo, 2) });
                let b = out[0].path.bounding_box();
                // Rounded corners may stray by the fit's accuracy, at most 1.5 pixels.
                let inside = kurbo::Rect::new(44.0, 20.0, 72.0, 60.0).inflate(0.3 + smoothing / 100.0 * 1.2, 0.3 + smoothing / 100.0 * 1.2);
                assert!(inside.contains_rect(b) && b.width() > 26.0 && b.height() > 38.0, "corners {corner_smoothness}, smoothing {smoothing}: {b:?}");
            }
        }
        let sharp = trace(&img, &TraceSettings { corner_smoothness: 0.0, ..settings(TracePreset::Logo, 2) });
        let round = trace(&img, &TraceSettings { corner_smoothness: 100.0, ..settings(TracePreset::Logo, 2) });
        let gap = |t: &[Traced]| {
            let corner = Point::new(44.0, 20.0);
            t[0].path.segments().map(|seg| kurbo::ParamCurveNearest::nearest(&seg, corner, 1e-4).distance_sq.sqrt()).fold(f64::MAX, f64::min)
        };
        assert!(gap(&sharp) < 0.01, "a sharp corner goes through the pixel corner");
        assert!(gap(&round) > 0.2, "a rounded one cuts across it: {}", gap(&round));
    }

    #[test]
    fn keeping_the_background_traces_it_below() {
        let s = TraceSettings { remove_background: false, ..settings(TracePreset::Logo, 2) };
        let out = trace(&image(40, 40, square(10, 10, 30, 30, BLACK)), &s);
        assert_eq!(out.iter().map(|t| t.color).collect::<Vec<_>>(), vec![[255, 255, 255], [0, 0, 0]], "the bigger area at the bottom");
        assert!(near(out[0].path.area().abs(), 1600.0, 16.0), "stacked: the background reaches under the square");
    }

    #[test]
    fn a_ring_keeps_its_hole_and_a_disc_stays_round() {
        let (cx, cy) = (32.0, 32.0);
        let ring = |x: u32, y: u32| {
            let r = (f64::from(x) + 0.5 - cx).hypot(f64::from(y) + 0.5 - cy);
            if (8.0..20.0).contains(&r) { BLACK } else { WHITE }
        };
        let out = trace(&image(64, 64, ring), &settings(TracePreset::Clipart, 2));
        assert_eq!(out.iter().map(|t| t.color).collect::<Vec<_>>(), vec![[0, 0, 0], [255, 255, 255]], "white inside the ring doesn't reach the edge, so it stays");
        let out = trace(&image(64, 64, ring), &TraceSettings { remove_entire: true, ..settings(TracePreset::Clipart, 2) });
        assert_eq!(out.len(), 1, "unless removed from the entire image");
        let expected = std::f64::consts::PI * (400.0 - 64.0);
        assert!(near(out[0].path.area().abs(), expected, expected * 0.04), "area {} ≈ {expected}", out[0].path.area());
        // Every point of the outer curve lies close to the circle.
        let mut far = 0.0f64;
        kurbo::flatten(&out[0].path, 0.1, |el| {
            if let PathEl::LineTo(p) | PathEl::MoveTo(p) = el {
                let r = (p.x - cx).hypot(p.y - cy);
                far = far.max((r - 20.0).abs().min((r - 8.0).abs()));
            }
        });
        assert!(far < 0.8, "round, not stepped: {far}");
    }

    #[test]
    fn overlap_is_removed_only_when_asked() {
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        let paint = |x: u32, y: u32| match (x, y) {
            (20..30, 20..30) => red,
            (10..40, 10..40) => blue,
            _ => WHITE,
        };
        let stacked = trace(&image(50, 50, paint), &settings(TracePreset::Logo, 3));
        assert_eq!(stacked.iter().map(|t| t.color).collect::<Vec<_>>(), vec![[0, 0, 255], [255, 0, 0]]);
        assert!(near(stacked[0].path.area().abs(), 900.0, 10.0), "blue reaches under red");
        let apart = trace(&image(50, 50, paint), &TraceSettings { remove_overlap: true, ..settings(TracePreset::Logo, 3) });
        assert!(near(apart[0].path.area().abs(), 800.0, 10.0), "blue has a hole for red: {}", apart[0].path.area());
        assert!(near(apart[1].path.area().abs(), 100.0, 4.0));
    }

    #[test]
    fn transparent_pixels_are_not_traced() {
        let paint = square(5, 5, 15, 15, BLACK);
        let out = trace(&image(20, 20, |x, y| if paint(x, y) == BLACK { BLACK } else { [0, 0, 0, 0] }), &settings(TracePreset::Logo, 2));
        assert_eq!(out.len(), 1);
        assert!(near(out[0].path.area().abs(), 100.0, 4.0));
        assert!(trace(&Pixels::filled(8, 8, [0, 0, 0, 0]), &TraceSettings::default()).is_empty(), "nothing to trace");
    }

    #[test]
    fn detail_decides_whether_specks_are_kept() {
        let specks = |x: u32, y: u32| if (x == 20 && y == 20) || (30..60).contains(&x) && (30..60).contains(&y) { BLACK } else { WHITE };
        let img = image(100, 100, specks);
        let area = |detail| {
            let out = trace(&img, &TraceSettings { detail, ..settings(TracePreset::Logo, 2) });
            out[0].path.area().abs()
        };
        assert!(near(area(0.0), 900.0, 10.0), "the speck joins the background: {}", area(0.0));
        assert!(area(100.0) > 900.0, "full detail keeps the speck");
    }

    #[test]
    fn colours_are_reduced_to_the_number_asked_for() {
        let paint = |x: u32, _| match x / 10 {
            0 => [200, 30, 30, 255],
            1 => [30, 200, 30, 255],
            2 => [30, 30, 200, 255],
            _ => [205, 35, 35, 255],
        };
        let img = image(40, 10, paint);
        let all = |colors| {
            let mut c: Vec<[u8; 3]> = trace(&img, &TraceSettings { colors, remove_background: false, ..TraceSettings::default() }).iter().map(|t| t.color).collect();
            c.sort();
            c
        };
        assert_eq!(all(3).len(), 3, "the two near reds become one");
        assert!(all(3).contains(&[203, 33, 33]), "their mean: {:?}", all(3));
        assert_eq!(all(1).len(), 1);
    }

    #[test]
    fn centerline_runs_down_the_middle_of_a_stroke() {
        // A stroke 4 pixels thick, from x 10 to 50 along y 28..32.
        let img = image(60, 60, square(10, 28, 50, 32, BLACK));
        let out = trace(&img, &TracePreset::TechnicalIllustration.settings());
        assert_eq!(out.len(), 1);
        let line = &out[0];
        let width = line.line_width.expect("an outline");
        assert!(near(width, 4.0, 1.0), "as wide as the stroke: {width}");
        assert!(!line.path.elements().iter().any(|e| matches!(e, PathEl::ClosePath)), "open");
        let b = line.path.bounding_box();
        assert!(near(b.center().y, 30.0, 0.6) && b.height() < 1.0, "along the middle: {b:?}");
        assert!(b.x0 < 14.0 && b.x1 > 46.0, "nearly end to end: {b:?}");
    }

    #[test]
    fn centerline_keeps_a_corner_and_traces_a_loop_closed() {
        // An L: down then right.
        let l = |x: u32, y: u32| if (10..14).contains(&x) && (10..50).contains(&y) || (10..50).contains(&x) && (46..50).contains(&y) { BLACK } else { WHITE };
        let out = trace(&image(60, 60, l), &TracePreset::TechnicalIllustration.settings());
        let b = out[0].path.bounding_box();
        assert!(b.x0 < 13.0 && b.y0 < 15.0 && b.x1 > 45.0 && b.y1 > 47.0, "both arms: {b:?}");
        let ends: Vec<Point> = out[0].path.segments().map(|s| s.end()).collect();
        assert!(ends.iter().any(|p| p.distance(Point::new(12.0, 48.0)) < 2.0), "a node at the corner: {ends:?}");

        let ring = |x: u32, y: u32| {
            let r = (f64::from(x) + 0.5 - 30.0).hypot(f64::from(y) + 0.5 - 30.0);
            if (14.0..17.0).contains(&r) { BLACK } else { WHITE }
        };
        let out = trace(&image(60, 60, ring), &TracePreset::LineDrawing.settings());
        assert_eq!(out.len(), 1);
        assert!(out[0].path.elements().iter().any(|e| matches!(e, PathEl::ClosePath)), "a closed line");
        let b = out[0].path.bounding_box();
        assert!(near(b.width(), 31.0, 2.0) && near(b.height(), 31.0, 2.0), "along the middle of the ring: {b:?}");
    }

    #[test]
    fn trace_bitmap_puts_the_curves_above_the_bitmap_where_it_is() {
        use crate::{Bitmap, BitmapFormat};
        let pixels = image(40, 40, |x, y| match (x, y) {
            (0..20, 0..20) => [255, 0, 0, 255],
            (20..40, 20..40) => BLACK,
            _ => WHITE,
        });
        let bitmap = Bitmap { format: BitmapFormat::Png, width: 40, height: 40, rect: kurbo::Rect::new(0.0, 0.0, 80.0, 80.0), data: Default::default() };
        let mut doc = Document::default();
        let below = doc.add_object(Shape::Rectangle { rect: kurbo::Rect::new(0.0, 0.0, 5.0, 5.0), corner_radius: 0.0 }, Style::default());
        let image = doc.add_object(Shape::Image(bitmap), Style::default());
        let above = doc.add_object(Shape::Rectangle { rect: kurbo::Rect::new(0.0, 0.0, 5.0, 5.0), corner_radius: 0.0 }, Style::default());
        doc.active_page_mut().object_mut(image).unwrap().transform = Affine::translate((100.0, 0.0));

        let group = doc.trace_bitmap(image, &pixels, &settings(TracePreset::Logo, 3), false).unwrap();
        let ids: Vec<ObjectId> = doc.active_page().objects.iter().map(|o| o.id).collect();
        assert_eq!(ids, vec![below, image, group, above], "just above the bitmap");
        let g = doc.active_page().object(group).unwrap();
        let Shape::Group { children } = &g.shape else { panic!("a group: {:?}", g.shape) };
        assert_eq!(children.len(), 2, "red and black; white was the background");
        let red = children.iter().find(|c| c.style.fill == Some(Rgba::rgb(255, 0, 0).into())).unwrap();
        let b = g.transform * red.shape.to_path();
        let b = kurbo::Shape::bounding_box(&b);
        assert!(near(b.x0, 100.0, 0.6) && near(b.x1, 140.0, 1.2) && near(b.y1, 40.0, 1.2), "on the bitmap's top-left quarter: {b:?}");

        let single = doc.trace_bitmap(image, &pixels, &TraceSettings { colors: 2, remove_background: true, ..settings(TracePreset::Logo, 2) }, true);
        let page = doc.active_page();
        assert!(page.object(image).is_none(), "Delete original image");
        assert!(matches!(page.object(single.unwrap()).unwrap().shape, Shape::Curve { .. } | Shape::Group { .. }));
        assert_eq!(doc.trace_bitmap(above, &pixels, &TraceSettings::default(), false), None, "only bitmaps");
    }

    #[test]
    fn centerline_widths_are_in_points() {
        let line = Traced { color: [0, 0, 0], path: BezPath::new(), line_width: Some(4.0) };
        let style = traced_style(&line, 0.5);
        let outline = style.outline.unwrap();
        assert_eq!((outline.width, outline.join, outline.cap), (2.0, LineJoin::Round, LineCap::Round));
        assert!(style.fill.is_none());
    }

    #[test]
    fn presets_belong_to_their_method() {
        for method in [TraceMethod::Outline, TraceMethod::Centerline] {
            assert!(method.presets().contains(&method.default_preset()));
            for p in method.presets() {
                assert_eq!(p.method(), method);
                assert_eq!(p.settings().preset, *p);
            }
        }
    }
}


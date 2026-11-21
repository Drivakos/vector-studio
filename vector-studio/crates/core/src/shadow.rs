//! Drop shadows: a blurred copy of an object's silhouette, offset, drawn under it. As in
//! CorelDRAW the shadow is drawn as a bitmap. It is made again whenever the object or the
//! shadow's settings change ([`Document::sync_shadows`]), so the shadow stays editable.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape as _, Vec2};
use serde::{Deserialize, Serialize};

use crate::bitmap::{Bitmap, BitmapFormat};
use crate::document::{Document, Object, ObjectId, Rgba, Shape};

/// Longest side of a shadow bitmap, in pixels. This app's choice: shadows are soft, so a
/// coarse bitmap looks the same and is quick to make while the object is dragged.
const MAX_SIDE: f64 = 1024.0;
/// Most shadow pixels per point (144 dpi).
const MAX_SCALE: f64 = 2.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DropShadow {
    /// From the object to its shadow, in points, in the space the object is placed in
    /// (the page's, for an object not in a group).
    pub offset: Vec2,
    pub color: Rgba,
    /// 0 is invisible, 1 as solid as the colour.
    pub opacity: f64,
    /// How far the shadow's edge fades out, in points; 0 is a hard edge.
    pub feather: f64,
    /// The shadow as drawn (not saved).
    #[serde(skip)]
    pub image: ShadowImage,
}

impl DropShadow {
    /// This app's starting settings, close to CorelDRAW's: black, half opaque, soft.
    pub const OPACITY: f64 = 0.5;
    pub const FEATHER: f64 = 6.0;
    pub const MAX_FEATHER: f64 = 100.0;

    pub fn new(offset: Vec2) -> Self {
        Self { offset, color: Rgba::BLACK, opacity: Self::OPACITY, feather: Self::FEATHER, image: ShadowImage::default() }
    }

    /// The bitmap drawn for the shadow, once [`Document::sync_shadows`] has made it: a
    /// [`Shape::Image`] placed in the object's parent space.
    pub fn shape(&self) -> Option<&Shape> {
        self.image.shape.as_ref()
    }
}

/// A shadow's bitmap and a fingerprint of what it was made from. Every shadow image is
/// equal, so making one again is not an edit.
#[derive(Clone, Debug, Default)]
pub struct ShadowImage {
    key: u64,
    shape: Option<Shape>,
}

impl PartialEq for ShadowImage {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Object {
    /// The object without the shadow and the objects in it without theirs, to measure.
    fn silhouette(&self) -> Vec<(BezPath, bool)> {
        let mut out = Vec::new();
        for leaf in self.leaves() {
            // PowerClip contents show only inside their frame: the frame stands for them.
            if let Some(clip) = leaf.clips.last() {
                out.push((clip.clone(), true));
                continue;
            }
            let solid = leaf.fill().is_some() || matches!(leaf.shape, Shape::Image(_) | Shape::Text(_));
            if solid {
                out.push((leaf.path.clone(), true));
            }
            if let Some(o) = &leaf.style.outline {
                let width = o.page_width(leaf.transform).max(0.1);
                let stroked = kurbo::stroke(leaf.path.iter(), &o.stroke(width), &kurbo::StrokeOpts::default(), 0.05);
                out.push((stroked, false));
            }
        }
        out
    }
}

impl Document {
    /// Make every shadow's bitmap whose object or settings changed since it was made.
    pub fn sync_shadows(&mut self) {
        for page in &mut self.pages {
            page.objects.iter_mut().for_each(sync_object);
        }
    }

    /// Selected objects that have a shadow.
    pub fn with_shadow(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let page = self.active_page();
        ids.iter().copied().filter(|id| page.object(*id).is_some_and(|o| o.shadow.is_some())).collect()
    }

    /// Give each unlocked object `shadow`'s settings (and a shadow if it has none).
    pub fn set_shadow(&mut self, ids: &[ObjectId], shadow: &DropShadow) {
        let page = self.active_page_mut();
        for id in ids {
            if let Some(obj) = page.object_mut(*id).filter(|o| !o.locked) {
                let image = obj.shadow.take().map(|s| s.image).unwrap_or_default();
                obj.shadow = Some(DropShadow { image, ..shadow.clone() });
            }
        }
    }

    /// Remove the shadows of the unlocked objects.
    pub fn clear_shadow(&mut self, ids: &[ObjectId]) {
        let page = self.active_page_mut();
        for id in ids {
            if let Some(obj) = page.object_mut(*id).filter(|o| !o.locked) {
                obj.shadow = None;
            }
        }
    }
}

fn sync_object(obj: &mut Object) {
    if let Shape::Group { children } = &mut obj.shape {
        children.iter_mut().for_each(sync_object);
    }
    obj.contents.iter_mut().for_each(sync_object);
    let Some(shadow) = &obj.shadow else { return };
    let parts = obj.silhouette();
    let key = fingerprint(&parts, shadow);
    if shadow.image.key == key && shadow.image.shape.is_some() {
        return;
    }
    let shape = render(&parts, shadow).map(Shape::Image);
    if let Some(shadow) = &mut obj.shadow {
        shadow.image = ShadowImage { key, shape };
    }
}

fn fingerprint(parts: &[(BezPath, bool)], shadow: &DropShadow) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    let mut point = |p: Point| {
        p.x.to_bits().hash(&mut h);
        p.y.to_bits().hash(&mut h);
    };
    for (path, even_odd) in parts {
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(a) | PathEl::LineTo(a) => point(a),
                PathEl::QuadTo(a, b) => [a, b].into_iter().for_each(&mut point),
                PathEl::CurveTo(a, b, c) => [a, b, c].into_iter().for_each(&mut point),
                PathEl::ClosePath => point(Point::new(f64::NAN, 0.0)),
            }
        }
        point(Point::new(f64::INFINITY, f64::from(u8::from(*even_odd))));
    }
    for v in [shadow.offset.x, shadow.offset.y, shadow.opacity, shadow.feather] {
        v.to_bits().hash(&mut h);
    }
    [shadow.color.r, shadow.color.g, shadow.color.b, shadow.color.a].hash(&mut h);
    h.finish()
}

/// The shadow's bitmap for a silhouette made of `parts` (path, filled even-odd or not).
fn render(parts: &[(BezPath, bool)], shadow: &DropShadow) -> Option<Bitmap> {
    let shape = parts.iter().map(|(p, _)| p.bounding_box()).reduce(|a, b| a.union(b))?;
    let feather = shadow.feather.clamp(0.0, DropShadow::MAX_FEATHER);
    // A Gaussian blur fades an edge over about 2.5 sigma.
    let sigma = feather / 2.5;
    let margin = 3.0 * sigma + 1.0;
    let area = shape.inflate(margin, margin);
    if !(area.width() > 0.0 && area.height() > 0.0 && area.width().is_finite() && area.height().is_finite()) {
        return None;
    }
    let scale = (MAX_SIDE / area.width().max(area.height())).min(MAX_SCALE);
    let (width, height) = ((area.width() * scale).ceil() as usize, (area.height() * scale).ceil() as usize);
    let to_pixels = Affine::scale(scale) * Affine::translate(-area.origin().to_vec2());
    let mut alpha = vec![0.0f32; width * height];
    for (path, even_odd) in parts {
        let mut path = path.clone();
        path.apply_affine(to_pixels);
        cover(&path, *even_odd, width, height, &mut alpha);
    }
    blur(&mut alpha, width, height, sigma * scale);

    let c = shadow.color;
    let k = shadow.opacity.clamp(0.0, 1.0) as f32 * f32::from(c.a);
    let rgba: Vec<u8> = alpha.iter().flat_map(|a| [c.r, c.g, c.b, (a.clamp(0.0, 1.0) * k).round() as u8]).collect();
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    }
    let rect = Rect::from_origin_size(area.origin() + shadow.offset, (width as f64 / scale, height as f64 / scale));
    Some(Bitmap { format: BitmapFormat::Png, width: width as u32, height: height as u32, rect, data: Arc::new(data) })
}

/// Sub-rows sampled per pixel row.
const SUB: usize = 4;

/// Add the area of `path` (pixel space) to `alpha`, keeping the larger where parts
/// overlap. Filled even-odd, or non-zero (outlines turned into areas).
fn cover(path: &BezPath, even_odd: bool, width: usize, height: usize, alpha: &mut [f32]) {
    // Edges as (y top, y bottom, x at top, dx/dy, winding).
    let mut edges = Vec::new();
    let mut add = |a: Point, b: Point| {
        if a.y == b.y {
            return;
        }
        let (top, bottom, dir) = if a.y < b.y { (a, b, 1) } else { (b, a, -1) };
        edges.push((top.y, bottom.y, top.x, (bottom.x - top.x) / (bottom.y - top.y), dir));
    };
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    kurbo::flatten(path, 0.1, |el| match el {
        PathEl::MoveTo(p) => {
            add(last, start);
            (start, last) = (p, p);
        }
        PathEl::LineTo(p) => {
            add(last, p);
            last = p;
        }
        PathEl::ClosePath => {
            add(last, start);
            last = start;
        }
        _ => {}
    });
    add(last, start);
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut row = vec![0.0f32; width];
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    let mut next = 0;
    let mut active: Vec<usize> = Vec::new();
    for y in 0..height {
        row.iter_mut().for_each(|v| *v = 0.0);
        for s in 0..SUB {
            let sy = y as f64 + (s as f64 + 0.5) / SUB as f64;
            while next < edges.len() && edges[next].0 <= sy {
                active.push(next);
                next += 1;
            }
            active.retain(|&i| edges[i].1 > sy);
            crossings.clear();
            crossings.extend(active.iter().map(|&i| {
                let (y0, _, x0, slope, dir) = edges[i];
                (x0 + (sy - y0) * slope, dir)
            }));
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                let inside = if even_odd { winding % 2 != 0 } else { winding != 0 };
                if inside {
                    span(&mut row, pair[0].0, pair[1].0, 1.0 / SUB as f32);
                }
            }
        }
        for (dst, v) in alpha[y * width..(y + 1) * width].iter_mut().zip(&row) {
            *dst = dst.max(*v);
        }
    }
}

/// Add `weight` × the part of each pixel between `x0` and `x1` to `row`.
fn span(row: &mut [f32], x0: f64, x1: f64, weight: f32) {
    let (x0, x1) = (x0.max(0.0), x1.min(row.len() as f64));
    if x1 <= x0 {
        return;
    }
    let (first, last) = (x0.floor() as usize, (x1.ceil() as usize).min(row.len()));
    for (i, v) in row.iter_mut().enumerate().take(last).skip(first) {
        let covered = (x1.min(i as f64 + 1.0) - x0.max(i as f64)).max(0.0);
        *v += covered as f32 * weight;
    }
}

/// An approximate Gaussian blur of standard deviation `sigma` pixels: three box blurs
/// along each axis.
fn blur(alpha: &mut [f32], width: usize, height: usize, sigma: f64) {
    if sigma < 0.3 {
        return;
    }
    let size = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let radius = ((size - 1.0) / 2.0).round().max(1.0) as usize;
    let mut line = Vec::new();
    for _ in 0..3 {
        for y in 0..height {
            box_line(alpha, y * width, 1, width, radius, &mut line);
        }
        for x in 0..width {
            box_line(alpha, x, width, height, radius, &mut line);
        }
    }
}

/// Box-blur the `len` values of `data` from `start`, `step` apart, with a box `radius`
/// pixels either side; outside is 0.
fn box_line(data: &mut [f32], start: usize, step: usize, len: usize, radius: usize, line: &mut Vec<f32>) {
    line.clear();
    line.extend((0..len).map(|i| data[start + i * step]));
    let norm = 1.0 / (2 * radius + 1) as f32;
    let mut sum: f32 = line.iter().take(radius + 1).sum();
    for i in 0..len {
        data[start + i * step] = sum * norm;
        if i + radius + 1 < len {
            sum += line[i + radius + 1];
        }
        if i >= radius {
            sum -= line[i - radius];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Outline, Style};

    fn square(fill: bool) -> Object {
        let style = if fill { Style::filled(Rgba::rgb(0, 0, 255)) } else { Style::outlined(Outline::new(Rgba::BLACK, 4.0)) };
        Object::new(ObjectId(1), Shape::Rectangle { rect: Rect::new(0.0, 0.0, 100.0, 50.0), corner_radius: 0.0 }, style)
    }

    fn pixels(b: &Bitmap) -> (u32, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(b.data.as_slice()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        (b.width, buf)
    }

    /// Alpha of the shadow at page point `p`.
    fn alpha_at(obj: &Object, p: Point) -> u8 {
        let Some(Shape::Image(b)) = obj.shadow.as_ref().unwrap().shape() else { panic!("no shadow image") };
        let (w, data) = pixels(b);
        let x = ((p.x - b.rect.x0) / b.rect.width() * f64::from(b.width)) as usize;
        let y = ((p.y - b.rect.y0) / b.rect.height() * f64::from(b.height)) as usize;
        data[(y * w as usize + x) * 4 + 3]
    }

    #[test]
    fn a_hard_shadow_covers_the_shape_moved_by_the_offset() {
        let mut obj = square(true);
        obj.shadow = Some(DropShadow { feather: 0.0, opacity: 1.0, ..DropShadow::new(Vec2::new(10.0, 20.0)) });
        sync_object(&mut obj);
        assert_eq!(alpha_at(&obj, Point::new(105.0, 65.0)), 255, "inside the moved square");
        assert_eq!(alpha_at(&obj, Point::new(5.0, 15.0)), 0, "where the square is but its shadow is not");
        let Some(Shape::Image(b)) = obj.shadow.as_ref().unwrap().shape() else { panic!() };
        assert!((b.rect.x0 - 9.0).abs() < 1e-9 && (b.rect.y0 - 19.0).abs() < 1e-9, "a point of margin: {:?}", b.rect);
    }

    #[test]
    fn a_feathered_shadow_fades_across_the_edge_at_the_opacity() {
        let mut obj = square(true);
        obj.shadow = Some(DropShadow::new(Vec2::ZERO));
        sync_object(&mut obj);
        let middle = alpha_at(&obj, Point::new(50.0, 25.0));
        assert!((125..=130).contains(&middle), "half opaque in the middle: {middle}");
        let edge = alpha_at(&obj, Point::new(50.0, 0.5));
        assert!((50..100).contains(&edge), "about half of that on the edge: {edge}");
        assert_eq!(alpha_at(&obj, Point::new(50.0, -9.0)), 0, "nothing past the fade");
    }

    #[test]
    fn an_unfilled_shape_casts_the_shadow_of_its_outline_only() {
        let mut obj = square(false);
        obj.shadow = Some(DropShadow { feather: 0.0, opacity: 1.0, ..DropShadow::new(Vec2::ZERO) });
        sync_object(&mut obj);
        assert_eq!(alpha_at(&obj, Point::new(0.5, 25.0)), 255, "under the outline");
        assert_eq!(alpha_at(&obj, Point::new(50.0, 25.0)), 0, "not inside");
    }

    #[test]
    fn the_image_is_made_again_only_when_something_changed() {
        let mut obj = square(true);
        obj.shadow = Some(DropShadow::new(Vec2::new(5.0, 5.0)));
        sync_object(&mut obj);
        let data = |o: &Object| match o.shadow.as_ref().unwrap().shape() {
            Some(Shape::Image(b)) => b.data.clone(),
            _ => panic!(),
        };
        let first = data(&obj);
        sync_object(&mut obj);
        assert!(Arc::ptr_eq(&first, &data(&obj)), "unchanged: kept");
        obj.transform = Affine::translate((30.0, 0.0));
        sync_object(&mut obj);
        assert!(!Arc::ptr_eq(&first, &data(&obj)), "moved: made again");
        assert!(alpha_at(&obj, Point::new(85.0, 30.0)) > 100, "under the moved object");
    }

    #[test]
    fn shadows_are_not_part_of_what_is_hit_or_shaped_but_are_drawn() {
        let mut obj = square(true);
        obj.shadow = Some(DropShadow::new(Vec2::new(200.0, 0.0)));
        sync_object(&mut obj);
        assert_eq!(obj.leaves().len(), 1);
        let drawn = obj.drawn_leaves();
        assert_eq!(drawn.len(), 2);
        assert!(drawn[0].shadow && matches!(drawn[0].shape, Shape::Image(_)) && drawn[0].fill().is_none(), "the shadow first, under the object");
        assert!(!obj.hit(Point::new(250.0, 25.0), 1.0), "the shadow is not hit");
    }
}

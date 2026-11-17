//! Bitmap pixels: decoding a placed bitmap, writing pixels back as PNG, resampling, and
//! drawing vector objects into a new bitmap (Convert to Bitmap).

use std::fmt::Write as _;
use std::sync::Arc;

use vs_core::kurbo::{Affine, Rect};
use vs_core::raster::{Effect, Pixels};
use vs_core::{Bitmap, BitmapFormat, Object, Rgba};

use crate::IoError;

/// The bitmap's pixels, decoded.
pub fn bitmap_pixels(b: &Bitmap) -> Result<Pixels, IoError> {
    let image = image::load_from_memory(&b.data).map_err(|e| IoError::Image(e.to_string()))?.into_rgba8();
    let (w, h) = image.dimensions();
    Pixels::new(w, h, image.into_raw()).ok_or_else(|| IoError::Image("unexpected pixel layout".into()))
}

/// A PNG bitmap of `pixels`, placed at `rect` (in the object's local space).
pub fn bitmap_from_pixels(pixels: &Pixels, rect: Rect) -> Result<Bitmap, IoError> {
    let mut data = Vec::new();
    image::codecs::png::PngEncoder::new(&mut data)
        .write_image_rgba(pixels)
        .map_err(|e| IoError::Png(e.to_string()))?;
    Ok(Bitmap { format: BitmapFormat::Png, width: pixels.width, height: pixels.height, rect, data: Arc::new(data) })
}

trait WriteRgba {
    fn write_image_rgba(self, pixels: &Pixels) -> image::ImageResult<()>;
}

impl<W: std::io::Write> WriteRgba for image::codecs::png::PngEncoder<W> {
    fn write_image_rgba(self, pixels: &Pixels) -> image::ImageResult<()> {
        use image::ImageEncoder as _;
        self.write_image(&pixels.data, pixels.width, pixels.height, image::ExtendedColorType::Rgba8)
    }
}

/// `b` with `effect` applied, as a PNG in the same place.
pub fn apply_effect(b: &Bitmap, effect: &Effect) -> Result<Bitmap, IoError> {
    bitmap_from_pixels(&effect.apply(&bitmap_pixels(b)?), b.rect)
}

/// `b` resampled to `width` × `height` pixels, in the same place: the same size on the
/// page at a new resolution.
pub fn resample(b: &Bitmap, width: u32, height: u32) -> Result<Bitmap, IoError> {
    let pixels = bitmap_pixels(b)?;
    let image = image::RgbaImage::from_raw(pixels.width, pixels.height, pixels.data).ok_or_else(|| IoError::Image("unexpected pixel layout".into()))?;
    let resized = image::imageops::resize(&image, width.max(1), height.max(1), image::imageops::FilterType::Lanczos3);
    let (w, h) = resized.dimensions();
    let pixels = Pixels::new(w, h, resized.into_raw()).ok_or_else(|| IoError::Image("unexpected pixel layout".into()))?;
    bitmap_from_pixels(&pixels, b.rect)
}

/// `pixels` shrunk to fit `longest` pixels on its longer side (as they are when they
/// already fit), and the scale it was shrunk by.
pub fn shrink(pixels: &Pixels, longest: u32) -> (Pixels, f64) {
    let side = pixels.width.max(pixels.height);
    if side <= longest {
        return (pixels.clone(), 1.0);
    }
    let factor = f64::from(longest) / f64::from(side);
    let w = ((f64::from(pixels.width) * factor).round() as u32).max(1);
    let h = ((f64::from(pixels.height) * factor).round() as u32).max(1);
    let Some(image) = image::RgbaImage::from_raw(pixels.width, pixels.height, pixels.data.clone()) else { return (pixels.clone(), 1.0) };
    let small = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
    (Pixels { width: w, height: h, data: small.into_raw() }, factor)
}

/// Area of `objects` that drawing them covers: their bounds with room for half the widest
/// outline.
pub fn drawn_bounds(objects: &[&Object]) -> Option<Rect> {
    let bounds = objects.iter().map(|o| o.world_bounds()).reduce(|a, b| a.union(b))?;
    let outline = objects
        .iter()
        .filter_map(|o| o.style.outline.as_ref().map(|outline| outline.page_width(o.transform)))
        .fold(0.0, f64::max);
    Some(bounds.inflate(outline / 2.0, outline / 2.0))
}

/// `objects` (bottom first) drawn into a bitmap at `dpi`, on `background` or transparent.
/// The bitmap covers [`drawn_bounds`], rounded out to whole pixels, and is placed there.
pub fn objects_to_bitmap(objects: &[&Object], dpi: f64, background: Option<Rgba>) -> Result<Bitmap, IoError> {
    use resvg::{tiny_skia, usvg};
    let bounds = drawn_bounds(objects).ok_or_else(|| IoError::Png("nothing to convert".into()))?;
    let scale = dpi / 72.0;
    let width = (bounds.width() * scale).ceil().max(1.0) as u32;
    let height = (bounds.height() * scale).ceil().max(1.0) as u32;
    // The whole pixels, back in points, so the bitmap is not stretched.
    let rect = Rect::from_origin_size(bounds.origin(), (f64::from(width) / scale, f64::from(height) / scale));

    let mut svg = String::new();
    let _ = writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{width}" height="{height}">"#);
    let to_pixels = Affine::scale(scale) * Affine::translate(-rect.origin().to_vec2());
    for obj in objects {
        crate::write_svg_object(&mut svg, obj, to_pixels, 1);
    }
    svg.push_str("</svg>\n");

    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| IoError::Png(e.to_string()))?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| IoError::Png(format!("image too large ({width}×{height})")))?;
    if let Some(bg) = background {
        pixmap.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
    }
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied colour; the bitmap file wants it straight.
    let data = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    let pixels = Pixels::new(width, height, data.collect()).ok_or_else(|| IoError::Png("unexpected pixel layout".into()))?;
    bitmap_from_pixels(&pixels, rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vs_core::{Document, Shape, Style};

    fn red_square_doc() -> Document {
        let mut doc = Document::default();
        let style = Style::filled(Rgba::rgb(255, 0, 0));
        doc.add_object(Shape::Rectangle { rect: Rect::new(10.0, 20.0, 46.0, 56.0), corner_radius: 0.0 }, style);
        doc
    }

    #[test]
    fn objects_become_a_bitmap_in_their_place() {
        let doc = red_square_doc();
        let objects: Vec<&Object> = doc.active_page().objects.iter().collect();
        let b = objects_to_bitmap(&objects, 144.0, None).unwrap();
        assert_eq!((b.width, b.height), (72, 72));
        assert_eq!(b.rect, Rect::new(10.0, 20.0, 46.0, 56.0));
        let pixels = bitmap_pixels(&b).unwrap();
        assert_eq!(pixels.pixel(36, 36), [255, 0, 0, 255]);
        let white = objects_to_bitmap(&objects, 72.0, Some(Rgba::WHITE)).unwrap();
        assert_eq!(bitmap_pixels(&white).unwrap().pixel(0, 0)[3], 255);
    }

    #[test]
    fn effects_and_resampling_keep_the_place() {
        let doc = red_square_doc();
        let objects: Vec<&Object> = doc.active_page().objects.iter().collect();
        let b = objects_to_bitmap(&objects, 72.0, None).unwrap();
        let grey = apply_effect(&b, &Effect::Grayscale).unwrap();
        assert_eq!(grey.rect, b.rect);
        assert_eq!(bitmap_pixels(&grey).unwrap().pixel(10, 10), [76, 76, 76, 255]);
        let (preview, factor) = shrink(&bitmap_pixels(&b).unwrap(), 9);
        assert_eq!((preview.width, preview.height, factor), (9, 9, 0.25));
        assert_eq!(preview.pixel(4, 4), [255, 0, 0, 255]);
        let small = resample(&b, 12, 12).unwrap();
        assert_eq!((small.width, small.height, small.rect), (12, 12, b.rect));
        assert_eq!(bitmap_pixels(&small).unwrap().pixel(6, 6), [255, 0, 0, 255]);
    }
}

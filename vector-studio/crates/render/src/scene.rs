//! Builds a Vello scene for one page of a document.

use kurbo::{Affine, Rect, Shape as _, Stroke};
use vello::Scene;
use vello::peniko::{BlendMode, Brush, Color, Fill, Gradient, Mix};
use vs_core::{GradientKind, Page, Rgba, Shape};

use crate::viewport::Viewport;

#[derive(Clone, Copy, Debug)]
pub struct CanvasColors {
    pub pasteboard: Color,
    pub page: Color,
    /// `None` draws no border (the reference app shows none).
    pub page_border: Option<Color>,
    pub page_shadow: Option<Color>,
}

impl Default for CanvasColors {
    fn default() -> Self {
        Self {
            // Grey desk, plain white page.
            pasteboard: Color::from_rgb8(0x5A, 0x5A, 0x5A),
            page: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            page_border: None,
            page_shadow: None,
        }
    }
}

/// How objects are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ViewMode {
    /// Fills, outlines and bitmaps, anti-aliased.
    #[default]
    Enhanced,
    /// Every outline as a one-pixel line: no fills, bitmaps as frames, PowerClip
    /// contents not clipped.
    Wireframe,
    /// As Enhanced, but as the bitmap the page would export to: drawn at the export
    /// resolution and magnified without smoothing (see [`pixel_grid`]).
    Pixels,
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Enhanced => "Enhanced",
            ViewMode::Wireframe => "Wireframe",
            ViewMode::Pixels => "Pixels",
        }
    }
}

/// How the canvas draws objects: the view mode and View-menu simulations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub mode: ViewMode,
    /// Simulate Overprints: overprinted fills and outlines multiply with what is below,
    /// as inks printed on top of each other do.
    pub overprints: bool,
}

impl From<ViewMode> for Look {
    fn from(mode: ViewMode) -> Self {
        Self { mode, ..Self::default() }
    }
}

/// How Pixels view draws the page: into a texture whose pixels are the pixels of the
/// page exported at `dpi`, which the canvas then shows magnified.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelGrid {
    /// Document → texture pixels (with a pixels-per-point of 1). The page's top-left
    /// corner falls exactly on a texture pixel corner.
    pub viewport: Viewport,
    /// Texture size, enough to cover the canvas.
    pub size: [u32; 2],
    /// Where texture pixel (0, 0) goes on the canvas, in logical points from its corner.
    pub origin: kurbo::Vec2,
    /// Logical canvas points per texture pixel.
    pub scale: f64,
}

/// The Pixels view grid for a canvas of `canvas_px` physical pixels showing `viewport`.
/// `None` while a bitmap pixel would be no bigger than a screen pixel: then the page
/// looks the same as in Enhanced view.
pub fn pixel_grid(viewport: &Viewport, canvas_px: [u32; 2], pixels_per_point: f64, dpi: f64) -> Option<PixelGrid> {
    let per_point = dpi / 72.0;
    // Physical screen pixels per bitmap pixel.
    let k = viewport.zoom() * pixels_per_point / per_point;
    if k <= 1.0 || !k.is_finite() {
        return None;
    }
    let offset = viewport.offset() * pixels_per_point / k;
    let whole = kurbo::Vec2::new(offset.x.floor(), offset.y.floor());
    let frac = offset - whole;
    let size = canvas_px.map(|n| (f64::from(n) / k).ceil() as u32 + 2);
    Some(PixelGrid {
        viewport: Viewport::new(per_point, whole),
        size,
        origin: frac * k / pixels_per_point,
        scale: k / pixels_per_point,
    })
}

/// Colour of wireframe lines.
const WIREFRAME: Color = Color::from_rgb8(0x20, 0x20, 0x20);

pub fn to_color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// The brush for `fill` at `opacity`, in the fill's (object-local) space.
pub fn to_brush(fill: &vs_core::Fill, opacity: f64) -> Brush {
    match fill {
        vs_core::Fill::Solid(c) => Brush::Solid(to_color(c.faded(opacity))),
        vs_core::Fill::Gradient(g) => {
            let gradient = match g.kind {
                GradientKind::Linear => Gradient::new_linear(g.start, g.end),
                GradientKind::Radial => Gradient::new_radial(g.start, (g.end - g.start).hypot() as f32),
            };
            let stops: Vec<(f32, Color)> = g.stops.iter().map(|s| (s.offset as f32, to_color(s.color.faded(opacity)))).collect();
            Brush::Gradient(gradient.with_stops(stops.as_slice()))
        }
    }
}

/// Append `page` to `scene`.
///
/// `pixels_per_point` converts the viewport's logical screen points to the
/// physical pixels of the target texture.
pub fn build_page_scene(
    scene: &mut Scene,
    page: &Page,
    viewport: &Viewport,
    pixels_per_point: f64,
    colors: &CanvasColors,
    look: impl Into<Look>,
) {
    let Look { mode, overprints } = look.into();
    let screen = Affine::scale(pixels_per_point);
    let doc = screen * viewport.transform();
    let page_rect = page.bounds();

    // Page shadow and paper are drawn in screen space so they stay crisp at any zoom.
    let page_screen = transform_rect(viewport, page_rect);
    if let Some(shadow) = colors.page_shadow {
        scene.draw_blurred_rounded_rect(screen, page_screen + kurbo::Vec2::new(2.0, 3.0), shadow, 0.0, 4.0);
    }
    scene.fill(Fill::NonZero, screen, colors.page, None, &page_screen);

    // Zero width = hairline: one physical pixel at any zoom.
    let hairline = 1.0 / (viewport.zoom() * pixels_per_point);
    if mode == ViewMode::Wireframe {
        let line = Stroke::new(hairline);
        for leaf in page.shown_objects().flat_map(vs_core::Object::leaves) {
            scene.stroke(&line, doc, WIREFRAME, None, &leaf.path);
        }
        if let Some(border) = colors.page_border {
            scene.stroke(&Stroke::new(1.0), screen, border, None, &page_screen);
        }
        return;
    }
    for object in page.shown_objects() {
        for leaf in object.leaves() {
            let opacity = leaf.style.opacity();
            let fill = leaf.fill().map(|f| to_brush(f, opacity));
            let outline = leaf.style.outline.as_ref().map(|o| {
                let width = if o.width > 0.0 { o.page_width(leaf.transform) } else { hairline };
                (o.stroke(width), to_color(o.color.faded(opacity)), o.behind_fill)
            });
            let draw_fill = |scene: &mut Scene| {
                if let Shape::Image(bitmap) = leaf.shape {
                    if let Some(image) = crate::bitmaps::decoded(bitmap) {
                        let place = Affine::translate(bitmap.rect.origin().to_vec2())
                            * Affine::scale_non_uniform(bitmap.rect.width() / f64::from(image.width), bitmap.rect.height() / f64::from(image.height));
                        let mut brush = vello::peniko::ImageBrush::new(image);
                        brush.sampler.alpha = opacity as f32;
                        scene.draw_image(&brush, doc * leaf.transform * place);
                    }
                    return;
                }
                if let Some(brush) = &fill {
                    // Even-odd: overlaps in a combined curve are holes.
                    // Fountain fills are in local space; the path is already on the page.
                    scene.fill(Fill::EvenOdd, doc, brush, Some(leaf.transform), &leaf.path);
                }
            };
            // PowerClip contents show only inside their containers.
            for clip in &leaf.clips {
                scene.push_clip_layer(Fill::EvenOdd, doc, clip);
            }
            // An overprinted part is drawn in a layer that multiplies with what is below.
            let reach = outline.as_ref().map_or(0.0, |(stroke, ..)| stroke.width * stroke.miter_limit.max(1.0));
            let bounds = leaf.path.bounding_box().inflate(reach + 1.0, reach + 1.0);
            let overprinted = |scene: &mut Scene, on: bool, draw: &dyn Fn(&mut Scene)| {
                let on = overprints && on;
                if on {
                    scene.push_layer(Fill::NonZero, BlendMode::from(Mix::Multiply), 1.0, doc, &bounds);
                }
                draw(scene);
                if on {
                    scene.pop_layer();
                }
            };
            let fill_part = |scene: &mut Scene| overprinted(scene, leaf.style.overprint_fill, &draw_fill);
            match &outline {
                Some((stroke, color, behind)) => {
                    let outline_part = |scene: &mut Scene| {
                        overprinted(scene, leaf.style.overprint_outline, &|scene: &mut Scene| scene.stroke(stroke, doc, *color, None, &leaf.path));
                    };
                    if *behind {
                        outline_part(scene);
                        fill_part(scene);
                    } else {
                        fill_part(scene);
                        outline_part(scene);
                    }
                }
                None => fill_part(scene),
            }
            for _ in &leaf.clips {
                scene.pop_layer();
            }
        }
    }

    if let Some(border) = colors.page_border {
        scene.stroke(&Stroke::new(1.0), screen, border, None, &page_screen);
    }
}

fn transform_rect(viewport: &Viewport, r: Rect) -> Rect {
    let p0 = viewport.doc_to_screen(kurbo::Point::new(r.x0, r.y0));
    let p1 = viewport.doc_to_screen(kurbo::Point::new(r.x1, r.y1));
    // Snap to the pixel grid so the page edge is sharp.
    Rect::new(p0.x.round(), p0.y.round(), p1.x.round(), p1.y.round())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Point, Vec2};

    #[test]
    fn pixel_grid_lines_bitmap_pixels_up_with_the_page_and_the_screen() {
        let mut view = Viewport::new(8.0, Vec2::new(37.3, -12.6));
        let grid = pixel_grid(&view, [1000, 600], 2.0, 72.0).expect("8 pt per bitmap pixel on screen");
        assert_eq!(grid.scale, 8.0, "one 72 dpi pixel per document point, 8 screen points wide");
        let origin = grid.viewport.doc_to_screen(Point::ZERO);
        assert_eq!((origin.x.fract(), origin.y.fract()), (0.0, 0.0), "the page corner is a pixel corner");
        for p in [Point::ZERO, Point::new(10.0, 3.0), Point::new(-2.5, 40.25)] {
            let shown = grid.origin + grid.viewport.doc_to_screen(p).to_vec2() * grid.scale;
            assert!((shown - view.doc_to_screen(p).to_vec2()).hypot() < 1e-9, "{p:?} lands where Enhanced draws it");
        }
        assert!(grid.size[0] as f64 * grid.scale * 2.0 >= 1000.0 && grid.size[1] as f64 * grid.scale * 2.0 >= 600.0, "covers the canvas");

        view = Viewport::new(1.0, Vec2::ZERO);
        assert_eq!(pixel_grid(&view, [1000, 600], 2.0, 300.0), None, "pixels smaller than the screen's");
    }
}

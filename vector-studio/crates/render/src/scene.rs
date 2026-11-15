//! Builds a Vello scene for one page of a document.

use kurbo::{Affine, Rect, Stroke};
use vello::Scene;
use vello::peniko::{Color, Fill};
use vs_core::{Page, Rgba};

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

pub fn to_color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
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
) {
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
    for object in &page.objects {
        for leaf in object.leaves() {
            let fill = leaf.fill();
            let outline = leaf.style.outline.as_ref().map(|o| {
                let width = if o.width > 0.0 { o.page_width(leaf.transform) } else { hairline };
                (o.stroke(width), to_color(o.color), o.behind_fill)
            });
            let draw_fill = |scene: &mut Scene| {
                if let Some(fill) = fill {
                    scene.fill(Fill::NonZero, doc, to_color(fill), None, &leaf.path);
                }
            };
            match outline {
                Some((stroke, color, true)) => {
                    scene.stroke(&stroke, doc, color, None, &leaf.path);
                    draw_fill(scene);
                }
                Some((stroke, color, false)) => {
                    draw_fill(scene);
                    scene.stroke(&stroke, doc, color, None, &leaf.path);
                }
                None => draw_fill(scene),
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

//! Import: SVG files become editable curves; PNG and JPEG files become bitmaps.

use resvg::usvg;
use std::sync::Arc;

use vs_core::kurbo::{Affine, BezPath, Point, Rect};
use vs_core::{Bitmap, BitmapFormat, Fill, Gradient, GradientKind, GradientStop, LineCap, LineJoin, LineStyle, Object, ObjectId, Outline, Rgba, Shape, Style};

use crate::IoError;

/// SVG user units are CSS pixels (96 per inch); the document works in points (72).
const PX_TO_PT: f64 = 72.0 / 96.0;

/// Read an SVG file as objects in page space (points), with the drawing's top-left corner
/// at the origin. Several top-level shapes come back as one group. Shapes, groups,
/// solid and fountain fills, outlines and opacity are kept; text, images, patterns,
/// clipping, masks and filters are not.
pub fn import_svg(data: &[u8]) -> Result<Vec<Object>, IoError> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| IoError::Svg(e.to_string()))?;
    let mut objects = Vec::new();
    collect(tree.root(), 1.0, &mut objects);
    if objects.len() > 1 {
        objects = vec![object(Shape::Group { children: objects }, Style::default())];
    }
    Ok(objects)
}

/// Read a PNG or JPEG file as a bitmap object at 96 pixels per inch, its top-left corner
/// at the origin.
pub fn import_bitmap(data: &[u8]) -> Result<Object, IoError> {
    let format = match image::guess_format(data) {
        Ok(image::ImageFormat::Png) => BitmapFormat::Png,
        Ok(image::ImageFormat::Jpeg) => BitmapFormat::Jpeg,
        _ => return Err(IoError::Image("only PNG and JPEG files can be imported".into())),
    };
    // Decode it all now, so a damaged file is refused here and not drawn blank later.
    let decoded = image::load_from_memory(data).map_err(|e| IoError::Image(e.to_string()))?;
    let (width, height) = (decoded.width(), decoded.height());
    let rect = Rect::new(0.0, 0.0, f64::from(width) * PX_TO_PT, f64::from(height) * PX_TO_PT);
    let bitmap = Bitmap { format, width, height, rect, data: Arc::new(data.to_vec()) };
    Ok(object(Shape::Image(bitmap), Style::default()))
}

fn object(shape: Shape, style: Style) -> Object {
    Object { id: ObjectId(0), name: None, shape, style, transform: Affine::IDENTITY, locked: false, layer: vs_core::LayerId::UNSET, contents: Vec::new() }
}

fn affine(t: usvg::Transform) -> Affine {
    Affine::new([t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from))
}

/// The objects under `group`, in page space. `opacity` is the product of the opacity of
/// the groups around them.
fn collect(group: &usvg::Group, opacity: f64, out: &mut Vec<Object>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => {
                let mut children = Vec::new();
                collect(g, opacity * f64::from(g.opacity().get()), &mut children);
                match children.len() {
                    0 => {}
                    1 => out.append(&mut children),
                    _ => out.push(object(Shape::Group { children }, Style::default())),
                }
            }
            usvg::Node::Path(p) if p.is_visible() => {
                if let Some(obj) = path_object(p, opacity) {
                    out.push(obj);
                }
            }
            _ => {}
        }
    }
}

fn path_object(p: &usvg::Path, opacity: f64) -> Option<Object> {
    let to_page = Affine::scale(PX_TO_PT) * affine(p.abs_transform());
    let mut path = BezPath::new();
    for seg in p.data().segments() {
        let pt = |q: usvg::tiny_skia_path::Point| to_page * Point::new(f64::from(q.x), f64::from(q.y));
        match seg {
            usvg::tiny_skia_path::PathSegment::MoveTo(a) => path.move_to(pt(a)),
            usvg::tiny_skia_path::PathSegment::LineTo(a) => path.line_to(pt(a)),
            usvg::tiny_skia_path::PathSegment::QuadTo(a, b) => path.quad_to(pt(a), pt(b)),
            usvg::tiny_skia_path::PathSegment::CubicTo(a, b, c) => path.curve_to(pt(a), pt(b), pt(c)),
            usvg::tiny_skia_path::PathSegment::Close => path.close_path(),
        }
    }
    if path.elements().len() < 2 {
        return None;
    }
    let fill = p.fill().and_then(|f| {
        // The renderer fills even-odd; a non-zero fill becomes the area it covers.
        if f.rule() == usvg::FillRule::NonZero
            && let Some(region) = vs_core::curves::nonzero_region(&path).filter(|r| !r.is_empty())
        {
            path = region;
        }
        paint(f.paint(), f64::from(f.opacity().get()), to_page)
    });
    let outline = p.stroke().map(|s| {
        let color = match paint(s.paint(), f64::from(s.opacity().get()), to_page) {
            Some(Fill::Solid(c)) => c,
            Some(Fill::Gradient(g)) => g.stops.first().map_or(Rgba::BLACK, |s| s.color),
            None => Rgba::BLACK,
        };
        let scale = to_page.determinant().abs().sqrt();
        Outline {
            color,
            width: f64::from(s.width().get()) * scale,
            style: if s.dasharray().is_some_and(|d| !d.is_empty()) { LineStyle::Dash } else { LineStyle::Solid },
            join: match s.linejoin() {
                usvg::LineJoin::Round => LineJoin::Round,
                usvg::LineJoin::Bevel => LineJoin::Bevel,
                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
            },
            cap: match s.linecap() {
                usvg::LineCap::Butt => LineCap::Butt,
                usvg::LineCap::Round => LineCap::Round,
                usvg::LineCap::Square => LineCap::Square,
            },
            behind_fill: p.paint_order() == usvg::PaintOrder::StrokeAndFill,
            scale_with_object: false,
        }
    });
    if fill.is_none() && outline.is_none() {
        return None;
    }
    let style = Style { fill, outline, transparency: 1.0 - opacity.clamp(0.0, 1.0) };
    Some(object(Shape::Curve { path }, style))
}

fn color(c: usvg::Color, alpha: f64) -> Rgba {
    Rgba { r: c.red, g: c.green, b: c.blue, a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8 }
}

/// A fill for `paint` with the path's `to_page` transform. Patterns are not supported.
fn paint(paint: &usvg::Paint, opacity: f64, to_page: Affine) -> Option<Fill> {
    let gradient = |base: &usvg::BaseGradient, kind, start: Point, end: Point| {
        let t = to_page * affine(base.transform());
        let stops = base
            .stops()
            .iter()
            .map(|s| GradientStop { offset: f64::from(s.offset().get()), color: color(s.color(), opacity * f64::from(s.opacity().get())) })
            .collect();
        Fill::Gradient(Gradient { kind, start: t * start, end: t * end, stops })
    };
    match paint {
        usvg::Paint::Color(c) => Some(Fill::Solid(color(*c, opacity))),
        usvg::Paint::LinearGradient(g) => {
            let (a, b) = (Point::new(g.x1().into(), g.y1().into()), Point::new(g.x2().into(), g.y2().into()));
            Some(gradient(g, GradientKind::Linear, a, b))
        }
        usvg::Paint::RadialGradient(g) => {
            let c = Point::new(g.cx().into(), g.cy().into());
            Some(gradient(g, GradientKind::Radial, c, c + (f64::from(g.r().get()), 0.0)))
        }
        usvg::Paint::Pattern(_) => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use vs_core::kurbo::Shape as _;

    fn import(svg: &str) -> Vec<Object> {
        import_svg(svg.as_bytes()).expect("parses")
    }

    #[test]
    fn a_rectangle_comes_in_as_a_curve_in_points() {
        let objs = import(r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200"><rect x="10" y="20" width="96" height="48" fill="#ff0000" stroke="#0000ff" stroke-width="4"/></svg>"##);
        assert_eq!(objs.len(), 1);
        let o = &objs[0];
        assert!(matches!(o.shape, Shape::Curve { .. }));
        let b = o.world_bounds();
        assert!((b.x0 - 7.5).abs() < 1e-6 && (b.y0 - 15.0).abs() < 1e-6 && (b.width() - 72.0).abs() < 1e-6 && (b.height() - 36.0).abs() < 1e-6, "{b:?}");
        assert_eq!(o.style.fill, Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
        let outline = o.style.outline.as_ref().expect("outline");
        assert_eq!((outline.color, outline.width), (Rgba::rgb(0, 0, 255), 3.0));
    }

    #[test]
    fn several_shapes_come_in_as_one_group_with_transforms_and_opacity() {
        let objs = import(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <g transform="translate(40 0)" opacity="0.5"><circle cx="10" cy="10" r="10" fill="green"/></g>
                <path d="M0 0 L40 0 L40 40 Z" fill="none" stroke="black" stroke-dasharray="4 2"/>
            </svg>"##,
        );
        assert_eq!(objs.len(), 1);
        let Shape::Group { children } = &objs[0].shape else { panic!("group") };
        assert_eq!(children.len(), 2);
        let circle = &children[0];
        let b = circle.world_bounds();
        assert!((b.x0 - 30.0).abs() < 1e-3 && (b.width() - 15.0).abs() < 1e-3, "{b:?}");
        assert!((circle.style.transparency - 0.5).abs() < 1e-6);
        let path = &children[1];
        assert_eq!(path.style.fill, None);
        assert_eq!(path.style.outline.as_ref().unwrap().style, LineStyle::Dash);
        assert!((path.world_path().area().abs() - 450.0).abs() < 1e-3, "30 × 30 pt half square");
    }

    #[test]
    fn gradients_and_bad_files() {
        let objs = import(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
                <rect width="80" height="40" fill="url(#g)"/>
            </svg>"##,
        );
        let Some(Fill::Gradient(g)) = &objs[0].style.fill else { panic!("gradient") };
        assert_eq!(g.kind, GradientKind::Linear);
        assert!((g.start.x).abs() < 1e-6 && (g.end.x - 60.0).abs() < 1e-6, "bounding-box units become page points: {g:?}");
        assert_eq!((g.stops[0].color, g.stops[1].color), (Rgba::BLACK, Rgba::WHITE));
        assert!(import_svg(b"not svg").is_err());
    }

    /// A `w`×`h` PNG, all `rgb`.
    pub(crate) fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn png_comes_in_at_96_pixels_per_inch_and_keeps_its_bytes() {
        let data = png(40, 20, [255, 0, 0]);
        let obj = import_bitmap(&data).expect("decodes");
        let Shape::Image(b) = &obj.shape else { panic!("bitmap") };
        assert_eq!((b.format, b.width, b.height), (BitmapFormat::Png, 40, 20));
        assert_eq!(b.rect, Rect::new(0.0, 0.0, 30.0, 15.0));
        assert_eq!(b.data.as_slice(), data.as_slice());
        assert!(matches!(import_bitmap(b"GIF89a....").unwrap_err(), IoError::Image(_)));
        let mut broken = data.clone();
        broken.truncate(40);
        assert!(import_bitmap(&broken).is_err(), "a damaged PNG is refused");
    }
}

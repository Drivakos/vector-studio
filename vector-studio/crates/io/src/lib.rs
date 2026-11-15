//! File formats: the native document format, and SVG / PNG / PDF export.

use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vs_core::kurbo::{BezPath, PathEl};
use vs_core::geometry::Leaf;
use vs_core::{Document, LineCap, LineJoin, Object, Outline, Page, Rgba, Shape};

/// Extension of the native format (JSON).
pub const FILE_EXTENSION: &str = "vstudio";
const FORMAT_NAME: &str = "vector-studio";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug)]
pub enum IoError {
    Io(std::io::Error),
    Json(serde_json::Error),
    NotOurFormat,
    NewerVersion(u32),
    UnknownExtension(String),
    Png(String),
    Pdf(String),
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoError::Io(e) => write!(f, "{e}"),
            IoError::Json(e) => write!(f, "the file is damaged or not a Vector Studio document ({e})"),
            IoError::NotOurFormat => write!(f, "the file is not a Vector Studio document"),
            IoError::NewerVersion(v) => write!(f, "the file was saved by a newer version (format {v})"),
            IoError::UnknownExtension(e) => write!(f, "cannot export to .{e}; use .svg, .png or .pdf"),
            IoError::Png(e) => write!(f, "PNG export failed: {e}"),
            IoError::Pdf(e) => write!(f, "PDF export failed: {e}"),
        }
    }
}

impl std::error::Error for IoError {}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::Io(e)
    }
}

impl From<serde_json::Error> for IoError {
    fn from(e: serde_json::Error) -> Self {
        IoError::Json(e)
    }
}

// ---------------------------------------------------------------------------------------
// Native format

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'a str,
    version: u32,
    document: &'a Document,
}

#[derive(Deserialize)]
struct FileIn {
    format: String,
    version: u32,
    document: Document,
}

pub fn document_to_string(doc: &Document) -> Result<String, IoError> {
    let file = FileOut { format: FORMAT_NAME, version: FORMAT_VERSION, document: doc };
    Ok(serde_json::to_string_pretty(&file)?)
}

pub fn document_from_str(text: &str) -> Result<Document, IoError> {
    let file: FileIn = serde_json::from_str(text)?;
    if file.format != FORMAT_NAME {
        return Err(IoError::NotOurFormat);
    }
    if file.version > FORMAT_VERSION {
        return Err(IoError::NewerVersion(file.version));
    }
    let mut doc = file.document;
    doc.repair();
    Ok(doc)
}

pub fn save_document(doc: &Document, path: &Path) -> Result<(), IoError> {
    let text = document_to_string(doc)?;
    // Write next to the target, then rename, so a failed save never truncates the file.
    let tmp = path.with_extension(format!("{FILE_EXTENSION}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn open_document(path: &Path) -> Result<Document, IoError> {
    document_from_str(&std::fs::read_to_string(path)?)
}

// ---------------------------------------------------------------------------------------
// Export

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Png,
    Pdf,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 3] = [ExportFormat::Svg, ExportFormat::Png, ExportFormat::Pdf];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Png => "png",
            ExportFormat::Pdf => "pdf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Svg => "SVG - Scalable Vector Graphics",
            ExportFormat::Png => "PNG - Portable Network Graphics",
            ExportFormat::Pdf => "PDF - Portable Document Format",
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, IoError> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|f| f.extension() == ext)
            .ok_or(IoError::UnknownExtension(ext))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// PNG resolution, pixels per inch.
    pub dpi: f64,
    /// PNG background; `None` keeps it transparent.
    pub background: Option<Rgba>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { dpi: 300.0, background: None }
    }
}

/// Export to `path`, picking the format from its extension. SVG and PNG export the
/// active page; PDF exports every page.
pub fn export(doc: &Document, path: &Path, options: &ExportOptions) -> Result<ExportFormat, IoError> {
    let format = ExportFormat::from_path(path)?;
    let bytes = match format {
        ExportFormat::Svg => page_to_svg(doc.active_page()).into_bytes(),
        ExportFormat::Png => page_to_png(doc.active_page(), options)?,
        ExportFormat::Pdf => document_to_pdf(doc)?,
    };
    std::fs::write(path, bytes)?;
    Ok(format)
}

/// One `<path>` per drawable object; groups become `<g>` around their members.
fn write_svg_object(out: &mut String, obj: &Object, parent: vs_core::kurbo::Affine, depth: usize) {
    let indent = "  ".repeat(depth);
    if let Shape::Group { children } = &obj.shape {
        let _ = writeln!(out, "{indent}<g>");
        for child in children {
            write_svg_object(out, child, parent * obj.transform, depth + 1);
        }
        let _ = writeln!(out, "{indent}</g>");
        return;
    }
    let mut path = obj.shape.to_path();
    let transform = parent * obj.transform;
    path.apply_affine(transform);
    round_path(&mut path);
    let _ = write!(out, r#"{indent}<path d="{}""#, path.to_svg());
    match obj.style.fill.filter(|_| obj.shape.is_closed()) {
        Some(c) => {
            let _ = write!(out, r#" fill="{}" fill-rule="evenodd""#, hex(c));
            if c.a < 255 {
                let _ = write!(out, r#" fill-opacity="{}""#, num(f64::from(c.a) / 255.0));
            }
        }
        None => out.push_str(r#" fill="none""#),
    }
    if let Some(o) = &obj.style.outline {
        write_svg_stroke(out, o, o.page_width(transform));
    }
    out.push_str("/>\n");
}

fn write_svg_stroke(out: &mut String, o: &Outline, width: f64) {
    let _ = write!(out, r#" stroke="{}" stroke-width="{}" stroke-miterlimit="{}""#, hex(o.color), num(width), num(Outline::MITER_LIMIT));
    if o.color.a < 255 {
        let _ = write!(out, r#" stroke-opacity="{}""#, num(f64::from(o.color.a) / 255.0));
    }
    match o.join {
        LineJoin::Miter => {}
        LineJoin::Round => out.push_str(r#" stroke-linejoin="round""#),
        LineJoin::Bevel => out.push_str(r#" stroke-linejoin="bevel""#),
    }
    match o.cap {
        LineCap::Butt => {}
        LineCap::Round => out.push_str(r#" stroke-linecap="round""#),
        LineCap::Square => out.push_str(r#" stroke-linecap="square""#),
    }
    let dashes = o.dashes(width);
    if !dashes.is_empty() {
        let list: Vec<String> = dashes.iter().map(|d| num(*d)).collect();
        let _ = write!(out, r#" stroke-dasharray="{}""#, list.join(" "));
    }
    if o.behind_fill {
        out.push_str(r#" paint-order="stroke""#);
    }
}

fn hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// SVG 1.1 document of one page; 1 user unit = 1 pt.
pub fn page_to_svg(page: &Page) -> String {
    let (w, h) = (page.size.width, page.size.height);
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{}pt" height="{}pt" viewBox="0 0 {} {}">"#,
        num(w), num(h), num(w), num(h)
    );
    for obj in &page.objects {
        write_svg_object(&mut out, obj, vs_core::kurbo::Affine::IDENTITY, 1);
    }
    out.push_str("</svg>\n");
    out
}

/// Keep exported coordinates short: 3 decimals of a point is far below print resolution.
fn round_path(path: &mut BezPath) {
    let r = |p: &mut vs_core::kurbo::Point| {
        *p = ((p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0).into();
    };
    for el in path.elements_mut() {
        match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => r(p),
            PathEl::QuadTo(a, p) => {
                r(a);
                r(p);
            }
            PathEl::CurveTo(a, b, p) => {
                r(a);
                r(b);
                r(p);
            }
            PathEl::ClosePath => {}
        }
    }
}

/// Render one page to PNG at `options.dpi`.
pub fn page_to_png(page: &Page, options: &ExportOptions) -> Result<Vec<u8>, IoError> {
    use resvg::{tiny_skia, usvg};
    let svg = page_to_svg(page);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| IoError::Png(e.to_string()))?;
    let scale = options.dpi / 72.0;
    let width = (page.size.width * scale).ceil().max(1.0) as u32;
    let height = (page.size.height * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| IoError::Png(format!("image too large ({width}×{height})")))?;
    if let Some(bg) = options.background {
        pixmap.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
    }
    // Map the SVG's own size (whatever px it resolved pt to) onto the page at `dpi`.
    let k = (page.size.width * scale) as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| IoError::Png(e.to_string()))
}

fn pdf_path(leaf: &Leaf<'_>) -> Option<krilla::geom::Path> {
    let mut pb = krilla::geom::PathBuilder::new();
    let f = |p: vs_core::kurbo::Point| (p.x as f32, p.y as f32);
    for el in leaf.path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                let (x, y) = f(p);
                pb.move_to(x, y)
            }
            PathEl::LineTo(p) => {
                let (x, y) = f(p);
                pb.line_to(x, y)
            }
            PathEl::QuadTo(a, p) => {
                let ((ax, ay), (x, y)) = (f(a), f(p));
                pb.quad_to(ax, ay, x, y)
            }
            PathEl::CurveTo(a, b, p) => {
                let ((ax, ay), (bx, by), (x, y)) = (f(a), f(b), f(p));
                pb.cubic_to(ax, ay, bx, by, x, y)
            }
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// Every page of the document as a PDF, 1 unit = 1 pt.
pub fn document_to_pdf(doc: &Document) -> Result<Vec<u8>, IoError> {
    use krilla::color::rgb;
    use krilla::num::NormalizedF32;
    use krilla::page::PageSettings;
    use krilla::paint::{Fill, FillRule, Stroke, StrokeDash};

    let opacity = |a: u8| NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE);
    let mut pdf = krilla::Document::new();
    for page in doc.pages() {
        let settings = PageSettings::from_wh(page.size.width as f32, page.size.height as f32)
            .ok_or_else(|| IoError::Pdf("invalid page size".into()))?;
        let mut pdf_page = pdf.start_page_with(settings);
        let mut surface = pdf_page.surface();
        for leaf in page.objects.iter().flat_map(Object::leaves) {
            let Some(path) = pdf_path(&leaf) else { continue };
            let fill = leaf.fill().map(|c| Fill {
                paint: rgb::Color::new(c.r, c.g, c.b).into(),
                opacity: opacity(c.a),
                rule: FillRule::EvenOdd,
            });
            let outline = leaf.style.outline.as_ref();
            let stroke = outline.map(|o| {
                let width = o.page_width(leaf.transform);
                let dashes = o.dashes(width);
                Stroke {
                    paint: rgb::Color::new(o.color.r, o.color.g, o.color.b).into(),
                    width: width as f32,
                    miter_limit: Outline::MITER_LIMIT as f32,
                    line_cap: match o.cap {
                        LineCap::Butt => krilla::paint::LineCap::Butt,
                        LineCap::Round => krilla::paint::LineCap::Round,
                        LineCap::Square => krilla::paint::LineCap::Square,
                    },
                    line_join: match o.join {
                        LineJoin::Miter => krilla::paint::LineJoin::Miter,
                        LineJoin::Round => krilla::paint::LineJoin::Round,
                        LineJoin::Bevel => krilla::paint::LineJoin::Bevel,
                    },
                    opacity: opacity(o.color.a),
                    dash: (!dashes.is_empty()).then(|| StrokeDash { array: dashes.iter().map(|d| *d as f32).collect(), offset: 0.0 }),
                }
            });
            if fill.is_none() && stroke.is_none() {
                continue;
            }
            if outline.is_some_and(|o| o.behind_fill) && fill.is_some() {
                // Outline under the fill: two passes.
                surface.set_fill(None);
                surface.set_stroke(stroke);
                surface.draw_path(&path);
                surface.set_fill(fill);
                surface.set_stroke(None);
            } else {
                surface.set_fill(fill);
                surface.set_stroke(stroke);
            }
            surface.draw_path(&path);
        }
        surface.finish();
        pdf_page.finish();
    }
    pdf.finish().map_err(|e| IoError::Pdf(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vs_core::kurbo::{Affine, Rect};
    use vs_core::{Outline, Shape, Style};

    fn sample() -> Document {
        let mut doc = Document::new("Logo");
        let id = doc.add_object(
            Shape::Star { rect: Rect::new(100.0, 100.0, 300.0, 300.0), points: 5, inner_ratio: 0.5 },
            Style { fill: Some(Rgba::rgb(255, 0, 0)), outline: Some(Outline::new(Rgba::BLACK, 2.0)) },
        );
        doc.transform_objects(&[id], Affine::rotate_about(0.2, vs_core::kurbo::Point::new(200.0, 200.0)));
        doc.add_object(Shape::Ellipse { rect: Rect::new(10.0, 10.0, 60.0, 40.0), arc: None }, Style::new_object());
        doc
    }

    #[test]
    fn native_round_trip_and_new_ids_do_not_collide() {
        let doc = sample();
        let text = document_to_string(&doc).unwrap();
        let mut back = document_from_str(&text).unwrap();
        assert_eq!(back, doc);
        let new = back.add_object(Shape::Ellipse { rect: Rect::ZERO, arc: None }, Style::default());
        assert!(doc.active_page().objects.iter().all(|o| o.id != new));
    }

    #[test]
    fn rejects_other_json_and_newer_versions() {
        assert!(matches!(document_from_str(r#"{"format":"x","version":1,"document":null}"#), Err(IoError::Json(_))));
        let text = document_to_string(&sample()).unwrap();
        let other = text.replacen("\"vector-studio\"", "\"something\"", 1);
        assert!(matches!(document_from_str(&other), Err(IoError::NotOurFormat)));
        let newer = text.replacen("\"version\": 1", "\"version\": 99", 1);
        assert!(matches!(document_from_str(&newer), Err(IoError::NewerVersion(99))));
    }

    #[test]
    fn svg_has_one_path_per_object_and_parses() {
        let svg = page_to_svg(sample().active_page());
        assert_eq!(svg.matches("<path ").count(), 2);
        assert!(svg.contains(r##"fill="#ff0000" fill-rule="evenodd""##), "even-odd, like the canvas");
        assert!(svg.contains(r#"width="595.276pt""#), "{svg}");
        resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    }

    #[test]
    fn svg_writes_outline_style_and_groups() {
        let mut doc = sample();
        let ids = doc.all_object_ids();
        doc.restyle_objects(&ids, |s| {
            if let Some(o) = &mut s.outline {
                o.style = vs_core::LineStyle::Dash;
                o.join = LineJoin::Round;
                o.cap = LineCap::Square;
                o.behind_fill = true;
            }
        });
        doc.group_objects(&ids).unwrap();
        let svg = page_to_svg(doc.active_page());
        assert_eq!(svg.matches("<g>").count(), 1);
        assert_eq!(svg.matches("<path ").count(), 2);
        assert!(svg.contains(r#"stroke-dasharray="8 4""#), "{svg}");
        assert!(svg.contains(r#"stroke-linejoin="round""#) && svg.contains(r#"stroke-linecap="square""#));
        assert!(svg.contains(r#"paint-order="stroke""#));
        resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
        assert!(document_to_pdf(&doc).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn png_has_page_size_at_dpi_and_draws_the_fill() {
        let doc = sample();
        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: None }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (596, 842));
        let centre = pixmap.pixel(200, 200).unwrap();
        assert_eq!((centre.red(), centre.green(), centre.blue(), centre.alpha()), (255, 0, 0, 255));
        assert_eq!(pixmap.pixel(500, 700).unwrap().alpha(), 0, "background stays transparent");
    }

    #[test]
    fn pdf_has_a_page_per_document_page() {
        let mut doc = sample();
        doc.insert_page_after(0).unwrap();
        let pdf = document_to_pdf(&doc).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"), "page tree lists two pages");
    }

    #[test]
    fn export_picks_format_by_extension() {
        assert_eq!(ExportFormat::from_path(Path::new("a.SVG")).unwrap(), ExportFormat::Svg);
        assert!(matches!(ExportFormat::from_path(Path::new("a.cdr")), Err(IoError::UnknownExtension(_))));
    }
}

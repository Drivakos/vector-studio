//! File formats: the native document format, SVG / PNG / PDF export and SVG import.

mod bitmaps;
mod import;

pub use bitmaps::{apply_effect, bitmap_from_pixels, bitmap_pixels, drawn_bounds, glyph_image, objects_to_bitmap, resample, shrink, trim_transparent};
pub use import::{import_bitmap, import_svg};

use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vs_core::kurbo::{BezPath, PathEl};
use vs_core::geometry::Leaf;
use vs_core::{Document, Fill, Gradient, GradientKind, LineCap, LineJoin, Object, Outline, Page, Rgba, Shape};

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
    Svg(String),
    Image(String),
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
            IoError::Svg(e) => write!(f, "not a readable SVG file ({e})"),
            IoError::Image(e) => write!(f, "not a readable image ({e})"),
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
/// active page; PDF exports every page. A page background set to print and export
/// comes out under the objects (for PNG, in place of `options.background`).
pub fn export(doc: &Document, path: &Path, options: &ExportOptions) -> Result<ExportFormat, IoError> {
    let format = ExportFormat::from_path(path)?;
    let background = doc.exported_background();
    let bytes = match format {
        ExportFormat::Svg => svg_of_page(doc.active_page(), background, Some(&doc.properties)).into_bytes(),
        ExportFormat::Png => {
            let options = ExportOptions { background: background.or(options.background), ..*options };
            page_to_png(doc.active_page(), &options)?
        }
        ExportFormat::Pdf => document_to_pdf(doc)?,
    };
    std::fs::write(path, bytes)?;
    Ok(format)
}

/// One `<path>` per drawable object; groups become `<g>` around their members.
fn write_svg_object(out: &mut String, obj: &Object, parent: vs_core::kurbo::Affine, depth: usize) {
    if obj.perspective.is_some() {
        // SVG has no projective transform: write the projected curves.
        let mut baked = obj.clone();
        baked.bake_perspective();
        return write_svg_object(out, &baked, parent, depth);
    }
    let indent = "  ".repeat(depth);
    if let Shape::Image(b) = &obj.shape {
        write_svg_image(out, &indent, b, parent * obj.transform, obj.style.opacity());
        if obj.style.outline.is_none() {
            return;
        }
    }
    if let Shape::Group { children } = &obj.shape {
        let _ = writeln!(out, "{indent}<g>");
        for (copy, at) in obj.member_placements(parent * obj.transform).into_iter().enumerate() {
            for child in children {
                if copy == 0 {
                    write_svg_object(out, child, at, depth + 1);
                } else {
                    // Mirrored copies need their own gradient and clip ids.
                    let mut child = child.clone();
                    tag_ids(&mut child, copy as u64);
                    write_svg_object(out, &child, at, depth + 1);
                }
            }
        }
        let _ = writeln!(out, "{indent}</g>");
        return;
    }
    if !obj.contents.is_empty() {
        write_svg_powerclip(out, obj, parent, depth);
        return;
    }
    let mut path = obj.shape.to_path();
    let transform = parent * obj.transform;
    path.apply_affine(transform);
    round_path(&mut path);
    let opacity = obj.style.opacity();
    let fill = obj.style.fill.as_ref().filter(|_| obj.shape.is_closed() && !matches!(obj.shape, Shape::Image(_)));
    if let Some(Fill::Gradient(g)) = fill {
        write_svg_gradient(out, &indent, obj.id.0, g, transform, opacity);
    }
    let _ = write!(out, r#"{indent}<path d="{}""#, path.to_svg());
    match fill {
        Some(Fill::Solid(c)) => {
            let _ = write!(out, r#" fill="{}" fill-rule="evenodd""#, hex(*c));
            let a = f64::from(c.a) / 255.0 * opacity;
            if a < 1.0 {
                let _ = write!(out, r#" fill-opacity="{}""#, num(a));
            }
        }
        Some(Fill::Gradient(_)) => {
            let _ = write!(out, r#" fill="url(#fill{})" fill-rule="evenodd""#, obj.id.0);
        }
        None => out.push_str(r#" fill="none""#),
    }
    if let Some(o) = &obj.style.outline {
        write_svg_stroke(out, o, o.page_width(transform), opacity);
    }
    out.push_str("/>\n");
}

/// Give `obj` and everything in it ids that no object has, for copy `copy` of it.
fn tag_ids(obj: &mut Object, copy: u64) {
    obj.id.0 |= copy << 48;
    if let Shape::Group { children } = &mut obj.shape {
        children.iter_mut().for_each(|c| tag_ids(c, copy));
    }
    obj.contents.iter_mut().for_each(|c| tag_ids(c, copy));
}

/// A PowerClip: the container's fill, its contents clipped to its outline, then the
/// container's outline on top.
fn write_svg_powerclip(out: &mut String, obj: &Object, parent: vs_core::kurbo::Affine, depth: usize) {
    let indent = "  ".repeat(depth);
    let bare = |style: vs_core::Style| Object { style, contents: Vec::new(), ..obj.clone() };
    write_svg_object(out, &bare(vs_core::Style { outline: None, ..obj.style.clone() }), parent, depth);
    let transform = parent * obj.transform;
    let mut clip = obj.shape.to_path();
    clip.apply_affine(transform);
    round_path(&mut clip);
    let _ = writeln!(out, r#"{indent}<clipPath id="clip{}"><path d="{}" clip-rule="evenodd"/></clipPath>"#, obj.id.0, clip.to_svg());
    let _ = writeln!(out, r#"{indent}<g clip-path="url(#clip{})">"#, obj.id.0);
    for content in &obj.contents {
        write_svg_object(out, content, transform, depth + 1);
    }
    let _ = writeln!(out, "{indent}</g>");
    if obj.style.outline.is_some() {
        write_svg_object(out, &bare(vs_core::Style { fill: None, ..obj.style.clone() }), parent, depth);
    }
}

/// An `<image>` with the bitmap's own bytes inline.
fn write_svg_image(out: &mut String, indent: &str, b: &vs_core::Bitmap, transform: vs_core::kurbo::Affine, opacity: f64) {
    let [a, bb, c, d, e, f] = transform.as_coeffs();
    let _ = write!(
        out,
        r#"{indent}<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" transform="matrix({} {} {} {} {} {})""#,
        num(b.rect.x0), num(b.rect.y0), num(b.rect.width()), num(b.rect.height()),
        num(a), num(bb), num(c), num(d), num(e), num(f)
    );
    if opacity < 1.0 {
        let _ = write!(out, r#" opacity="{}""#, num(opacity));
    }
    let _ = writeln!(out, r#" href="data:{};base64,{}"/>"#, b.format.mime(), b.base64());
}

/// A `<linearGradient>` or `<radialGradient>` with id `fill{id}`. Its points are in the
/// object's local space, placed on the page by `gradientTransform`.
fn write_svg_gradient(out: &mut String, indent: &str, id: u64, g: &Gradient, transform: vs_core::kurbo::Affine, opacity: f64) {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    let matrix = [a, b, c, d, e, f].map(num).join(" ");
    match g.kind {
        GradientKind::Linear => {
            let _ = writeln!(
                out,
                r#"{indent}<linearGradient id="fill{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}" gradientTransform="matrix({matrix})">"#,
                num(g.start.x), num(g.start.y), num(g.end.x), num(g.end.y)
            );
        }
        GradientKind::Radial => {
            let _ = writeln!(
                out,
                r#"{indent}<radialGradient id="fill{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}" gradientTransform="matrix({matrix})">"#,
                num(g.start.x), num(g.start.y), num((g.end - g.start).hypot())
            );
        }
    }
    for stop in &g.stops {
        let _ = write!(out, r#"{indent}  <stop offset="{}" stop-color="{}""#, num(stop.offset), hex(stop.color));
        let a = f64::from(stop.color.a) / 255.0 * opacity;
        if a < 1.0 {
            let _ = write!(out, r#" stop-opacity="{}""#, num(a));
        }
        out.push_str("/>\n");
    }
    let tag = if g.kind == GradientKind::Linear { "linearGradient" } else { "radialGradient" };
    let _ = writeln!(out, "{indent}</{tag}>");
}

fn write_svg_stroke(out: &mut String, o: &Outline, width: f64, opacity: f64) {
    let _ = write!(out, r#" stroke="{}" stroke-width="{}" stroke-miterlimit="{}""#, hex(o.color), num(width), num(Outline::MITER_LIMIT));
    let a = f64::from(o.color.a) / 255.0 * opacity;
    if a < 1.0 {
        let _ = write!(out, r#" stroke-opacity="{}""#, num(a));
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
    svg_of_page(page, None, None)
}

/// [`page_to_svg`] with a rectangle of `background` under everything, and `properties`
/// as the title, description and Dublin Core metadata.
fn svg_of_page(page: &Page, background: Option<Rgba>, properties: Option<&vs_core::DocumentProperties>) -> String {
    let (w, h) = (page.size.width, page.size.height);
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{}pt" height="{}pt" viewBox="0 0 {} {}">"#,
        num(w), num(h), num(w), num(h)
    );
    if let Some(p) = properties.filter(|p| !p.is_empty()) {
        write_svg_metadata(&mut out, p);
    }
    if let Some(bg) = background {
        let _ = writeln!(out, r#"  <rect width="{}" height="{}" fill="{}"/>"#, num(w), num(h), hex(bg));
    }
    for obj in page.shown_objects() {
        write_svg_object(&mut out, obj, vs_core::kurbo::Affine::IDENTITY, 1);
    }
    out.push_str("</svg>\n");
    out
}

/// `<title>`, `<desc>` and an RDF block of Dublin Core terms for the filled-in properties.
fn write_svg_metadata(out: &mut String, p: &vs_core::DocumentProperties) {
    if !p.title.is_empty() {
        let _ = writeln!(out, "  <title>{}</title>", xml_escape(&p.title));
    }
    if !p.subject.is_empty() {
        let _ = writeln!(out, "  <desc>{}</desc>", xml_escape(&p.subject));
    }
    let keywords = p.keyword_list().join(", ");
    let terms = [("title", &p.title), ("description", &p.subject), ("creator", &p.author), ("rights", &p.copyright), ("subject", &keywords)];
    if terms.iter().all(|(_, v)| v.is_empty()) {
        return;
    }
    out.push_str("  <metadata>\n");
    out.push_str(r#"    <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:dc="http://purl.org/dc/elements/1.1/">"#);
    out.push_str("\n      <rdf:Description rdf:about=\"\">\n");
    for (term, value) in terms.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = writeln!(out, "        <dc:{term}>{}</dc:{term}>", xml_escape(value));
    }
    out.push_str("      </rdf:Description>\n    </rdf:RDF>\n  </metadata>\n");
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
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
    render_page(page, options)?.encode_png().map_err(|e| IoError::Png(e.to_string()))
}

/// A page drawn at `options.dpi`, on `options.background` or transparent.
fn render_page(page: &Page, options: &ExportOptions) -> Result<resvg::tiny_skia::Pixmap, IoError> {
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
    Ok(pixmap)
}

/// A page parsed once for the canvas's Normal view, which draws it without anti-aliasing.
pub struct AliasedPage {
    svg: String,
    tree: resvg::usvg::Tree,
    /// Page points per tree unit (the tree resolves the SVG's pt size to px).
    unit: f64,
}

impl AliasedPage {
    pub fn new(page: &Page) -> Result<Self, IoError> {
        let svg = Self::svg(page);
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).map_err(|e| IoError::Png(e.to_string()))?;
        let unit = page.size.width / f64::from(tree.size().width());
        Ok(Self { svg, tree, unit })
    }

    /// The page's SVG with edges and bitmaps drawn without smoothing.
    fn svg(page: &Page) -> String {
        page_to_svg(page).replacen("<svg ", r#"<svg shape-rendering="crispEdges" image-rendering="optimizeSpeed" "#, 1)
    }

    /// Whether this still shows `page` as it is.
    pub fn shows(&self, page: &Page) -> bool {
        self.svg == Self::svg(page)
    }

    /// The page's objects (no paper) in a `size` pixel image, `transform` mapping page
    /// points to pixels: straight-alpha RGBA bytes.
    pub fn render(&self, transform: vs_core::kurbo::Affine, size: [u32; 2]) -> Option<Vec<u8>> {
        use resvg::tiny_skia;
        let mut pixmap = tiny_skia::Pixmap::new(size[0], size[1])?;
        let transform = transform * vs_core::kurbo::Affine::scale(self.unit);
        let [a, b, c, d, e, f] = transform.as_coeffs().map(|v| v as f32);
        resvg::render(&self.tree, tiny_skia::Transform::from_row(a, b, c, d, e, f), &mut pixmap.as_mut());
        Some(pixmap.pixels().iter().flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        }).collect())
    }
}

/// A small picture of a page on `paper`, its longer side `longest` pixels: width,
/// height and premultiplied RGBA bytes.
pub fn page_thumbnail(page: &Page, paper: Rgba, longest: u32) -> Result<(u32, u32, Vec<u8>), IoError> {
    let side = page.size.width.max(page.size.height).max(1.0);
    // A hair under, so rounding up to whole pixels lands on `longest`, not one more.
    let dpi = (f64::from(longest.max(1)) - 1e-6) / side * 72.0;
    let pixmap = render_page(page, &ExportOptions { dpi, background: Some(paper) })?;
    Ok((pixmap.width(), pixmap.height(), pixmap.take()))
}

/// A PDF gradient paint for `g`, whose points are in the space `transform` maps to the page.
fn pdf_gradient(g: &Gradient, transform: vs_core::kurbo::Affine, alpha: f64) -> krilla::paint::Paint {
    use krilla::color::rgb;
    use krilla::num::NormalizedF32;
    use krilla::paint::{LinearGradient, RadialGradient, SpreadMethod, Stop};
    let [a, b, c, d, e, f] = transform.as_coeffs().map(|v| v as f32);
    let transform = krilla::geom::Transform::from_row(a, b, c, d, e, f);
    let stops: Vec<Stop> = g
        .stops
        .iter()
        .map(|s| Stop {
            offset: NormalizedF32::new(s.offset as f32).unwrap_or(NormalizedF32::ZERO),
            color: rgb::Color::new(s.color.r, s.color.g, s.color.b).into(),
            opacity: NormalizedF32::new(f32::from(s.color.faded(alpha).a) / 255.0).unwrap_or(NormalizedF32::ONE),
        })
        .collect();
    let (x1, y1, x2, y2) = (g.start.x as f32, g.start.y as f32, g.end.x as f32, g.end.y as f32);
    match g.kind {
        GradientKind::Linear => {
            LinearGradient { x1, y1, x2, y2, transform, spread_method: SpreadMethod::Pad, stops, anti_alias: false }.into()
        }
        GradientKind::Radial => RadialGradient {
            fx: x1,
            fy: y1,
            fr: 0.0,
            cx: x1,
            cy: y1,
            cr: (g.end - g.start).hypot() as f32,
            transform,
            spread_method: SpreadMethod::Pad,
            stops,
            anti_alias: false,
        }
        .into(),
    }
}

/// A page-space path as a PDF path.
fn pdf_path(path: &BezPath) -> Option<krilla::geom::Path> {
    let mut pb = krilla::geom::PathBuilder::new();
    let f = |p: vs_core::kurbo::Point| (p.x as f32, p.y as f32);
    for el in path.elements() {
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

/// Draw `b` on a PDF page; `transform` is its object's local → page transform.
fn pdf_image(surface: &mut krilla::surface::Surface<'_>, b: &vs_core::Bitmap, transform: vs_core::kurbo::Affine, opacity: f64) -> Result<(), IoError> {
    use krilla::num::NormalizedF32;
    let data: krilla::Data = b.data.clone().into();
    let image = match b.format {
        vs_core::BitmapFormat::Png => krilla::image::Image::from_png(data, true),
        vs_core::BitmapFormat::Jpeg => krilla::image::Image::from_jpeg(data, true),
    }
    .map_err(IoError::Pdf)?;
    let Some(size) = krilla::geom::Size::from_wh(b.rect.width() as f32, b.rect.height() as f32) else { return Ok(()) };
    let [a, bb, c, d, e, f] = (transform * vs_core::kurbo::Affine::translate(b.rect.origin().to_vec2())).as_coeffs().map(|v| v as f32);
    surface.push_transform(&krilla::geom::Transform::from_row(a, bb, c, d, e, f));
    let faded = opacity < 1.0;
    if faded {
        surface.push_opacity(NormalizedF32::new(opacity as f32).unwrap_or(NormalizedF32::ONE));
    }
    surface.draw_image(image, size);
    if faded {
        surface.pop();
    }
    surface.pop();
    Ok(())
}

/// Every page of the document as a PDF, 1 unit = 1 pt.
pub fn document_to_pdf(doc: &Document) -> Result<Vec<u8>, IoError> {
    let all: Vec<usize> = (0..doc.page_count()).collect();
    pages_to_pdf(doc, &all)
}

/// The pages at `indices` (0-based, in that order) as a PDF. Indices past the last page
/// are skipped.
pub fn pages_to_pdf(doc: &Document, indices: &[usize]) -> Result<Vec<u8>, IoError> {
    pages_to_print_pdf(doc, indices, &PrintMarks::default())
}

/// What a print-ready PDF adds around each page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrintMarks {
    /// How far the artwork runs past the page edge, in points.
    pub bleed: f64,
    /// Lines outside the corners showing where to trim.
    pub crop_marks: bool,
}

impl PrintMarks {
    /// Gap between the bleed and the start of a crop mark, in points.
    pub const MARK_GAP: f64 = 6.0;
    /// Length of a crop mark, in points.
    pub const MARK_LENGTH: f64 = 18.0;

    /// Space added on every side of the page.
    pub fn margin(&self) -> f64 {
        self.bleed.max(0.0) + if self.crop_marks { Self::MARK_GAP + Self::MARK_LENGTH } else { 0.0 }
    }
}

/// Like [`pages_to_pdf`], with each sheet grown by `marks`: the artwork runs `bleed` past
/// the page edge (the bleed box), the page itself is the trim box, and crop marks sit
/// outside the bleed at the corners.
pub fn pages_to_print_pdf(doc: &Document, indices: &[usize], marks: &PrintMarks) -> Result<Vec<u8>, IoError> {
    use krilla::page::PageSettings;
    use krilla::paint::FillRule;
    use vs_core::kurbo::{Affine, Rect, Shape as _};

    let bleed = marks.bleed.max(0.0);
    let m = marks.margin();
    let mut pdf = krilla::Document::new();
    pdf.set_metadata(pdf_metadata(&doc.properties));
    for page in indices.iter().filter_map(|&i| doc.pages().get(i)) {
        let (w, h) = (page.size.width, page.size.height);
        let pdf_rect = |r: Rect| krilla::geom::Rect::from_ltrb(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32);
        let mut settings = PageSettings::from_wh((w + 2.0 * m) as f32, (h + 2.0 * m) as f32)
            .ok_or_else(|| IoError::Pdf("invalid page size".into()))?;
        if m > 0.0 {
            settings = settings
                .with_trim_box(pdf_rect(Rect::new(m, m, m + w, m + h)))
                .with_bleed_box(pdf_rect(Rect::new(m - bleed, m - bleed, m + w + bleed, m + h + bleed)));
        }
        let mut pdf_page = pdf.start_page_with(settings);
        let mut surface = pdf_page.surface();
        if marks.crop_marks {
            draw_crop_marks(&mut surface, w, h, m, bleed);
        }
        let [a, b, c, d, e, f] = Affine::translate((m, m)).as_coeffs().map(|v| v as f32);
        surface.push_transform(&krilla::geom::Transform::from_row(a, b, c, d, e, f));
        // Artwork shows up to the bleed and no further.
        let sheet = Rect::new(-bleed, -bleed, w + bleed, h + bleed).to_path(0.1);
        let clipped = m > 0.0 && pdf_path(&sheet).is_some();
        if clipped && let Some(clip) = pdf_path(&sheet) {
            surface.push_clip_path(&clip, &FillRule::NonZero);
        }
        if let Some(bg) = doc.exported_background() {
            let paper = sheet.clone();
            if let Some(path) = pdf_path(&paper) {
                surface.set_fill(Some(krilla::paint::Fill { paint: krilla::color::rgb::Color::new(bg.r, bg.g, bg.b).into(), ..Default::default() }));
                surface.set_stroke(None);
                surface.draw_path(&path);
            }
        }
        for leaf in page.shown_objects().flat_map(Object::leaves) {
            for clip in leaf.clips.iter().filter_map(pdf_path) {
                surface.push_clip_path(&clip, &FillRule::EvenOdd);
            }
            pdf_leaf(&mut surface, &leaf)?;
            for _ in leaf.clips.iter().filter_map(pdf_path) {
                surface.pop();
            }
        }
        if clipped {
            surface.pop();
        }
        surface.pop();
        surface.finish();
        pdf_page.finish();
    }
    pdf.finish().map_err(|e| IoError::Pdf(format!("{e:?}")))
}

/// Crop marks for a `w` × `h` page placed `m` in from the sheet's edges: two short lines
/// at each corner, in line with the page edges, starting outside the bleed.
fn draw_crop_marks(surface: &mut krilla::surface::Surface<'_>, w: f64, h: f64, m: f64, bleed: f64) {
    use vs_core::kurbo::{BezPath, Point};
    let start = bleed + PrintMarks::MARK_GAP;
    let end = start + PrintMarks::MARK_LENGTH;
    let mut path = BezPath::new();
    for (x, sx) in [(m, -1.0), (m + w, 1.0)] {
        for (y, sy) in [(m, -1.0), (m + h, 1.0)] {
            path.move_to(Point::new(x + sx * start, y));
            path.line_to(Point::new(x + sx * end, y));
            path.move_to(Point::new(x, y + sy * start));
            path.line_to(Point::new(x, y + sy * end));
        }
    }
    let Some(path) = pdf_path(&path) else { return };
    surface.set_fill(None);
    surface.set_stroke(Some(krilla::paint::Stroke {
        paint: krilla::color::rgb::Color::new(0, 0, 0).into(),
        width: 0.25,
        ..Default::default()
    }));
    surface.draw_path(&path);
}

/// The PDF's document information from the drawing's properties.
fn pdf_metadata(p: &vs_core::DocumentProperties) -> krilla::metadata::Metadata {
    let mut m = krilla::metadata::Metadata::new().creator("Vector Studio".to_owned());
    let set = |v: &str| (!v.is_empty()).then(|| v.to_owned());
    if let Some(v) = set(&p.title) {
        m = m.title(v);
    }
    if let Some(v) = set(&p.subject) {
        m = m.description(v);
    }
    if let Some(v) = set(&p.author) {
        m = m.authors(vec![v]);
    }
    let keywords = p.keyword_list();
    if !keywords.is_empty() {
        m = m.keywords(keywords);
    }
    m
}

/// Draw one leaf on a PDF page.
fn pdf_leaf(surface: &mut krilla::surface::Surface<'_>, leaf: &Leaf<'_>) -> Result<(), IoError> {
    use krilla::color::rgb;
    use krilla::num::NormalizedF32;
    use krilla::paint::{Fill, FillRule, Stroke, StrokeDash};

    let opacity = |a: u8| NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE);
    if let Shape::Image(b) = leaf.shape {
        pdf_image(surface, b, leaf.transform, leaf.style.opacity())?;
    }
    let Some(path) = pdf_path(&leaf.path) else { return Ok(()) };
    let alpha = leaf.style.opacity();
    let fill = leaf.fill().map(|f| match f {
        vs_core::Fill::Solid(c) => Fill {
            paint: rgb::Color::new(c.r, c.g, c.b).into(),
            opacity: opacity(c.faded(alpha).a),
            rule: FillRule::EvenOdd,
        },
        vs_core::Fill::Gradient(g) => Fill { paint: pdf_gradient(g, leaf.transform, alpha), opacity: NormalizedF32::ONE, rule: FillRule::EvenOdd },
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
            opacity: opacity(o.color.faded(alpha).a),
            dash: (!dashes.is_empty()).then(|| StrokeDash { array: dashes.iter().map(|d| *d as f32).collect(), offset: 0.0 }),
        }
    });
    if fill.is_none() && stroke.is_none() {
        return Ok(());
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
    Ok(())
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
            Style { fill: Some(Rgba::rgb(255, 0, 0).into()), outline: Some(Outline::new(Rgba::BLACK, 2.0)), ..Style::default() },
        );
        doc.transform_objects(&[id], Affine::rotate_about(0.2, vs_core::kurbo::Point::new(200.0, 200.0)));
        doc.add_object(Shape::Ellipse { rect: Rect::new(10.0, 10.0, 60.0, 40.0), arc: None }, Style::new_object());
        doc
    }

    #[test]
    fn bitmaps_save_open_and_export_to_svg_png_and_pdf() {
        let mut doc = Document::new("Photo");
        let mut obj = import_bitmap(&crate::import::tests::png(4, 4, [0, 0, 255])).unwrap();
        obj.transform = Affine::translate((100.0, 100.0)) * Affine::scale(10.0);
        doc.paste_objects(&[obj], vs_core::kurbo::Vec2::ZERO);

        let reopened = document_from_str(&document_to_string(&doc).unwrap()).unwrap();
        assert_eq!(reopened.active_page().objects[0].shape, doc.active_page().objects[0].shape, "bytes survive save and open");

        let svg = page_to_svg(doc.active_page());
        assert!(svg.contains("<image ") && svg.contains("href=\"data:image/png;base64,"), "{svg}");

        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: Some(Rgba::WHITE) }).unwrap();
        let img = image::load_from_memory(&png).unwrap().into_rgba8();
        assert_eq!(img.get_pixel(115, 115).0, [0, 0, 255, 255], "the bitmap is drawn: 3 pt × 10 = 30 pt square at 100 pt");
        assert_eq!(img.get_pixel(140, 140).0, [255, 255, 255, 255], "and only there");

        let pdf = document_to_pdf(&doc).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Subtype/Image") && text.contains("/Width 4/Height 4"), "the PDF holds the 4 × 4 image");
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
    fn layers_survive_save_and_hidden_layers_are_not_exported() {
        let mut doc = sample();
        let page = doc.active_page_mut();
        let top = page.add_layer();
        page.rename_layer(top, "Notes").unwrap();
        doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 10.0), corner_radius: 0.0 }, Style::default());
        let reopened = document_from_str(&document_to_string(&doc).unwrap()).unwrap();
        assert_eq!(reopened.active_page().layers(), doc.active_page().layers());
        assert_eq!(reopened.active_page().objects, doc.active_page().objects);
        assert_eq!(page_to_svg(doc.active_page()).matches("<path ").count(), 3);
        doc.active_page_mut().set_layer_visible(top, false).unwrap();
        assert_eq!(page_to_svg(doc.active_page()).matches("<path ").count(), 2, "the hidden layer is left out");
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
    fn powerclips_clip_in_svg_png_and_pdf_and_survive_save() {
        let mut doc = Document::new("Clip");
        let red = Style::filled(vs_core::Rgba::rgb(255, 0, 0));
        let frame = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 100.0, 100.0), corner_radius: 0.0 }, Style::outlined(Outline::new(vs_core::Rgba::BLACK, 1.0)));
        let bar = doc.add_object(Shape::Rectangle { rect: Rect::new(50.0, 40.0, 200.0, 60.0), corner_radius: 0.0 }, red);
        assert!(doc.place_inside(&[bar], frame));
        let svg = page_to_svg(doc.active_page());
        assert!(svg.contains(&format!(r#"<clipPath id="clip{}">"#, frame.0)), "{svg}");
        assert!(svg.find("clip-path=").unwrap() < svg.rfind("stroke=").unwrap(), "the outline is drawn over the contents");
        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: None }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert_eq!(pixmap.pixel(75, 50).unwrap().red(), 255, "inside");
        assert_eq!(pixmap.pixel(150, 50).unwrap().alpha(), 0, "clipped away outside");
        let pdf = document_to_pdf(&doc).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let reopened = document_from_str(&document_to_string(&doc).unwrap()).unwrap();
        assert_eq!(reopened.active_page().objects, doc.active_page().objects);
    }

    #[test]
    fn symmetry_copies_export_with_their_own_gradient_ids() {
        use vs_core::{Fill, Gradient, GradientKind};
        let mut doc = Document::new("Mirror");
        let rect = Rect::new(0.0, 0.0, 40.0, 40.0);
        let mut style = Style::filled(Rgba::rgb(255, 0, 0));
        style.fill = Some(Fill::Gradient(Gradient::across(GradientKind::Linear, rect, Rgba::rgb(255, 0, 0), Rgba::rgb(255, 0, 0))));
        let a = doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, style);
        let group = doc.create_symmetry(&[a]).unwrap();
        // Mirror line at x = 60: the copy covers 80..120.
        doc.active_page_mut().object_mut(group).unwrap().symmetry.as_mut().unwrap().center.x = 60.0;
        let svg = page_to_svg(doc.active_page());
        assert_eq!(svg.matches("<path ").count(), 2, "{svg}");
        assert_eq!(svg.matches("<linearGradient id=").count(), 2);
        assert_ne!(svg.find(&format!(r#"id="fill{}""#, a.0)), svg.rfind("<linearGradient id="), "the copy has its own id");
        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: None }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert_eq!(pixmap.pixel(100, 20).unwrap().red(), 255, "the mirrored copy is drawn");
        assert_eq!(pixmap.pixel(60, 20).unwrap().alpha(), 0);
    }

    #[test]
    fn perspective_exports_projected_and_survives_save() {
        use vs_core::kurbo::Point;
        let mut doc = Document::new("Angle");
        let id = doc.add_object(Shape::Text(vs_core::Text::new("Hi")), Style::filled(Rgba::rgb(255, 0, 0)));
        let rect = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 100.0, 100.0, 200.0), corner_radius: 0.0 }, Style::filled(Rgba::rgb(255, 0, 0)));
        doc.add_perspective(&[rect]);
        // Narrow at the top: 40..60.
        assert!(doc.set_perspective_corner(rect, 0, Point::new(40.0, 100.0)));
        assert!(doc.set_perspective_corner(rect, 1, Point::new(60.0, 100.0)));
        doc.add_perspective(&[id]);
        assert_eq!(page_to_svg(doc.active_page()).matches("<path ").count(), 2, "text with a perspective is written as curves");
        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: None }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert_eq!(pixmap.pixel(50, 102).unwrap().red(), 255, "the narrow top");
        assert_eq!(pixmap.pixel(10, 102).unwrap().alpha(), 0, "cut away by the perspective");
        assert_eq!(pixmap.pixel(5, 197).unwrap().red(), 255, "the full-width bottom");
        assert!(document_to_pdf(&doc).unwrap().starts_with(b"%PDF"));
        let reopened = document_from_str(&document_to_string(&doc).unwrap()).unwrap();
        assert_eq!(reopened.active_page().objects, doc.active_page().objects);
    }

    #[test]
    fn fountain_fills_and_transparency_export() {
        use vs_core::{Fill, Gradient, GradientKind};
        let mut doc = Document::new("Fills");
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let mut style = Style::filled(Rgba::rgb(255, 0, 0));
        style.fill = Some(Fill::Gradient(Gradient::across(GradientKind::Linear, rect, Rgba::rgb(255, 0, 0), Rgba::WHITE)));
        let id = doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, style);
        doc.transform_objects(&[id], Affine::translate((100.0, 100.0)));
        let mut faded = Style::filled(Rgba::rgb(0, 0, 255));
        faded.transparency = 0.5;
        doc.add_object(Shape::Rectangle { rect: Rect::new(300.0, 100.0, 400.0, 150.0), corner_radius: 0.0 }, faded);

        let svg = page_to_svg(doc.active_page());
        assert!(svg.contains(r#"<linearGradient id="fill"#), "{svg}");
        assert!(svg.contains(r#"gradientTransform="matrix(1 0 0 1 100 100)""#), "{svg}");
        assert!(svg.contains(r#"fill="url(#fill"#));
        assert!(svg.contains(r#"fill-opacity="0.5""#), "{svg}");

        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: Some(Rgba::WHITE) }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        let px = |x, y| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        assert!(px(102, 125).1 < 20, "red at the start: {:?}", px(102, 125));
        assert!(px(198, 125).1 > 235, "white at the end: {:?}", px(198, 125));
        let half = px(350, 125);
        assert!(half.0.abs_diff(127) < 4 && half.2 == 255, "half see-through blue: {half:?}");
        assert!(document_to_pdf(&doc).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn text_exports_as_glyph_outlines() {
        let mut doc = Document::new("Text");
        let id = doc.add_object(Shape::Text(vs_core::Text::new("Hi")), Style::new_text());
        doc.transform_objects(&[id], Affine::translate((100.0, 100.0)));
        let svg = page_to_svg(doc.active_page());
        assert_eq!(svg.matches("<path ").count(), 1);
        assert!(svg.contains(r##"fill="#000000""##), "{svg}");
        let png = page_to_png(doc.active_page(), &ExportOptions { dpi: 72.0, background: None }).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        // "Hi" sits just right of the origin, above the baseline.
        let inked = (100..130).any(|x| (84..99).any(|y| pixmap.pixel(x, y).unwrap().alpha() > 128));
        assert!(inked, "the glyphs are drawn");
        assert!(document_to_pdf(&doc).unwrap().starts_with(b"%PDF-"));
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
    fn a_page_background_set_to_export_comes_out_under_the_objects() {
        let dir = std::env::temp_dir().join(format!("vs-io-background-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut doc = sample();
        let plain_pdf = document_to_pdf(&doc).unwrap();
        let corner = |doc: &Document| {
            let path = dir.join("page.png");
            export(doc, &path, &ExportOptions { dpi: 72.0, background: None }).unwrap();
            let pixmap = resvg::tiny_skia::Pixmap::decode_png(&std::fs::read(&path).unwrap()).unwrap();
            let p = pixmap.pixel(500, 700).unwrap();
            (p.red(), p.green(), p.blue(), p.alpha())
        };
        doc.background = Some(vs_core::Background { color: Rgba::rgb(0, 128, 255), print_and_export: false });
        assert_eq!(corner(&doc).3, 0, "screen only: still transparent");
        assert_eq!(document_to_pdf(&doc).unwrap(), plain_pdf);

        doc.background = Some(vs_core::Background { color: Rgba::rgb(0, 128, 255), print_and_export: true });
        assert_eq!(corner(&doc), (0, 128, 255, 255));
        let svg_path = dir.join("page.svg");
        export(&doc, &svg_path, &ExportOptions::default()).unwrap();
        let svg = std::fs::read_to_string(&svg_path).unwrap();
        let rect = svg.find(r##"<rect width="595.276" height="841.89" fill="#0080ff"/>"##).expect("background rect");
        assert!(rect < svg.find("<path").unwrap(), "under the objects");
        assert!(!page_to_svg(doc.active_page()).contains("<rect"), "the clipboard copy has no background");
        assert_ne!(document_to_pdf(&doc).unwrap(), plain_pdf, "the PDF pages are painted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn properties_become_svg_and_pdf_metadata_and_survive_save() {
        let mut doc = sample();
        assert!(!svg_of_page(doc.active_page(), None, Some(&doc.properties)).contains("<metadata"), "nothing to say");
        doc.properties = vs_core::DocumentProperties {
            title: "Logo <draft>".into(),
            subject: "Shop sign".into(),
            author: "A. Person".into(),
            copyright: "CC BY 4.0".into(),
            keywords: "red, star".into(),
            notes: "private".into(),
        };
        let svg = svg_of_page(doc.active_page(), None, Some(&doc.properties));
        assert!(svg.contains("<title>Logo &lt;draft&gt;</title>"), "{svg}");
        assert!(svg.contains("<desc>Shop sign</desc>"));
        for term in ["<dc:creator>A. Person</dc:creator>", "<dc:rights>CC BY 4.0</dc:rights>", "<dc:subject>red, star</dc:subject>"] {
            assert!(svg.contains(term), "{term} in {svg}");
        }
        assert!(!svg.contains("private"), "notes stay in the drawing");
        resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).expect("still valid SVG");
        assert!(!page_to_svg(doc.active_page()).contains("<title>"), "not on the clipboard");

        let pdf = String::from_utf8_lossy(&document_to_pdf(&doc).unwrap()).into_owned();
        for entry in ["/Title(Logo <draft>)", "/Subject(Shop sign)", "/Keywords(red, star)", "/Author(A. Person)"] {
            assert!(pdf.contains(entry), "{entry} in the PDF info");
        }
        let reopened = document_from_str(&document_to_string(&doc).unwrap()).unwrap();
        assert_eq!(reopened.properties, doc.properties);
    }

    #[test]
    fn print_ready_pdf_adds_bleed_crop_marks_and_page_boxes() {
        let doc = sample();
        let plain = String::from_utf8_lossy(&pages_to_pdf(&doc, &[0]).unwrap()).into_owned();
        assert!(!plain.contains("/TrimBox"), "a plain PDF has no print boxes");
        let marks = PrintMarks { bleed: 9.0, crop_marks: true };
        assert_eq!(marks.margin(), 33.0);
        let pdf = String::from_utf8_lossy(&pages_to_print_pdf(&doc, &[0], &marks).unwrap()).into_owned();
        assert!(pdf.contains("/MediaBox[0 0 661.2756 907.8898]"), "the sheet grows by the margin on every side");
        assert!(pdf.contains("/TrimBox[33 33 628.2756 874.8898]"), "the page is the trim box");
        assert!(pdf.contains("/BleedBox[24 24 637.2756 883.8898]"), "the bleed box");
        let bleed_only = String::from_utf8_lossy(&pages_to_print_pdf(&doc, &[0], &PrintMarks { bleed: 9.0, crop_marks: false }).unwrap()).into_owned();
        assert!(bleed_only.contains("/MediaBox[0 0 613.2756 859.8898]"));
    }

    #[test]
    fn pages_to_pdf_takes_the_pages_asked_for() {
        let mut doc = sample();
        doc.insert_page_after(0).unwrap();
        doc.insert_page_after(1).unwrap();
        let count = |pdf: Vec<u8>| String::from_utf8_lossy(&pdf).contains("/Count 2");
        assert!(count(pages_to_pdf(&doc, &[0, 2]).unwrap()));
        assert!(count(pages_to_pdf(&doc, &[1, 2, 9]).unwrap()), "pages that do not exist are skipped");
    }

    #[test]
    fn thumbnails_fit_the_longer_side_and_show_the_page() {
        let doc = sample();
        let (w, h, rgba) = page_thumbnail(doc.active_page(), Rgba::WHITE, 120).unwrap();
        assert_eq!(h, 120, "A4 is taller than wide");
        assert_eq!(w, 85);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        let at = |x: u32, y: u32| &rgba[((y * w + x) * 4) as usize..][..4];
        assert_eq!(at(80, 115), &[255, 255, 255, 255], "paper");
        assert!(at(28, 28)[0] > 200 && at(28, 28)[1] < 80, "the red star: {:?}", at(28, 28));
    }

    #[test]
    fn export_picks_format_by_extension() {
        assert_eq!(ExportFormat::from_path(Path::new("a.SVG")).unwrap(), ExportFormat::Svg);
        assert!(matches!(ExportFormat::from_path(Path::new("a.cdr")), Err(IoError::UnknownExtension(_))));
    }

    #[test]
    fn aliased_page_has_hard_edges_where_the_export_smooths_them() {
        let doc = sample();
        let page = doc.active_page();
        let aliased = AliasedPage::new(page).unwrap();
        assert!(aliased.shows(page));
        // Page point (100, 100) at pixel (0, 0), two pixels per point.
        let size = [400, 400];
        let px = aliased.render(Affine::scale(2.0) * Affine::translate((-100.0, -100.0)), size).unwrap();
        assert_eq!(px.len(), 400 * 400 * 4);
        let alphas: std::collections::BTreeSet<u8> = px.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        assert_eq!(alphas, [0, 255].into(), "every pixel covered or not");
        let reds = px.as_chunks::<4>().0.iter().filter(|p| p[..3] == [255, 0, 0] && p[3] == 255).count();
        assert!(reds > 10_000, "the star's fill is there: {reds}");

        let smooth = render_page(page, &ExportOptions { dpi: 144.0, background: None }).unwrap();
        assert!(smooth.pixels().iter().any(|p| p.alpha() > 0 && p.alpha() < 255), "the export is anti-aliased");

        let mut moved = doc.clone();
        let id = moved.active_page().objects[0].id;
        moved.move_objects(&[id], vs_core::kurbo::Vec2::new(5.0, 0.0));
        assert!(!aliased.shows(moved.active_page()), "a change needs a new parse");
    }
}

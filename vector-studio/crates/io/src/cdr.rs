//! CorelDRAW (`.cdr`) and Corel Presentation Exchange (`.cmx`) files, read through libcdr
//! (see `build.rs` and `cdr/shim.cpp`): libcdr draws each page as SVG, and the SVG import
//! turns that into objects.

use std::borrow::Cow;
use std::path::Path;

use base64::Engine as _;
use vs_core::kurbo::{Size, Vec2};
use vs_core::{Document, Object, Shape};

use crate::IoError;

/// Extensions of the Corel files the app reads.
pub const CDR_EXTENSIONS: &[&str] = &["cdr", "cmx"];

/// Whether this build reads Corel files: it was built with libcdr.
pub const CAN_READ_CDR: bool = cfg!(cdr);

/// Whether `path` names a Corel file, by its extension.
pub fn is_cdr_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| CDR_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// A page of a Corel file: its size in points and its objects in page space.
#[derive(Clone, Debug)]
pub struct CdrPage {
    pub size: Size,
    pub objects: Vec<Object>,
}

/// Read a Corel file's pages. Shapes, groups, solid, fountain and pattern fills, outlines
/// and bitmaps come through as the SVG import keeps them; text comes as curves.
pub fn read_cdr(data: &[u8]) -> Result<Vec<CdrPage>, IoError> {
    let pages = svg_pages(data)?;
    if pages.is_empty() {
        return Err(IoError::Cdr("the file has no pages".into()));
    }
    pages
        .iter()
        .map(|svg| {
            let (size, mut objects) = crate::import::svg_drawing(bmp_to_png(svg).as_bytes())?;
            // libcdr wraps a page's objects in a group for each layer; a page that is one
            // group is taken apart, so its objects can be picked one by one.
            while let [Object { shape: Shape::Group { children }, .. }] = objects.as_mut_slice() {
                objects = std::mem::take(children);
            }
            Ok(CdrPage { size, objects })
        })
        .collect()
}

/// A new document titled `title` with a page for each page of the Corel file `data`, at
/// its size, the first page active.
pub fn cdr_document(data: &[u8], title: &str) -> Result<Document, IoError> {
    let pages = read_cdr(data)?;
    let mut doc = Document::new(title);
    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            doc.insert_page_after(i - 1).map_err(|e| IoError::Cdr(e.to_string()))?;
        }
        doc.active_page_mut().size = page.size;
        doc.paste_objects(&page.objects, Vec2::ZERO);
    }
    doc.set_active_page(0).map_err(|e| IoError::Cdr(e.to_string()))?;
    Ok(doc)
}

/// Open the Corel file at `path` as a new document named after the file.
pub fn open_cdr(path: &Path) -> Result<Document, IoError> {
    let data = std::fs::read(path)?;
    let title = path.file_stem().map_or_else(|| "Untitled".into(), |s| s.to_string_lossy());
    cdr_document(&data, &title)
}

#[cfg(cdr)]
fn svg_pages(data: &[u8]) -> Result<Vec<String>, IoError> {
    use std::ffi::{c_char, c_int, c_void};

    type PageFn = extern "C" fn(ctx: *mut c_void, svg: *const c_char, len: usize);
    unsafe extern "C" {
        fn vs_cdr_to_svg(data: *const u8, len: usize, page: PageFn, ctx: *mut c_void) -> c_int;
    }
    extern "C" fn page(ctx: *mut c_void, svg: *const c_char, len: usize) {
        // SAFETY: `ctx` is the `pages` vector below, alive for the whole call, and libcdr
        // passes `len` readable bytes at `svg`.
        let (pages, bytes) = unsafe { (&mut *ctx.cast::<Vec<String>>(), std::slice::from_raw_parts(svg.cast::<u8>(), len)) };
        pages.push(String::from_utf8_lossy(bytes).into_owned());
    }

    let mut pages: Vec<String> = Vec::new();
    // SAFETY: `data` is valid for `data.len()` bytes; the shim catches every C++ exception.
    let read = unsafe { vs_cdr_to_svg(data.as_ptr(), data.len(), page, (&raw mut pages).cast()) };
    match read {
        0 => Ok(pages),
        1 => Err(IoError::Cdr("not a CorelDRAW or CMX file".into())),
        _ => Err(IoError::Cdr("the file is damaged or could not be read".into())),
    }
}

#[cfg(not(cdr))]
fn svg_pages(_data: &[u8]) -> Result<Vec<String>, IoError> {
    Err(IoError::Cdr("this copy of the app was built without libcdr, so it cannot read CorelDRAW files".into()))
}

/// libcdr writes bitmaps as BMP, which the SVG import leaves out: make them PNG.
fn bmp_to_png(svg: &str) -> Cow<'_, str> {
    const BMP: &str = "data:image/bmp;base64,";
    if !svg.contains(BMP) {
        return Cow::Borrowed(svg);
    }
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find(BMP) {
        out.push_str(&rest[..at]);
        let tail = &rest[at + BMP.len()..];
        let end = tail.find(['"', '\'']).unwrap_or(tail.len());
        match png_from_bmp(&tail[..end]) {
            Some(png) => {
                out.push_str("data:image/png;base64,");
                out.push_str(&png);
            }
            None => {
                out.push_str(BMP);
                out.push_str(&tail[..end]);
            }
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn png_from_bmp(base64: &str) -> Option<String> {
    let engine = base64::engine::general_purpose::STANDARD;
    let bmp = engine.decode(base64.trim()).ok()?;
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
    let mut png = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(engine.encode(png))
}

/// Writes small CorelDRAW files for tests: CorelDRAW X3 (version 13) RIFF files with
/// pages of filled rectangles, laid out as libcdr reads them.
#[doc(hidden)]
pub mod testing {
    /// CDR coordinates are 1/254000 inch.
    const UNITS_PER_INCH: f64 = 254_000.0;

    /// A filled rectangle `width` × `height` inches whose bottom-left corner is (`x`, `y`)
    /// inches from the page centre, y up, filled with `rgb` (0xRRGGBB).
    #[derive(Clone, Copy, Debug)]
    pub struct Rect {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
        pub rgb: u32,
    }

    /// A page `width` × `height` inches.
    #[derive(Clone, Debug)]
    pub struct Page {
        pub width: f64,
        pub height: f64,
        /// Top first, as CorelDRAW stores them.
        pub rects: Vec<Rect>,
    }

    fn units(inches: f64) -> [u8; 4] {
        ((inches * UNITS_PER_INCH).round() as i32).to_le_bytes()
    }

    fn record(four_cc: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = [four_cc.as_slice(), &(data.len() as u32).to_le_bytes(), data].concat();
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8; 4], list_type: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
        let body = [list_type.as_slice(), &children.concat()].concat();
        [kind.as_slice(), &(body.len() as u32).to_le_bytes(), &body].concat()
    }

    /// The argument block of a `loda` (with types) or `trfd` (without) record.
    fn arguments(chunk_type: Option<u32>, args: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let header = if chunk_type.is_some() { 20 } else { 12 };
        let mut offsets = Vec::new();
        let mut body = Vec::new();
        for (_, arg) in args {
            offsets.push((header + body.len()) as u32);
            body.extend_from_slice(arg);
        }
        let start_offsets = (header + body.len()) as u32;
        let start_types = start_offsets + 4 * args.len() as u32;
        let end = if chunk_type.is_some() { start_types + 4 * args.len() as u32 } else { start_types };
        let mut out = [end, args.len() as u32, start_offsets].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>();
        if let Some(t) = chunk_type {
            out.extend([start_types, t].iter().flat_map(|v| v.to_le_bytes()));
        }
        out.extend(body);
        out.extend(offsets.iter().flat_map(|o| o.to_le_bytes()));
        if chunk_type.is_some() {
            // The types are stored last to first.
            out.extend(args.iter().rev().flat_map(|(t, _)| t.to_le_bytes()));
        }
        out
    }

    fn fill(id: u32, rgb: u32) -> Vec<u8> {
        let color = [5u16.to_le_bytes().as_slice(), &[0; 6], &rgb.to_le_bytes()].concat();
        record(b"fild", &[id.to_le_bytes().as_slice(), &[0; 8], &1u16.to_le_bytes(), &[0; 13], &color].concat())
    }

    fn rectangle(r: &Rect, fill_id: u32) -> Vec<u8> {
        let corners = [units(r.width), units(r.height), [0; 4], [0; 4], [0; 4], [0; 4]].concat();
        let shape = record(b"loda", &arguments(Some(1), &[(0x1e, corners), (0x14, fill_id.to_le_bytes().to_vec())]));
        let matrix: Vec<u8> = [1.0, 0.0, r.x * UNITS_PER_INCH, 0.0, 1.0, r.y * UNITS_PER_INCH].iter().flat_map(|v: &f64| v.to_le_bytes()).collect();
        let transform = [[0; 8].as_slice(), &8u16.to_le_bytes(), &[0; 6], &matrix].concat();
        list(b"LIST", b"obj ", &[shape, record(b"trfd", &arguments(None, &[(0, transform)]))])
    }

    /// A CorelDRAW file with `pages`.
    pub fn cdr_file(pages: &[Page]) -> Vec<u8> {
        let rects = || pages.iter().flat_map(|p| &p.rects);
        let fills = rects().zip(1..).map(|(r, id)| fill(id, r.rgb)).collect::<Vec<_>>();
        let mut parts = vec![record(b"vrsn", &1300u16.to_le_bytes()), list(b"LIST", b"doc ", &fills)];
        let mut id = 0;
        for page in pages {
            let size = record(b"loda", &arguments(Some(0), &[(0x4aba, [units(page.width), units(page.height)].concat())]));
            let mut layer = vec![record(b"flgs", &0x0a00_0000u32.to_le_bytes())];
            for r in &page.rects {
                id += 1;
                layer.push(rectangle(r, id));
            }
            parts.push(list(b"LIST", b"page", &[list(b"LIST", b"obj ", &[size]), list(b"LIST", b"layr", &layer)]));
        }
        list(b"RIFF", b"CDRD", &parts)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Page, Rect, cdr_file};
    use super::*;
    #[cfg(cdr)]
    use vs_core::{Fill, Rgba};

    fn two_pages() -> Vec<u8> {
        cdr_file(&[
            Page { width: 4.0, height: 3.0, rects: vec![Rect { x: -1.0, y: 0.0, width: 2.0, height: 1.0, rgb: 0xff0000 }] },
            Page {
                width: 6.0,
                height: 2.0,
                rects: vec![
                    Rect { x: 0.5, y: -1.0, width: 1.0, height: 1.0, rgb: 0x0000ff },
                    Rect { x: -3.0, y: -1.0, width: 1.0, height: 2.0, rgb: 0x00ff00 },
                ],
            },
        ])
    }

    #[test]
    fn the_committed_fixture_is_what_the_writer_makes() {
        let committed = include_bytes!("../testdata/two-pages.cdr");
        assert_eq!(committed.as_slice(), two_pages().as_slice(), "regenerate testdata/two-pages.cdr");
    }

    #[test]
    fn corel_extensions_are_recognised() {
        assert!(is_cdr_path(Path::new("a/Logo.CDR")) && is_cdr_path(Path::new("b.cmx")));
        assert!(!is_cdr_path(Path::new("c.svg")) && !is_cdr_path(Path::new("cdr")));
    }

    #[cfg(cdr)]
    #[test]
    fn pages_come_in_at_their_size_with_their_shapes_in_place() {
        let pages = read_cdr(&two_pages()).expect("reads");
        assert_eq!(pages.len(), 2);
        assert!((pages[0].size.width - 288.0).abs() < 1e-6 && (pages[0].size.height - 216.0).abs() < 1e-6, "{:?}", pages[0].size);
        assert!((pages[1].size.width - 432.0).abs() < 1e-6 && (pages[1].size.height - 144.0).abs() < 1e-6);
        let red = &pages[0].objects;
        assert_eq!(red.len(), 1);
        assert!(matches!(red[0].shape, Shape::Curve { .. }));
        assert_eq!(red[0].style.fill, Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
        assert_eq!(red[0].style.outline, None);
        // 2 × 1 inches, its left edge 1 inch from the page's left and top 0.5 inch down.
        let b = red[0].world_bounds();
        let want = vs_core::kurbo::Rect::new(72.0, 36.0, 216.0, 108.0);
        assert!((b.x0 - want.x0).abs() < 1e-3 && (b.y0 - want.y0).abs() < 1e-3 && (b.x1 - want.x1).abs() < 1e-3 && (b.y1 - want.y1).abs() < 1e-3, "{b:?}");
        // Two objects, not wrapped in libcdr's layer group, bottom first as the app keeps them.
        let fills: Vec<_> = pages[1].objects.iter().map(|o| o.style.fill.clone()).collect();
        assert_eq!(fills, [Some(Fill::Solid(Rgba::rgb(0, 255, 0))), Some(Fill::Solid(Rgba::rgb(0, 0, 255)))]);
    }

    #[cfg(cdr)]
    #[test]
    fn a_document_gets_a_page_for_each_page() {
        let doc = cdr_document(&two_pages(), "Logo").expect("reads");
        assert_eq!(doc.title, "Logo");
        assert_eq!(doc.page_count(), 2);
        assert_eq!(doc.active_page_index(), 0);
        assert!((doc.pages()[1].size.width - 432.0).abs() < 1e-6);
        assert_eq!(doc.pages()[1].objects.len(), 2);
        let ids: std::collections::HashSet<_> = doc.pages().iter().flat_map(|p| &p.objects).map(|o| o.id).collect();
        assert_eq!(ids.len(), 3, "every object has its own id");
    }

    #[cfg(cdr)]
    #[test]
    fn other_files_are_refused() {
        let err = read_cdr(b"not a drawing at all").unwrap_err();
        assert!(err.to_string().contains("not a CorelDRAW"), "{err}");
        let mut damaged = two_pages();
        damaged.truncate(60);
        assert!(matches!(read_cdr(&damaged), Err(IoError::Cdr(_))));
    }

    #[cfg(not(cdr))]
    #[test]
    fn without_libcdr_reading_says_so() {
        assert!(read_cdr(&two_pages()).unwrap_err().to_string().contains("without libcdr"));
    }

    #[test]
    fn bmp_images_become_png() {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        let mut bmp = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageFormat::Bmp).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bmp);
        let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="30" height="20"><image x="0" y="0" width="30" height="20" xlink:href="data:image/bmp;base64,{b64}"/></svg>"#);
        let png = bmp_to_png(&svg);
        assert!(png.contains("data:image/png;base64,") && !png.contains("image/bmp"));
        let (_, objects) = crate::import::svg_drawing(png.as_bytes()).unwrap();
        let [Object { shape: Shape::Image(b), .. }] = objects.as_slice() else { panic!("one bitmap: {objects:?}") };
        assert_eq!((b.width, b.height), (3, 2));
        assert!((objects[0].world_bounds().width() - 22.5).abs() < 1e-6, "30 px is 22.5 pt");
        assert_eq!(bmp_to_png("<svg/>"), Cow::Borrowed("<svg/>"));
    }
}


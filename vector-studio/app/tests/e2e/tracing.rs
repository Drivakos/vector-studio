//! Bitmaps > Trace Bitmap: Quick Trace, Outline Trace… and Centerline Trace….

use egui::{Key, Modifiers};
use vs_core::kurbo::{Rect, Shape as _};
use vs_core::trace::{TraceMethod, TracePreset};
use vs_core::{Object, Rgba, Shape};

use crate::support::E2e;

/// The test bitmap's size in pixels.
const SIDE: u32 = 80;

/// Import `img` as a PNG; the bitmap is selected.
fn import(app: &mut E2e, name: &str, img: image::RgbaImage) {
    let path = crate::support::temp_dir(name).join("art.png");
    img.save(&path).unwrap();
    app.script.borrow_mut().open_paths.push_back(path);
    app.shortcut(Modifiers::COMMAND, Key::I);
    assert!(matches!(app.objects().last().unwrap().shape, Shape::Image(_)), "imported");
}

/// White, with a black block on the left and a red one on the right.
fn two_blocks() -> image::RgbaImage {
    image::RgbaImage::from_fn(SIDE, SIDE, |x, y| match (x, y) {
        (8..36, 20..60) => image::Rgba([0, 0, 0, 255]),
        (44..72, 20..60) => image::Rgba([255, 0, 0, 255]),
        _ => image::Rgba([255, 255, 255, 255]),
    })
}

/// Where pixel area (`x0`, `y0`)–(`x1`, `y1`) of the bitmap lies on the page.
fn on_page(bitmap: &Object, x0: u32, y0: u32, x1: u32, y1: u32) -> Rect {
    let b = bitmap.world_bounds();
    let at = |x: u32, y: u32| (b.x0 + f64::from(x) / f64::from(SIDE) * b.width(), b.y0 + f64::from(y) / f64::from(SIDE) * b.height());
    let (a, c) = (at(x0, y0), at(x1, y1));
    Rect::new(a.0, a.1, c.0, c.1)
}

/// The traced group's curves, with their page bounds.
fn curves(group: &Object) -> Vec<(&Object, Rect)> {
    let Shape::Group { children } = &group.shape else { panic!("a group: {:?}", group.shape) };
    children.iter().map(|c| (c, (group.transform * c.transform * c.shape.to_path()).bounding_box())).collect()
}

fn fill_of(o: &Object) -> Option<Rgba> {
    match &o.style.fill {
        Some(vs_core::Fill::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn within(a: Rect, b: Rect, tolerance: f64) -> bool {
    [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)].iter().all(|(p, q)| (p - q).abs() <= tolerance)
}

fn near(px: [u8; 4], c: [u8; 3]) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 6)
}

#[test]
fn trace_items_need_a_bitmap() {
    let mut app = E2e::new();
    let items = ["Quick Trace", "Outline Trace…", "Centerline Trace…"];
    for item in items {
        assert!(!app.menu_enabled(&["Bitmaps", "Trace Bitmap", item]), "{item}: nothing selected");
    }
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    assert!(!app.menu_enabled(&["Bitmaps", "Trace Bitmap", "Quick Trace"]), "a rectangle is not a bitmap");
    import(&mut app, "trace-enabled", two_blocks());
    for item in items {
        assert!(app.menu_enabled(&["Bitmaps", "Trace Bitmap", item]), "{item}: a bitmap is selected");
    }
}

#[test]
fn quick_trace_puts_grouped_curves_over_the_bitmap() {
    let mut app = E2e::new();
    import(&mut app, "quick-trace", two_blocks());
    let bitmap = app.objects()[0].clone();
    app.menu(&["Bitmaps", "Trace Bitmap", "Quick Trace"]);

    assert_eq!(app.objects().len(), 2, "the bitmap stays");
    assert_eq!(app.objects()[0].id, bitmap.id, "the curves go above it");
    let group = app.objects()[1].clone();
    assert_eq!(app.selection(), vec![group.id], "and are selected");
    let traced = curves(&group);
    assert_eq!(traced.len(), 2, "black and red; the white background is left out");
    let tolerance = bitmap.world_bounds().width() / f64::from(SIDE);
    let black = on_page(&bitmap, 8, 20, 36, 60);
    let red = on_page(&bitmap, 44, 20, 72, 60);
    for (color, area) in [(Rgba::rgb(0, 0, 0), black), (Rgba::rgb(255, 0, 0), red)] {
        let (curve, bounds) = traced.iter().find(|(c, _)| fill_of(c) == Some(color)).unwrap_or_else(|| panic!("a {color:?} curve"));
        assert!(matches!(curve.shape, Shape::Curve { .. }));
        assert!(bounds.inflate(tolerance, tolerance).contains_rect(area), "{color:?} covers its block: {bounds:?} vs {area:?}");
    }
    // The curve on top fits its block; the one below may reach under it, so no gap shows.
    let (top, bounds) = traced.last().unwrap();
    let area = if fill_of(top) == Some(Rgba::rgb(0, 0, 0)) { black } else { red };
    assert!(within(*bounds, area, tolerance), "the top curve fits its block: {bounds:?} vs {area:?}");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 1, "one undo step");
    assert_eq!(app.objects()[0].id, bitmap.id);
}

#[test]
fn outline_trace_window_previews_and_traces_with_its_settings() {
    let mut app = E2e::new();
    import(&mut app, "outline-trace", two_blocks());
    let bitmap = app.objects()[0].clone();
    app.menu(&["Bitmaps", "Trace Bitmap", "Outline Trace…"]);
    let dialog = |app: &E2e| app.state().trace.dialog.as_ref().expect("the window is open").settings;
    assert_eq!(dialog(&app).preset, TraceMethod::Outline.default_preset());
    assert!(app.exists("Preview (Outline Trace)"));
    assert_eq!(app.state().trace.dialog.as_ref().unwrap().previewed(), Some(dialog(&app)), "previewed as set");

    app.click("Type of image (Outline Trace)");
    app.click("Line art");
    assert_eq!(dialog(&app).preset, TracePreset::LineArt);
    assert_eq!(dialog(&app).colors, 2, "the kind of image brings its colours");
    app.fill("Colors (Outline Trace)", "3");
    assert_eq!(dialog(&app).colors, 3);
    assert_eq!(app.state().trace.dialog.as_ref().unwrap().previewed().map(|s| s.colors), Some(3), "the preview follows");
    assert!(!app.is_enabled("Remove color from entire image (Outline Trace)") || dialog(&app).remove_background);
    app.click("Delete original image (Outline Trace)");
    app.click("OK (Outline Trace)");

    assert!(app.state().trace.dialog.is_none());
    assert_eq!(app.objects().len(), 1, "the bitmap is gone");
    let group = app.objects()[0].clone();
    assert_eq!(curves(&group).len(), 2);
    assert_eq!(app.state().trace.outline.colors, 3, "remembered for next time");
    assert!(app.state().trace.delete_original);

    // What shows now is the curves.
    app.click_doc(10.0, 800.0);
    let red = on_page(&bitmap, 44, 20, 72, 60).center();
    let black = on_page(&bitmap, 8, 20, 36, 60).center();
    assert!(near(app.pixel(red.x, red.y), [255, 0, 0]), "red: {:?}", app.pixel(red.x, red.y));
    assert!(near(app.pixel(black.x, black.y), [0, 0, 0]), "black: {:?}", app.pixel(black.x, black.y));
}

#[test]
fn quick_trace_uses_the_outline_settings_last_confirmed_and_cancel_traces_nothing() {
    let mut app = E2e::new();
    import(&mut app, "trace-cancel", two_blocks());
    app.menu(&["Bitmaps", "Trace Bitmap", "Outline Trace…"]);
    app.click("Remove background (Outline Trace)");
    app.click("Cancel (Outline Trace)");
    assert_eq!(app.objects().len(), 1, "Cancel traces nothing");
    assert!(app.state().trace.outline.remove_background, "and keeps nothing");

    app.menu(&["Bitmaps", "Trace Bitmap", "Outline Trace…"]);
    app.click("Remove background (Outline Trace)");
    app.click("OK (Outline Trace)");
    assert_eq!(curves(&app.objects()[1].clone()).len(), 3, "the white is traced too");

    // Everything selected: only the bitmap is traced.
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.menu(&["Bitmaps", "Trace Bitmap", "Quick Trace"]);
    assert_eq!(app.objects().len(), 3);
    assert_eq!(curves(&app.objects()[1].clone()).len(), 3, "Quick Trace kept the background as Outline Trace last did");
}

#[test]
fn centerline_trace_draws_lines_along_strokes() {
    let mut app = E2e::new();
    // A black stroke 6 pixels thick across the middle, on white.
    let img = image::RgbaImage::from_fn(SIDE, SIDE, |x, y| match (x, y) {
        (10..70, 37..43) => image::Rgba([0, 0, 0, 255]),
        _ => image::Rgba([255, 255, 255, 255]),
    });
    import(&mut app, "centerline-trace", img);
    let bitmap = app.objects()[0].clone();
    app.menu(&["Bitmaps", "Trace Bitmap", "Centerline Trace…"]);
    assert_eq!(app.state().trace.dialog.as_ref().unwrap().settings.method(), TraceMethod::Centerline);
    assert!(!app.exists("Remove object overlap (Centerline Trace)"), "only Outline Trace fills areas");
    app.click("Type of image (Centerline Trace)");
    app.click("Technical illustration");
    app.click("OK (Centerline Trace)");

    assert_eq!(app.objects().len(), 2);
    let line = app.objects()[1].clone();
    assert!(matches!(line.shape, Shape::Curve { .. }), "one line, not a group: {:?}", line.shape);
    assert!(line.style.fill.is_none(), "not filled");
    let outline = line.style.outline.as_ref().expect("an outline");
    let pixel = bitmap.world_bounds().width() / f64::from(SIDE);
    assert!((outline.width - 6.0 * pixel).abs() < 1.5 * pixel, "as wide as the stroke: {} vs {}", outline.width, 6.0 * pixel);
    let b = (line.transform * line.shape.to_path()).bounding_box();
    let stroke = on_page(&bitmap, 10, 37, 70, 43);
    assert!((b.center().y - stroke.center().y).abs() < pixel, "down the middle: {b:?} vs {stroke:?}");
    assert!(b.x0 < stroke.x0 + 4.0 * pixel && b.x1 > stroke.x1 - 4.0 * pixel, "end to end: {b:?}");
}

#[test]
fn a_bitmap_with_nothing_to_trace_says_so() {
    let mut app = E2e::new();
    import(&mut app, "trace-blank", image::RgbaImage::from_pixel(SIDE, SIDE, image::Rgba([255, 255, 255, 255])));
    app.menu(&["Bitmaps", "Trace Bitmap", "Quick Trace"]);
    assert_eq!(app.objects().len(), 1);
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Nothing to trace"));
}

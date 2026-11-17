//! Bitmaps menu (Convert to Bitmap, Resample) and the bitmap effects of the Effects menu.

use egui::{Key, Modifiers};
use vs_core::raster::Pixels;
use vs_core::{Bitmap, Shape};

use crate::support::{E2e, rect_close};

/// A red square at (100, 100)–(172, 172), converted to a 72 dpi bitmap (72 × 72 px plus
/// the half outline on each side), selected.
fn red_bitmap() -> E2e {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 172.0, 172.0);
    app.click("Swatch Red");
    app.menu(&["Bitmaps", "Convert to Bitmap…"]);
    app.fill("Resolution (bitmap)", "72");
    app.click("OK (Convert to Bitmap)");
    app
}

fn bitmap(app: &E2e) -> &Bitmap {
    match &app.objects()[0].shape {
        Shape::Image(b) => b,
        other => panic!("a bitmap: {other:?}"),
    }
}

fn pixels(app: &E2e) -> Pixels {
    vs_io::bitmap_pixels(bitmap(app)).unwrap()
}

#[test]
fn convert_to_bitmap_replaces_the_selection_with_one_bitmap_in_place() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 160.0, 140.0);
    app.click("Swatch Red");
    app.draw_rect(200.0, 100.0, 260.0, 140.0);
    app.shortcut(Modifiers::COMMAND, Key::A);
    let drawn = app.bounds(0).union(app.bounds(1)).inflate(0.25, 0.25);
    app.menu(&["Bitmaps", "Convert to Bitmap…"]);
    assert_eq!(app.value("Resolution (bitmap)"), "300 dpi");
    app.fill("Resolution (bitmap)", "144");
    app.click("Transparent background");
    app.click("OK (Convert to Bitmap)");

    assert_eq!(app.objects().len(), 1);
    let id = app.objects()[0].id;
    assert_eq!(app.selection(), vec![id]);
    let b = bitmap(&app).clone();
    // Both rectangles and half their 0.5 pt outlines, at 2 px a point, in whole pixels.
    assert_eq!((b.width, b.height), ((drawn.width() * 2.0).ceil() as u32, (drawn.height() * 2.0).ceil() as u32));
    let placed = app.bounds(0);
    assert!(rect_close(placed, drawn.x0, drawn.y0, drawn.x0 + f64::from(b.width) / 2.0, drawn.y0 + f64::from(b.height) / 2.0), "{placed:?}");
    let p = pixels(&app);
    let [r, g, b_, a] = p.pixel(60, 40);
    assert!(r > 200 && g < 40 && b_ < 40 && a == 255, "red where the first rectangle was");
    assert_eq!(p.pixel(160, 40), [255, 255, 255, 255], "white background between them");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 2, "undoable");
}

#[test]
fn adjust_effects_change_the_selected_bitmap() {
    let mut app = red_bitmap();
    assert_eq!(app.objects().len(), 1);
    let before = pixels(&app).pixel(36, 36);
    assert_eq!(before[3], 255);

    app.menu(&["Effects", "Adjust", "Grayscale"]);
    let [r, g, b, _] = pixels(&app).pixel(36, 36);
    assert!(r == g && g == b && r < 150, "grey: {:?}", [r, g, b]);

    app.menu(&["Effects", "Adjust", "Brightness/Contrast/Intensity…"]);
    app.fill("Brightness (Brightness/Contrast/Intensity)", "50");
    app.click("OK (Brightness/Contrast/Intensity)");
    assert!(pixels(&app).pixel(36, 36)[0] > r, "brighter");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(pixels(&app).pixel(36, 36)[0], r, "undoable");
}

#[test]
fn blur_distort_noise_and_sharpen_windows_apply_their_settings() {
    let mut app = red_bitmap();
    let sharp = pixels(&app);

    app.menu(&["Effects", "Blur", "Gaussian Blur…"]);
    assert_eq!(app.value("Radius (Gaussian Blur)"), "3.0 px");
    app.fill("Radius (Gaussian Blur)", "6");
    app.click("OK (Gaussian Blur)");
    let blurred = pixels(&app);
    // The dark outline spreads into the red next to it; the middle stays as it was.
    assert!(blurred.pixel(3, 36)[0] < sharp.pixel(3, 36)[0] - 10, "{:?} → {:?}", sharp.pixel(3, 36), blurred.pixel(3, 36));
    assert_eq!(blurred.pixel(36, 36), sharp.pixel(36, 36));

    app.menu(&["Effects", "Distort", "Pixelate…"]);
    app.fill("Cell size (Pixelate)", "8");
    app.click("OK (Pixelate)");
    let cells = pixels(&app);
    assert_eq!(cells.pixel(0, 0), cells.pixel(7, 7), "one colour per cell");

    for (path, window) in [
        (["Effects", "Noise", "Add Noise…"], "Add Noise"),
        (["Effects", "Sharpen", "Unsharp Mask…"], "Unsharp Mask"),
    ] {
        let before = pixels(&app);
        app.menu(&path);
        app.click(&format!("OK ({window})"));
        assert_ne!(pixels(&app), before, "{window} changed the bitmap");
    }

    // Cancel leaves the bitmap alone.
    let before = pixels(&app);
    app.menu(&["Effects", "Distort", "Swirl…"]);
    app.click("Cancel (Swirl)");
    assert_eq!(pixels(&app), before);
    assert!(!app.exists("OK (Swirl)"));
}

#[test]
fn resample_changes_the_pixels_not_the_size_on_the_page() {
    let mut app = red_bitmap();
    let place = app.bounds(0);
    let (w, h) = (bitmap(&app).width, bitmap(&app).height);
    assert_eq!(w, h);

    app.menu(&["Bitmaps", "Resample…"]);
    app.fill("Width (resample)", "20");
    assert_eq!(app.value("Height (resample)"), "20 px", "keeps the proportions");
    app.click("OK (Resample)");
    assert_eq!((bitmap(&app).width, bitmap(&app).height), (20, 20));
    assert_eq!(app.bounds(0), place);

    app.menu(&["Bitmaps", "Resample…"]);
    app.click("Maintain aspect ratio");
    app.fill("Height (resample)", "5");
    app.click("OK (Resample)");
    assert_eq!((bitmap(&app).width, bitmap(&app).height), (20, 5));
}

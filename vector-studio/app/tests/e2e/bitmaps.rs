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

#[test]
fn tone_curve_maps_levels_with_a_preview() {
    let mut app = red_bitmap();
    let before = pixels(&app).pixel(36, 36);
    app.menu(&["Effects", "Adjust", "Tone Curve…"]);
    assert!(app.exists("Preview (Tone Curve)"));
    assert_eq!(app.value("Black (Tone Curve)"), "0");

    // Black lifted to 100: every level lifts.
    app.fill("Black (Tone Curve)", "100");
    let shown = app.state().effect.as_ref().and_then(|d| d.previewed());
    assert!(matches!(shown, Some(vs_core::raster::Effect::ToneCurve { outputs, .. }) if outputs[0] == 100.0), "the preview follows: {shown:?}");

    // Only the green channel, and the black node dragged up to about half way.
    app.click("Green (Tone Curve)");
    let curve = app.rect_of("Curve (Tone Curve)");
    app.drag_screen(curve.left_bottom() + egui::vec2(2.0, -2.0), curve.left_center());
    let black: f64 = app.value("Black (Tone Curve)").parse().unwrap();
    assert!((black - 128.0).abs() < 3.0, "{black}");
    app.click("OK (Tone Curve)");

    let after = pixels(&app).pixel(36, 36);
    assert_eq!((after[0], after[2], after[3]), (before[0], before[2], before[3]), "red and blue stay: {before:?} → {after:?}");
    let lut = vs_core::raster::tone_lut([black, 63.75, 127.5, 191.25, 255.0]);
    assert_eq!(after[1], lut[before[1] as usize], "green goes through the curve: {before:?} → {after:?}");
    assert!(after[1] > before[1] + 50);

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(pixels(&app).pixel(36, 36), before, "undoable");
}

#[test]
fn bitmap_color_mask_hides_a_colour_picked_on_the_preview() {
    let mut app = red_bitmap();
    let before = pixels(&app);
    let red = before.pixel(36, 36);
    app.menu(&["Bitmaps", "Bitmap Color Mask"]);
    assert_eq!(app.value("Color (Bitmap Color Mask)"), "#FFFFFF");
    assert!(app.is_selected("Hide colors (Bitmap Color Mask)"));
    let preview = app.rect_of("Preview (Bitmap Color Mask)");
    app.drag_screen(preview.center(), preview.center());
    assert_eq!(app.value("Color (Bitmap Color Mask)"), format!("#{:02X}{:02X}{:02X}", red[0], red[1], red[2]), "picked from the preview");
    app.click("OK (Bitmap Color Mask)");

    let after = pixels(&app);
    assert_eq!(after.pixel(36, 36)[3], 0, "the red is hidden");
    assert_eq!(after.pixel(0, 36), before.pixel(0, 36), "the outline stays");
    assert_eq!((after.width, after.height), (before.width, before.height));

    // Show colors: everything but the red goes.
    app.shortcut(Modifiers::COMMAND, Key::Z);
    app.menu(&["Bitmaps", "Bitmap Color Mask"]);
    app.fill("Color (Bitmap Color Mask)", &format!("{:02x}{:02x}{:02x}", red[0], red[1], red[2]));
    app.click("Show colors (Bitmap Color Mask)");
    app.click("OK (Bitmap Color Mask)");
    let shown = pixels(&app);
    assert_eq!(shown.pixel(36, 36), red);
    assert_eq!(shown.pixel(0, 36)[3], 0, "the outline is hidden");
}

#[test]
fn straighten_image_turns_the_bitmap_within_its_frame() {
    let mut app = red_bitmap();
    let before = pixels(&app);
    let place = app.bounds(0);
    app.menu(&["Bitmaps", "Straighten Image…"]);
    assert_eq!(app.value("Angle (Straighten Image)"), "0.0°");
    assert!(app.exists("Preview (Straighten Image)"));
    app.fill("Angle (Straighten Image)", "10");
    app.click("OK (Straighten Image)");

    let after = pixels(&app);
    assert_eq!((after.width, after.height), (before.width, before.height), "the same pixels across");
    assert!(rect_close(app.bounds(0), place.x0, place.y0, place.x1, place.y1), "and the same place");
    assert_ne!(after, before);
    assert_eq!(after, vs_core::raster::Effect::Straighten { angle: 10.0 }.apply(&before), "turned 10° within its frame");
}

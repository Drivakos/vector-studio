//! Overprint Fill and Overprint Outline (Object menu), View > Simulate Overprints, and
//! View > Proof Colors.

use egui::{Key, Modifiers};
use vs_core::OverprintPart;

use crate::support::E2e;

fn near(px: [u8; 4], c: [u8; 3], tolerance: u8) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

/// A yellow square with a cyan square over its right half, the cyan one selected.
fn yellow_under_cyan(app: &mut E2e) -> vs_core::ObjectId {
    app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Yellow");
    let cyan = app.draw_rect(200.0, 150.0, 400.0, 250.0);
    app.click("Swatch Cyan");
    cyan
}

const YELLOW: [u8; 3] = [0xFF, 0xED, 0x00];
const CYAN: [u8; 3] = [0x00, 0xA0, 0xE3];
/// Cyan ink on yellow ink: the two multiplied.
const GREEN: [u8; 3] = [0x00, 0x95, 0x00];

#[test]
fn overprinted_fill_mixes_with_what_is_below_only_when_simulated() {
    let mut app = E2e::new();
    let cyan = yellow_under_cyan(&mut app);
    assert!(!app.doc().overprints(&[cyan], OverprintPart::Fill), "knocks out by default");

    app.menu(&["Object", "Overprint Fill"]);
    assert!(app.doc().overprints(&[cyan], OverprintPart::Fill));
    assert!(!app.doc().overprints(&[cyan], OverprintPart::Outline), "only the fill");
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(250.0, 200.0), CYAN, 6), "not simulated: cyan covers yellow {:?}", app.pixel(250.0, 200.0));

    app.menu(&["View", "Simulate Overprints"]);
    assert!(app.state().simulate_overprints);
    assert!(near(app.pixel(250.0, 200.0), GREEN, 8), "the inks mix: {:?}", app.pixel(250.0, 200.0));
    assert!(near(app.pixel(350.0, 200.0), CYAN, 6), "on white paper cyan stays cyan: {:?}", app.pixel(350.0, 200.0));
    assert!(near(app.pixel(150.0, 200.0), YELLOW, 6), "yellow alone stays yellow");

    // The menu item turns it off again, and undo takes the overprint away.
    app.click_doc(250.0, 200.0);
    app.menu(&["Object", "Overprint Fill"]);
    assert!(!app.doc().overprints(&[cyan], OverprintPart::Fill));
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.doc().overprints(&[cyan], OverprintPart::Fill), "undo turns it back on");

    app.menu(&["View", "Simulate Overprints"]);
    assert!(!app.state().simulate_overprints);
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(250.0, 200.0), CYAN, 6), "simulation off: {:?}", app.pixel(250.0, 200.0));
}

#[test]
fn overprinted_outline_and_group_members() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Yellow");
    // A thick cyan outline on an unfilled square across the yellow one's right edge.
    let square = app.draw_rect(250.0, 150.0, 350.0, 250.0);
    app.right_click("Swatch Cyan");
    app.fill("Outline width (property bar)", "20");
    app.menu(&["Object", "Overprint Outline"]);
    assert!(app.doc().overprints(&[square], OverprintPart::Outline));
    assert!(!app.doc().overprints(&[square], OverprintPart::Fill));
    app.menu(&["View", "Simulate Overprints"]);
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(250.0, 200.0), GREEN, 8), "outline on yellow: {:?}", app.pixel(250.0, 200.0));
    assert!(near(app.pixel(350.0, 200.0), CYAN, 6), "outline on paper: {:?}", app.pixel(350.0, 200.0));

    // On a group, every member overprints.
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.shortcut(Modifiers::COMMAND, Key::G);
    let group = app.selection();
    assert!(!app.doc().overprints(&group, OverprintPart::Fill));
    app.menu(&["Object", "Overprint Fill"]);
    assert!(app.doc().overprints(&group, OverprintPart::Fill));
}

#[test]
fn overprint_items_need_a_selection_and_are_saved() {
    let mut app = E2e::new();
    assert!(!app.menu_enabled(&["Object", "Overprint Fill"]), "nothing selected");
    assert!(!app.menu_enabled(&["Object", "Overprint Outline"]));
    let cyan = yellow_under_cyan(&mut app);
    assert!(app.menu_enabled(&["Object", "Overprint Fill"]));
    app.menu(&["Object", "Overprint Fill"]);

    let path = crate::support::temp_dir("overprint").join("inks.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let back = vs_io::open_document(&path).unwrap();
    assert!(back.overprints(&[cyan], OverprintPart::Fill), "saved with the drawing");
    assert!(!back.overprints(&[cyan], OverprintPart::Outline));
}

#[test]
fn proof_colors_shows_screen_colours_as_they_would_print() {
    let dir = crate::support::temp_dir("proof");
    let svg = dir.join("screen.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="96">
            <rect width="96" height="96" fill="#0000ff"/>
            <rect x="104" width="96" height="96" fill="#808080"/>
        </svg>"##,
    )
    .unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(svg);
    app.shortcut(Modifiers::COMMAND, Key::I);
    let b = app.objects()[0].world_bounds();
    let (blue, grey) = ((b.x0 + 36.0, b.center().y), (b.x1 - 36.0, b.center().y));
    app.click_doc(10.0, 800.0);
    assert!(near(app.pixel(blue.0, blue.1), [0, 0, 255], 2), "screen blue: {:?}", app.pixel(blue.0, blue.1));

    assert!(app.menu_enabled(&["View", "Proof Colors"]), "macOS has a CMYK profile");
    app.menu(&["View", "Proof Colors"]);
    assert!(app.state().proof_colors);
    let px = app.pixel(blue.0, blue.1);
    assert!(px[0] > 20 && px[1] > 20 && px[2] < 235, "inks can't print screen blue: {px:?}");
    let px = app.pixel(grey.0, grey.1);
    assert!(px[..3].iter().all(|v| v.abs_diff(128) < 20), "grey stays grey: {px:?}");
    assert!(near(app.pixel(10.0, 10.0), [255, 255, 255], 2), "paper stays white");
    assert_eq!(app.objects()[0].leaves()[0].style.fill, Some(vs_core::Rgba::rgb(0, 0, 255).into()), "the drawing keeps its colours");

    app.menu(&["View", "Proof Colors"]);
    assert!(!app.state().proof_colors);
    assert!(near(app.pixel(blue.0, blue.1), [0, 0, 255], 2), "off again: {:?}", app.pixel(blue.0, blue.1));
}

#[test]
fn proof_colors_reaches_bitmaps() {
    let path = crate::support::temp_dir("proof-bitmap").join("blue.png");
    let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 255, 255]));
    img.save(&path).unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(path);
    app.shortcut(Modifiers::COMMAND, Key::I);
    let c = app.objects()[0].world_bounds().center();
    app.click_doc(10.0, 800.0);
    assert!(near(app.pixel(c.x, c.y), [0, 0, 255], 2), "{:?}", app.pixel(c.x, c.y));
    app.menu(&["View", "Proof Colors"]);
    let px = app.pixel(c.x, c.y);
    assert!(px[0] > 20 && px[1] > 20 && px[2] < 235, "proofed bitmap: {px:?}");
}

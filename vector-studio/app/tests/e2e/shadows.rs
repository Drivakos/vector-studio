//! Drop Shadow tool: a drag from an object casts its shadow; the property bar edits or
//! clears it; Object > Clear Effect removes it. Shadows are saved and exported.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::kurbo::Vec2;

use crate::support::{E2e, temp_dir};

fn grey(px: [u8; 4]) -> u8 {
    assert!(px[0].abs_diff(px[1]) <= 3 && px[1].abs_diff(px[2]) <= 3, "a grey: {px:?}");
    px[0]
}

#[test]
fn dragging_from_an_object_casts_a_shadow_that_stays_editable() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    let plain_pdf = vs_io::document_to_pdf(app.doc()).unwrap().len();
    app.click("Drop Shadow");
    assert_eq!(app.state().tool, Tool::DropShadow);

    // Cast down and to the right: 20 pt across, 30 pt down.
    app.drag_doc((150.0, 150.0), (170.0, 180.0));
    let shadow = app.objects()[0].shadow.clone().expect("a shadow");
    assert!((shadow.offset - Vec2::new(20.0, 30.0)).hypot() < 0.01, "{:?}", shadow.offset);
    assert_eq!(app.selection(), vec![id]);
    let shade = grey(app.pixel(210.0, 215.0));
    assert!((110..=150).contains(&shade), "half-opaque black beside the square: {shade}");
    assert_eq!(app.pixel(160.0, 170.0)[..3], [0xE3, 0x06, 0x13], "the square (palette red) is drawn over its shadow");
    assert_eq!(app.pixel(110.0, 215.0)[..3], [255, 255, 255], "nothing left of the shadow");
    assert!(grey(app.pixel(221.0, 215.0)) < 250, "a feathered edge reaches past the shadow's side");
    assert!(!app.objects()[0].hit(vs_core::kurbo::Point::new(210.0, 215.0), 0.5), "the shadow is not part of the object");

    // The property bar edits it.
    app.fill("Shadow opacity", "100");
    assert!(grey(app.pixel(210.0, 215.0)) < 15, "solid black");
    app.fill("Shadow feathering", "0");
    assert_eq!(app.pixel(221.0, 215.0)[..3], [255, 255, 255], "a hard edge");
    let unit = app.doc().display_unit;
    app.fill("Shadow offset X", &format!("{}", unit.from_points(40.0)));
    let shadow = app.objects()[0].shadow.clone().unwrap();
    assert!((shadow.offset.x - 40.0).abs() < 1e-6 && shadow.opacity == 1.0 && shadow.feather == 0.0, "{shadow:?}");
    assert!(grey(app.pixel(235.0, 215.0)) < 15, "moved right");

    // Saved with the drawing, and exported.
    let path = temp_dir("shadow-save").join("Shadow.vstudio");
    vs_io::save_document(app.doc(), &path).unwrap();
    let opened = vs_io::open_document(&path).unwrap();
    let saved = opened.active_page().objects[0].shadow.as_ref().map(|s| s.offset).expect("saved");
    assert!((saved - Vec2::new(40.0, 30.0)).hypot() < 0.01, "{saved:?}");
    assert!(vs_io::page_to_svg(app.doc().active_page()).contains("<image"), "the SVG draws the shadow as an image");
    let pdf = vs_io::document_to_pdf(app.doc()).unwrap().len();
    assert!(pdf > plain_pdf + 200, "the PDF has the shadow image: {pdf} vs {plain_pdf}");
    let png = vs_io::page_to_png(app.doc().active_page(), &Default::default()).unwrap();
    assert!(png.len() > 100);

    // Clear shadow, then undo; Object > Clear Effect.
    app.click("Clear shadow (property bar)");
    assert!(app.objects()[0].shadow.is_none());
    assert_eq!(app.pixel(235.0, 215.0)[..3], [255, 255, 255]);
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects()[0].shadow.is_some(), "undo brings it back");
    app.menu(&["Object", "Clear Effect"]);
    assert!(app.objects()[0].shadow.is_none(), "Clear Effect removes it");
}

#[test]
fn a_new_shadow_takes_the_property_bar_settings_and_a_click_only_selects() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.click("Drop Shadow");
    app.click_doc(600.0, 700.0);
    assert!(app.selection().is_empty(), "an empty click clears the selection");
    app.fill("Shadow opacity", "80");
    app.click_doc(100.0, 150.0);
    assert_eq!(app.selection(), vec![id]);
    assert!(app.objects()[0].shadow.is_none(), "a click casts nothing");
    app.drag_doc((150.0, 150.0), (160.0, 160.0));
    let shadow = app.objects()[0].shadow.clone().unwrap();
    assert!((shadow.opacity - 0.8).abs() < 1e-9 && shadow.feather == vs_core::DropShadow::FEATHER, "{shadow:?}");
}

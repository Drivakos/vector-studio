//! Crop, Knife and Eraser tools.

use egui::Key;
use vs_core::kurbo::Shape as _;

use crate::support::{E2e, close};

fn area(app: &E2e, i: usize) -> f64 {
    app.objects()[i].world_path().area().abs()
}

#[test]
fn crop_tool_keeps_the_dragged_area_after_enter() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.click_doc(600.0, 700.0); // nothing selected: everything is cropped
    app.click("Crop");
    assert!(!app.is_enabled("Crop (property bar)"), "nothing to crop to yet");
    app.drag_doc((150.0, 50.0), (250.0, 250.0));
    assert_eq!(app.objects().len(), 2, "dragging only marks the area");
    assert!(app.is_enabled("Crop (property bar)"));

    app.key(Key::Enter);
    assert_eq!(app.objects().len(), 1, "the square outside the area is gone");
    let b = app.bounds(0);
    assert!(close(b.x0, 150.0) && close(b.x1, 200.0), "the other is cut at the edge: {b:?}");
    assert!((area(&app, 0) - 5000.0).abs() < 0.1, "{}", area(&app, 0));
    let left = app.pixel(120.0, 150.0);
    let right = app.pixel(170.0, 150.0);
    assert!(left[0] > 200 && left[1] > 200, "cut away: {left:?}");
    assert!(right[0] > 200 && right[1] < 60, "kept: {right:?}");

    app.shortcut(egui::Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 2, "one undo step");
}

#[test]
fn crop_area_can_be_cleared_and_double_click_applies_it_to_the_selection() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    let other = app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.click("Crop");
    app.drag_doc((350.0, 50.0), (450.0, 250.0));
    app.key(Key::Escape);
    assert!(!app.is_enabled("Crop (property bar)"), "Esc clears the area");
    app.drag_doc((350.0, 50.0), (450.0, 250.0));
    app.double_click_doc(400.0, 150.0);
    assert_eq!(app.objects().len(), 2, "only the selected square is cropped");
    assert_eq!(app.selection(), vec![other]);
    assert!(close(app.bounds(1).x0, 350.0) && close(app.bounds(0).x0, 100.0), "{:?} {:?}", app.bounds(0), app.bounds(1));
}

#[test]
fn knife_cuts_an_object_in_two_along_the_dragged_line() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.flyout_tool("Crop", "Knife");
    app.drag_doc((150.0, 50.0), (150.0, 250.0));
    assert_eq!(app.objects().len(), 2);
    let widths: Vec<f64> = (0..2).map(|i| app.bounds(i).width()).collect();
    assert!(widths.iter().any(|w| close(*w, 50.0)) && widths.iter().any(|w| close(*w, 150.0)), "{widths:?}");
    assert_eq!(app.selection().len(), 2, "both pieces are selected");
    assert!(((area(&app, 0) + area(&app, 1)) - 20000.0).abs() < 0.1, "{} {}", area(&app, 0), area(&app, 1));
}

#[test]
fn eraser_removes_a_band_with_the_size_from_the_property_bar() {
    let mut app = E2e::new();
    let unit = app.doc().display_unit;
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.key(Key::X);
    assert_eq!(app.state().tool, vector_studio::toolbox::Tool::Eraser);
    app.fill("Eraser size", &format!("{}", unit.from_points(20.0)));
    app.stroke_doc(&[(150.0, 50.0), (150.0, 120.0), (150.0, 180.0), (150.0, 250.0)]);
    let a = area(&app, 0);
    assert!((a - (20000.0 - 20.0 * 100.0)).abs() < 1.0, "a 20 pt band is erased: {a}");
    let px = app.pixel(150.0, 150.0);
    assert!(px[0] > 200 && px[1] > 200, "the band shows the page: {px:?}");
    let kept = app.pixel(200.0, 150.0);
    assert!(kept[1] < 60, "the rest is still red: {kept:?}");
}

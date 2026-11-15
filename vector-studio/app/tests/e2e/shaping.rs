//! Shaping (Weld, Trim, Intersect, Simplify, Front/Back Minus, Boundary), Combine (Cmd+L)
//! and Break Apart (Cmd+K).

use egui::{Key, Modifiers};
use vs_core::{ObjectId, Rgba, Shape};

use crate::support::{E2e, rect_close};

const RED: Rgba = Rgba::rgb(0xE3, 0x06, 0x13);
const BLUE: Rgba = Rgba::rgb(0x00, 0x71, 0xBC);

/// A red square at the back and a blue one overlapping its right half, in front.
fn two_squares(app: &mut E2e) -> (ObjectId, ObjectId) {
    let red = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    let blue = app.draw_rect(150.0, 100.0, 250.0, 200.0);
    app.click("Swatch Blue");
    (red, blue)
}

fn fill(app: &E2e, id: ObjectId) -> Option<Rgba> {
    app.objects().iter().find(|o| o.id == id).expect("object exists").style.fill
}

fn bounds(app: &E2e, id: ObjectId) -> vs_core::kurbo::Rect {
    let i = app.ids().iter().position(|x| *x == id).expect("object exists");
    app.bounds(i)
}

fn marquee_all(app: &mut E2e) {
    app.click_doc(600.0, 700.0);
    app.drag_doc((50.0, 50.0), (300.0, 300.0));
}

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[1] < 60 && px[2] < 60
}

fn is_white(px: [u8; 4]) -> bool {
    px[..3] == [255, 255, 255]
}

#[test]
fn shaping_buttons_show_for_several_objects_and_need_closed_ones() {
    let mut app = E2e::new();
    two_squares(&mut app);
    assert!(!app.exists("Weld (property bar)"), "one object selected: no shaping buttons");
    marquee_all(&mut app);
    for name in ["Weld", "Trim", "Intersect", "Simplify", "Front minus back", "Back minus front", "Create boundary", "Combine"] {
        assert!(app.is_enabled(&format!("{name} (property bar)")), "{name}");
    }
    assert!(!app.exists("Break Apart (property bar)"));

    // An open line has no area to weld.
    app.key(Key::F5);
    app.stroke_doc(&[(120.0, 250.0), (160.0, 260.0), (200.0, 250.0)]);
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert!(!app.is_enabled("Weld (property bar)"));
    assert!(app.is_enabled("Combine (property bar)"), "open lines can still be combined");
}

#[test]
fn weld_takes_the_bottom_object_after_a_marquee_and_the_last_selected_after_shift_click() {
    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    marquee_all(&mut app);
    app.click("Weld (property bar)");
    assert_eq!(app.ids(), vec![red], "one object, in the bottom one's place");
    assert_eq!(fill(&app, red), Some(RED), "marquee: the bottom-most object is the target");
    assert!(rect_close(bounds(&app, red), 100.0, 100.0, 250.0, 200.0));
    assert!(matches!(app.objects()[0].shape, Shape::Curve { .. }));
    app.click_doc(600.0, 700.0);
    assert!(is_red(app.pixel(225.0, 150.0)), "the blue square's area is now red");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.ids(), vec![red, blue], "undo brings both back");

    // Select blue, then Shift-click red: red is the last selected, so the target.
    app.click_doc(225.0, 150.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(125.0, 150.0);
    app.release_mods();
    app.menu(&["Object", "Shaping", "Weld"]);
    assert_eq!(app.ids(), vec![red]);
    assert_eq!(fill(&app, red), Some(RED));

    app.shortcut(Modifiers::COMMAND, Key::Z);
    app.click_doc(125.0, 150.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(225.0, 150.0);
    app.release_mods();
    app.click("Weld (property bar)");
    assert_eq!(app.ids(), vec![blue]);
    assert_eq!(fill(&app, blue), Some(BLUE), "blue was selected last");
}

#[test]
fn trim_cuts_the_target_and_intersect_adds_the_overlap() {
    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    // Marquee: the red square at the back is the target, trimmed by the blue one.
    marquee_all(&mut app);
    app.click("Trim (property bar)");
    assert_eq!(app.ids(), vec![red, blue], "the source stays");
    assert_eq!(app.selection(), vec![red]);
    assert!(rect_close(bounds(&app, red), 100.0, 100.0, 150.0, 200.0));
    // Move the blue square away: the trimmed part shows the page.
    app.click_doc(225.0, 150.0);
    app.drag_doc((225.0, 150.0), (425.0, 150.0));
    app.click_doc(600.0, 700.0);
    assert!(is_white(app.pixel(175.0, 150.0)), "{:?}", app.pixel(175.0, 150.0));
    assert!(is_red(app.pixel(125.0, 150.0)));

    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    marquee_all(&mut app);
    app.menu(&["Object", "Shaping", "Intersect"]);
    let ids = app.ids();
    assert_eq!(ids.len(), 3);
    assert_eq!(&ids[..2], &[red, blue], "both originals stay");
    let new = ids[2];
    assert_eq!(app.selection(), vec![new]);
    assert!(rect_close(bounds(&app, new), 150.0, 100.0, 200.0, 200.0));
    assert_eq!(fill(&app, new), Some(RED));
    app.click_doc(600.0, 700.0);
    assert!(is_red(app.pixel(175.0, 150.0)), "the overlap is on top");
}

#[test]
fn simplify_front_minus_back_back_minus_front_and_boundary() {
    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    marquee_all(&mut app);
    app.menu(&["Object", "Shaping", "Simplify"]);
    assert!(rect_close(bounds(&app, red), 100.0, 100.0, 150.0, 200.0), "red loses what blue covers");
    assert!(rect_close(bounds(&app, blue), 150.0, 100.0, 250.0, 200.0));

    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    marquee_all(&mut app);
    app.click("Front minus back (property bar)");
    assert_eq!(app.ids(), vec![blue]);
    assert!(rect_close(bounds(&app, blue), 200.0, 100.0, 250.0, 200.0));
    app.shortcut(Modifiers::COMMAND, Key::Z);
    marquee_all(&mut app);
    app.click("Back minus front (property bar)");
    assert_eq!(app.ids(), vec![red]);
    assert!(rect_close(bounds(&app, red), 100.0, 100.0, 150.0, 200.0));

    let mut app = E2e::new();
    let (red, blue) = two_squares(&mut app);
    marquee_all(&mut app);
    app.menu(&["Object", "Shaping", "Boundary"]);
    let ids = app.ids();
    assert_eq!(&ids[..2], &[red, blue]);
    let boundary = &app.objects()[2];
    assert_eq!(boundary.style.fill, None);
    assert!(boundary.style.outline.is_some());
    assert!(rect_close(boundary.world_bounds(), 100.0, 100.0, 250.0, 200.0));
}

#[test]
fn combine_cuts_holes_where_objects_overlap_and_break_apart_undoes_it() {
    let mut app = E2e::new();
    let big = app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Red");
    let small = app.draw_rect(150.0, 150.0, 250.0, 250.0);
    app.click("Swatch Blue");
    app.click_doc(200.0, 200.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(120.0, 120.0);
    app.release_mods();
    assert_eq!(app.selection(), vec![small, big]);

    app.shortcut(Modifiers::COMMAND, Key::L);
    assert_eq!(app.ids(), vec![big], "one object");
    assert_eq!(fill(&app, big), Some(RED), "styled like the last selected");
    app.click_doc(600.0, 700.0);
    assert!(is_white(app.pixel(200.0, 200.0)), "the overlap is a hole");
    assert!(is_red(app.pixel(120.0, 200.0)));

    app.click_doc(120.0, 200.0);
    assert!(app.is_enabled("Break Apart (property bar)"));
    app.shortcut(Modifiers::COMMAND, Key::K);
    assert_eq!(app.ids().len(), 2);
    assert_eq!(app.selection().len(), 2, "every piece is selected");
    app.click_doc(600.0, 700.0);
    assert!(is_red(app.pixel(200.0, 200.0)), "both pieces are filled again");
}

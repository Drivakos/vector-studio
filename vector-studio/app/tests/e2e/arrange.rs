//! Align (keys, toolbar, docker), order (keys, toolbar, docker), Convert to Curves.

use egui::{Key, Modifiers};
use vs_core::Shape;

use crate::support::{E2e, close};

#[test]
fn align_keys_align_one_object_to_the_page() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    let page = app.doc().active_page().bounds();
    app.key(Key::L);
    assert!(close(app.bounds(0).x0, 0.0));
    app.key(Key::R);
    assert!(close(app.bounds(0).x1, page.x1));
    app.key(Key::T);
    assert!(close(app.bounds(0).y0, 0.0));
    app.key(Key::B);
    assert!(close(app.bounds(0).y1, page.y1));
    app.key(Key::P);
    let c = app.bounds(0).center();
    assert!(close(c.x, page.center().x) && close(c.y, page.center().y), "P centres on the page");
}

#[test]
fn several_objects_align_to_the_last_selected() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    let _ = a;
    app.draw_rect(300.0, 200.0, 400.0, 260.0);
    app.click_doc(100.0, 125.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(300.0, 230.0);
    app.release_mods();
    app.key(Key::E);
    assert!(close(app.bounds(0).center().x, 350.0), "centres horizontally on the last selected");
    app.key(Key::C);
    assert!(close(app.bounds(0).center().y, 230.0));
    assert!(close(app.bounds(1).x0, 300.0), "the target does not move");
}

#[test]
fn toolbar_alignment_menu_and_align_docker() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.menu(&["Align and distribute", "Align Left"]);
    assert!(close(app.bounds(0).x0, 0.0));
    app.menu(&["Align and distribute", "Align and Distribute…"]);
    assert!(app.is_selected("Align and Distribute docker"), "opens the docker");
    app.click("Align top (T) (docker)");
    assert!(close(app.bounds(0).y0, 0.0));
    app.click("Center to Page (P)");
    let page = app.doc().active_page().bounds();
    assert!(close(app.bounds(0).center().x, page.center().x));
}

#[test]
fn order_from_keys_toolbar_and_docker() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    let b = app.draw_rect(120.0, 120.0, 170.0, 170.0);
    let c = app.draw_rect(140.0, 140.0, 190.0, 190.0);
    app.click_doc(100.0, 110.0); // a, only its left edge is uncovered
    app.shortcut(Modifiers::SHIFT, Key::PageUp);
    assert_eq!(app.ids(), vec![b, c, a], "Shift+PageUp: to front");
    app.click("To back of layer");
    assert_eq!(app.ids(), vec![a, b, c]);
    app.click("Forward one");
    assert_eq!(app.ids(), vec![b, a, c]);
    app.docker("Align and Distribute");
    app.click("To front (docker)");
    assert_eq!(app.ids(), vec![b, c, a]);
    app.shortcut(Modifiers::COMMAND, Key::PageDown);
    assert_eq!(app.ids(), vec![b, a, c], "Cmd+PageDown: back one");
}

#[test]
fn convert_to_curves_from_the_property_bar_and_menu() {
    let mut app = E2e::new();
    app.key(Key::F7);
    app.drag_doc((100.0, 100.0), (200.0, 160.0));
    app.pick_tool();
    let before = app.bounds(0);
    app.click("Convert to Curves (property bar)");
    assert!(matches!(app.objects()[0].shape, Shape::Curve { .. }));
    assert!(!app.is_enabled("Convert to Curves (property bar)"), "already curves");
    assert!(close(app.bounds(0).x0, before.x0) && close(app.bounds(0).y1, before.y1));

    app.draw_rect(300.0, 300.0, 350.0, 350.0);
    app.menu(&["Object", "Convert to Curves"]);
    assert!(matches!(app.objects()[1].shape, Shape::Curve { .. }));
}

#[test]
fn distribute_from_the_docker_and_menu_and_reverse_order() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 120.0, 120.0);
    let b = app.draw_rect(150.0, 300.0, 210.0, 320.0);
    let c = app.draw_rect(400.0, 200.0, 420.0, 220.0);
    let x0 = |app: &E2e, id| app.objects().iter().find(|o| o.id == id).unwrap().world_bounds().x0;

    app.click_doc(110.0, 100.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(180.0, 300.0);
    app.release_mods();
    app.docker("Align and Distribute");
    assert!(!app.is_enabled("Distribute left (docker)"), "two objects: nothing to spread");
    app.hold(Modifiers::SHIFT);
    app.click_doc(410.0, 200.0);
    app.release_mods();

    app.click("Distribute left (docker)");
    assert!(close(x0(&app, a), 100.0) && close(x0(&app, b), 250.0) && close(x0(&app, c), 400.0));
    // Widths 20 + 60 + 20 across 320: gaps of 110.
    app.menu(&["Object", "Align and Distribute", "Distribute Spacing Horizontally"]);
    assert!(close(x0(&app, b), 230.0), "{}", x0(&app, b));
    app.click("Distribute top (docker)");
    let y0 = |app: &E2e, id| app.objects().iter().find(|o| o.id == id).unwrap().world_bounds().y0;
    assert!(close(y0(&app, c), 200.0) && close(y0(&app, a), 100.0) && close(y0(&app, b), 300.0));
    app.click("Distribute centers vertically (docker)");
    assert!(close(app.objects()[2].world_bounds().center().y, 210.0));

    app.menu(&["Object", "Order", "Reverse Order"]);
    assert_eq!(app.ids(), vec![c, b, a]);
    app.click("Reverse order (docker)");
    assert_eq!(app.ids(), vec![a, b, c]);
}

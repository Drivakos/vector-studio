//! Object > Perspective > Add Perspective (corners dragged with the Shape tool), Object >
//! Copy Effect > Perspective From, Object > Clear Effect, Object > Perspective > Draw in
//! Perspective.

use egui::{Key, Modifiers};
use vs_core::Shape;
use vs_core::kurbo::Point;
use vector_studio::toolbox::Tool;

use crate::support::E2e;

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[1] < 60 && px[2] < 60
}

fn corners(app: &E2e, i: usize) -> [Point; 4] {
    app.objects()[i].perspective_corners().expect("a perspective")
}

fn near(a: Point, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 0.5 && (a.y - y).abs() < 0.5
}

#[test]
fn add_perspective_drag_its_corners_copy_it_and_clear_it() {
    let mut app = E2e::new();
    let first = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    assert!(app.objects()[0].perspective.is_none());

    // Add Perspective changes nothing yet, and hands over to the Shape tool.
    app.menu(&["Object", "Perspective", "Add Perspective"]);
    assert_eq!(app.state().tool, Tool::Shape);
    assert!(app.objects()[0].perspective.is_some());
    assert!(near(corners(&app, 0)[0], 100.0, 100.0));

    // Pull the top corners in: the top narrows to 140..160.
    app.drag_doc((100.0, 100.0), (140.0, 100.0));
    app.drag_doc((200.0, 100.0), (160.0, 100.0));
    let c = corners(&app, 0);
    assert!(near(c[0], 140.0, 100.0) && near(c[1], 160.0, 100.0) && near(c[2], 200.0, 200.0) && near(c[3], 100.0, 200.0), "{c:?}");
    app.pick_tool();
    app.key(Key::Escape);
    assert!(is_red(app.pixel(152.0, 106.0)), "the narrow top");
    assert!(!is_red(app.pixel(110.0, 106.0)), "cut away");
    assert!(is_red(app.pixel(106.0, 194.0)), "the full-width bottom");
    // Clicking where the square was, but the perspective is not, misses it.
    app.click_doc(110.0, 106.0);
    assert!(app.selection().is_empty());

    // Copy Effect > Perspective From puts the same corners on another object's bounds.
    let second = app.draw_rect(300.0, 100.0, 400.0, 300.0);
    app.menu(&["Object", "Copy Effect", "Perspective From…"]);
    assert!(app.state().copy_perspective);
    app.click_doc(150.0, 150.0);
    assert!(!app.state().copy_perspective);
    assert_eq!(app.selection(), vec![second]);
    let c = corners(&app, 1);
    assert!(near(c[0], 340.0, 100.0) && near(c[1], 360.0, 100.0) && near(c[2], 400.0, 300.0), "{c:?}");

    // Clear Effect takes it off; undo puts it back.
    app.menu(&["Object", "Clear Effect"]);
    assert!(app.objects()[1].perspective.is_none());
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects()[1].perspective.is_some(), "undo");

    // Convert to Curves makes the perspective part of the curve.
    app.click_doc(150.0, 150.0);
    assert_eq!(app.selection(), vec![first]);
    app.shortcut(Modifiers::COMMAND, Key::Q);
    let obj = &app.objects()[0];
    assert!(obj.perspective.is_none() && matches!(obj.shape, Shape::Curve { .. }));
    assert!(!obj.hit(Point::new(110.0, 106.0), 0.0) && obj.hit(Point::new(152.0, 106.0), 0.0));
}

#[test]
fn draw_in_perspective_puts_new_shapes_on_the_plane_under_the_pointer() {
    let mut app = E2e::new();
    assert!(!app.menu_enabled(&["Object", "Perspective", "Add Perspective"]), "nothing selected");
    app.menu(&["Object", "Perspective", "Draw in Perspective"]);
    let plane = app.state().draw_perspective.clone().expect("on");
    let page = app.doc().active_page().bounds();
    let quad = plane.quad(page);
    assert!(quad[1].x - quad[0].x < quad[2].x - quad[3].x, "the plane narrows away from the viewer: {quad:?}");

    // A rectangle dragged from (200, 500) to (400, 700) on the plane.
    app.key(Key::F6);
    app.drag_doc((200.0, 500.0), (400.0, 700.0));
    assert_eq!(app.objects().len(), 1);
    let c = corners(&app, 0);
    assert!(near(c[0], 200.0, 500.0) && near(c[2], 400.0, 700.0), "the corners dragged stay under the pointer: {c:?}");
    assert!(c[1].x - c[0].x < c[2].x - c[3].x - 5.0, "the far edge is shorter: {c:?}");
    assert!(app.objects()[0].perspective.is_some(), "it keeps the perspective");

    app.pick_tool();
    app.click("Swatch Red");
    app.click_doc(20.0, 40.0);
    assert!(is_red(app.pixel(300.0, 650.0)), "{:?}", app.pixel(300.0, 650.0));

    // Drawing and the perspective are one undo step (after the fill's).
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects()[0].perspective.is_some());
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects().is_empty(), "{:?}", app.objects().len());

    // The Pick tool drags the plane's corners.
    app.pick_tool();
    app.drag_doc((quad[0].x, quad[0].y), (quad[0].x - 60.0, quad[0].y));
    let moved = app.state().draw_perspective.as_ref().unwrap().quad(page);
    assert!(near(moved[0], quad[0].x - 60.0, quad[0].y), "{moved:?}");
    assert!(app.objects().is_empty(), "dragging a corner draws nothing");
    // A corner never goes where it would fold the plane.
    let fold = (moved[2].x + 50.0, moved[2].y + 50.0);
    app.drag_doc((moved[0].x, moved[0].y), fold);
    let plane = app.state().draw_perspective.clone().unwrap();
    assert!(plane.map(page).is_some(), "still a plane");
    assert!(!near(plane.quad(page)[0], fold.0, fold.1), "{:?}", plane.quad(page));
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.state().draw_perspective, Some(plane), "moving the plane is not an undo step");

    // Ellipses and freehand lines go onto the plane too; the Text tool draws flat.
    app.key(Key::F7);
    app.drag_doc((150.0, 600.0), (250.0, 700.0));
    assert!(app.objects().last().unwrap().perspective.is_some());
    app.key(Key::F8);
    app.click_doc(100.0, 780.0);
    app.type_text("Flat");
    app.key(Key::Escape);
    assert!(matches!(app.objects().last().unwrap().shape, Shape::Text(_)));
    assert!(app.objects().last().unwrap().perspective.is_none());

    // Off again: shapes are drawn flat.
    app.menu(&["Object", "Perspective", "Draw in Perspective"]);
    assert!(app.state().draw_perspective.is_none());
    app.key(Key::F6);
    app.drag_doc((300.0, 100.0), (400.0, 200.0));
    assert!(app.objects().last().unwrap().perspective.is_none());
    assert_eq!(app.state().tool, Tool::Rectangle);
}

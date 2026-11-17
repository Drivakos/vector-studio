//! Object > Perspective > Add Perspective (corners dragged with the Shape tool), Object >
//! Copy Effect > Perspective From, Object > Clear Effect.

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

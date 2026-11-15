//! Join Curves (Object menu) and Convert Outline to Object (Cmd+Shift+Q).

use egui::{Key, Modifiers};
use vs_core::{Fill, Shape};

use crate::support::E2e;

fn bezier(app: &mut E2e, points: &[(f64, f64)]) {
    for &(x, y) in points {
        app.click_doc(x, y);
    }
    app.key(Key::Enter);
}

#[test]
fn join_curves_closes_two_open_paths_whose_ends_nearly_meet() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Bézier");
    bezier(&mut app, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);
    bezier(&mut app, &[(201.0, 201.0), (100.0, 200.0), (100.0, 101.0)]);
    assert_eq!(app.objects().len(), 2);
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.menu(&["Object", "Join Curves"]);

    assert_eq!(app.objects().len(), 1, "one curve is left");
    let obj = &app.objects()[0];
    assert_eq!(app.selection(), vec![obj.id]);
    assert!(obj.shape.is_closed(), "both gaps are under 1 mm, so the path closes");
    let Shape::Curve { path } = &obj.shape else { panic!("curve") };
    assert_eq!(vs_core::nodes::nodes(path).len(), 4, "the meeting ends become single nodes");
}

#[test]
fn convert_outline_to_object_turns_the_outline_into_a_filled_curve() {
    let mut app = E2e::new();
    let rect = app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Yellow");
    app.right_click("Swatch Red");
    app.fill("Outline width (property bar)", "20");
    let red = app.objects()[0].style.outline.as_ref().unwrap().color;

    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Q);
    assert_eq!(app.objects().len(), 2);
    let (inner, ring) = (&app.objects()[0], &app.objects()[1]);
    assert_eq!(inner.id, rect);
    assert_eq!(inner.style.outline, None, "the rectangle keeps only its fill");
    assert_eq!(ring.style.fill, Some(Fill::Solid(red)));
    assert_eq!(ring.style.outline, None);
    assert_eq!(app.selection(), vec![ring.id], "the new curve is selected");
    let b = app.bounds(1);
    assert!((b.x0 - 90.0).abs() < 0.01 && (b.x1 - 310.0).abs() < 0.01, "the ring covers the outline's width: {b:?}");

    // The canvas still looks the same: red edge, yellow middle.
    app.click_doc(600.0, 700.0);
    let edge = app.pixel(100.0, 200.0);
    assert!(edge[0] > 200 && edge[1] < 60, "edge is red: {edge:?}");
    let middle = app.pixel(200.0, 200.0);
    assert!(middle[0] > 200 && middle[1] > 200 && middle[2] < 60, "middle is yellow: {middle:?}");
}

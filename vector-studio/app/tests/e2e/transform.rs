//! Position, size, rotate and mirror: property bar, toolbar and Transform docker.

use vs_core::Unit;

use crate::support::{E2e, close, rect_close};

fn mm(v: f64) -> f64 {
    Unit::Millimeters.to_points(v)
}

#[test]
fn property_bar_moves_and_resizes_the_selection() {
    let mut app = E2e::new();
    app.draw_rect(mm(10.0), mm(10.0), mm(30.0), mm(20.0));
    app.fill("X: object position", "50");
    app.fill("Y: object position", "40");
    assert!(rect_close(app.bounds(0), mm(50.0), mm(40.0), mm(70.0), mm(50.0)), "{:?}", app.bounds(0));
    app.fill("Object width", "40");
    app.fill("Object height", "5");
    assert!(rect_close(app.bounds(0), mm(50.0), mm(40.0), mm(90.0), mm(45.0)), "{:?}", app.bounds(0));
    app.click("Object width: step up or down");
    assert!(app.bounds(0).width() != mm(40.0), "the stepper changes the width");
}

#[test]
fn rotate_and_mirror_from_the_property_bar_and_toolbar() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 140.0);
    app.click("Rotate 90° right (property bar)");
    let b = app.bounds(0);
    assert!(close(b.width(), 40.0) && close(b.height(), 100.0), "{b:?}");
    app.click("Rotate 90° left (property bar)");
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 140.0), "{:?}", app.bounds(0));

    let before = app.objects()[0].transform;
    app.click("Mirror horizontally (property bar)");
    assert!(app.objects()[0].transform.as_coeffs()[0] < 0.0, "x flipped");
    app.click("Mirror horizontally");
    let after = app.objects()[0].transform.as_coeffs();
    assert!(before.as_coeffs().iter().zip(after).all(|(a, b)| close(*a, b)), "the toolbar button flips it back: {after:?}");
    app.click("Mirror vertically");
    assert!(app.objects()[0].transform.as_coeffs()[3] < 0.0, "y flipped");
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 140.0), "mirroring keeps the bounds");
}

#[test]
fn transform_docker_position_proportional_size_and_rotation() {
    let mut app = E2e::new();
    app.draw_rect(mm(10.0), mm(10.0), mm(30.0), mm(20.0));
    app.docker("Transform");
    app.fill("X (transform)", "0");
    assert!(close(app.bounds(0).x0, 0.0), "{:?}", app.bounds(0));
    app.fill("Width (transform)", "40");
    let b = app.bounds(0);
    assert!(close(b.width(), mm(40.0)) && close(b.height(), mm(20.0)), "Proportional is on: {b:?}");

    app.click("Proportional");
    app.fill("Height (transform)", "5");
    let b = app.bounds(0);
    assert!(close(b.width(), mm(40.0)) && close(b.height(), mm(5.0)), "{b:?}");

    app.fill("Rotation angle", "90");
    app.click("Apply");
    let b = app.bounds(0);
    assert!(close(b.width(), mm(5.0)) && close(b.height(), mm(40.0)), "{b:?}");
    app.click("Mirror vertically (docker)");
    assert!(app.objects()[0].transform.determinant() < 0.0, "mirrored");
}

#[test]
fn free_transform_rotates_reflects_scales_and_skews_about_the_press_point() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.flyout_tool("Pick", "Free Transform");
    assert!(app.is_selected("Free rotation"), "rotation is the first mode");
    let undo = |app: &mut E2e| {
        app.shortcut(egui::Modifiers::COMMAND, egui::Key::Z);
        assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 200.0), "undone");
    };

    // Sweep a quarter turn around the top-left corner.
    app.stroke_doc(&[(100.0, 100.0), (200.0, 100.0), (100.0, 200.0)]);
    assert!(rect_close(app.bounds(0), 0.0, 100.0, 100.0, 200.0), "rotated 90° about the press point: {:?}", app.bounds(0));
    undo(&mut app);

    app.click("Free angle reflection");
    app.drag_doc((250.0, 50.0), (250.0, 300.0));
    assert!(rect_close(app.bounds(0), 300.0, 100.0, 400.0, 200.0), "mirrored across x = 250: {:?}", app.bounds(0));
    undo(&mut app);

    app.click("Free scale");
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 300.0, 300.0), "moving by the size doubles it: {:?}", app.bounds(0));
    undo(&mut app);

    app.click("Free skew");
    app.drag_doc((100.0, 200.0), (200.0, 205.0));
    assert!(rect_close(app.bounds(0), 0.0, 100.0, 200.0, 200.0), "slanted 45° along the bottom edge: {:?}", app.bounds(0));
    undo(&mut app);

    app.click_doc(500.0, 500.0);
    assert!(app.selection().is_empty(), "a click on empty space deselects");
    app.click_doc(100.0, 150.0);
    assert_eq!(app.selection().len(), 1, "a click on an object selects it");
}

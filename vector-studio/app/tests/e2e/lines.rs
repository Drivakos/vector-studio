//! 2-Point Line, Polyline, B-Spline, 3-Point Curve, 3-Point Rectangle and Ellipse, Spiral.

use egui::{Key, Modifiers};
use vs_core::kurbo::{ParamCurve, PathEl, Point, Shape as _};
use vs_core::{Object, Shape};

use crate::support::{E2e, close};

fn path(o: &Object) -> vs_core::kurbo::BezPath {
    match &o.shape {
        Shape::Curve { path } => path.clone(),
        s => panic!("not a curve: {s:?}"),
    }
}

#[test]
fn line_tools_draw_lines_polylines_splines_and_three_point_curves() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "2-Point Line");
    app.drag_doc((100.0, 100.0), (200.0, 150.0));
    let line = path(&app.objects()[0]);
    assert_eq!(line.elements().len(), 2);
    assert!(close(line.segments().next().unwrap().eval(1.0).y, 150.0));
    app.hold(Modifiers::COMMAND);
    app.drag_doc((100.0, 300.0), (200.0, 310.0));
    app.release_mods();
    let snapped = path(&app.objects()[1]).segments().next().unwrap().eval(1.0);
    assert!(close(snapped.y, 300.0), "Cmd keeps it level: {snapped:?}");

    app.flyout_tool("2-Point Line", "Polyline");
    for (x, y) in [(300.0, 100.0), (350.0, 100.0), (350.0, 150.0), (350.0, 150.0)] {
        app.click_doc(x, y);
    }
    let poly = path(&app.objects()[2]);
    assert_eq!(poly.elements().len(), 3, "three nodes; clicking the last one again finishes");
    assert!(poly.elements().iter().all(|e| matches!(e, PathEl::MoveTo(_) | PathEl::LineTo(_))), "straight segments only");

    app.flyout_tool("Polyline", "B-Spline");
    for (x, y) in [(100.0, 400.0), (150.0, 500.0), (200.0, 400.0)] {
        app.click_doc(x, y);
    }
    app.key(Key::Enter);
    let spline = path(&app.objects()[3]);
    let segs: Vec<_> = spline.segments().collect();
    assert!((segs[0].eval(0.0) - Point::new(100.0, 400.0)).hypot() < 1.0);
    assert!((segs.last().unwrap().eval(1.0) - Point::new(200.0, 400.0)).hypot() < 1.0);
    let b = spline.bounding_box();
    assert!(b.y1 > 430.0 && b.y1 < 495.0, "pulled towards the middle point, not through it: {b:?}");

    app.flyout_tool("B-Spline", "3-Point Curve");
    app.drag_doc((300.0, 300.0), (400.0, 300.0));
    assert_eq!(app.objects().len(), 4, "waits for the third point");
    app.click_doc(350.0, 340.0);
    let curve = path(&app.objects()[4]);
    let mid = curve.segments().next().unwrap().eval(0.5);
    assert!((mid - Point::new(350.0, 340.0)).hypot() < 1.0, "passes through the click: {mid:?}");
}

#[test]
fn three_point_shapes_and_spirals() {
    let mut app = E2e::new();
    app.flyout_tool("Rectangle", "3-Point Rectangle");
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    app.click_doc(100.0, 200.0);
    let rect = &app.objects()[0];
    let Shape::Rectangle { rect: r, .. } = rect.shape else { panic!("stays a rectangle: {:?}", rect.shape) };
    assert!((r.width() - 141.42).abs() < 0.1 && (r.height() - 70.71).abs() < 0.1, "{r:?}");
    let corner = rect.transform * Point::ORIGIN;
    assert!(close(corner.x, 100.0) && close(corner.y, 100.0), "the first edge starts at the drag: {corner:?}");

    app.flyout_tool("Ellipse", "3-Point Ellipse");
    app.drag_doc((300.0, 100.0), (400.0, 100.0));
    app.key(Key::Escape);
    app.click_doc(350.0, 150.0);
    assert_eq!(app.objects().len(), 1, "Esc dropped the first axis");
    app.drag_doc((300.0, 100.0), (400.0, 100.0));
    app.click_doc(350.0, 130.0);
    assert!(matches!(app.objects()[1].shape, Shape::Ellipse { .. }));
    let b = app.bounds(1);
    assert!(close(b.x0, 300.0) && close(b.x1, 400.0) && close(b.y0, 70.0) && close(b.y1, 130.0), "{b:?}");

    app.key(Key::A);
    app.fill("Spiral turns", "2");
    app.drag_doc((100.0, 400.0), (300.0, 600.0));
    let spiral = path(&app.objects()[2]);
    assert_eq!(spiral.segments().count(), 16, "8 segments per turn");
    let start = spiral.segments().next().unwrap().eval(0.0);
    assert!((start - Point::new(200.0, 500.0)).hypot() < 1.0, "starts in the middle: {start:?}");
}

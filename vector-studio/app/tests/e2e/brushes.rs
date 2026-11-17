//! Smooth, Roughen, Twirl and Attract and Repel brush tools (Shape tool flyout).

use egui::{Key, Modifiers};
use vs_core::kurbo::Shape as _;
use vs_core::Shape;

use crate::support::E2e;

fn nodes(app: &E2e) -> usize {
    app.objects()[0].world_path().elements().len()
}

#[test]
fn smooth_rounds_off_a_corner_under_the_brush() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    let area = app.objects()[0].world_path().area().abs();
    app.flyout_tool("Shape", "Smooth");
    assert_eq!(app.value("Brush size"), "7.056mm", "20 pt to start");
    assert_eq!(app.value("Smoothness"), "50%");
    app.fill("Smoothness", "100");
    // Scrub back and forth over the bottom-right corner.
    app.stroke_doc(&[(285.0, 185.0), (300.0, 200.0), (285.0, 185.0), (300.0, 200.0), (285.0, 185.0), (300.0, 200.0)]);

    let obj = &app.objects()[0];
    assert!(matches!(obj.shape, Shape::Curve { .. }), "the rectangle became a curve");
    let after = obj.world_path().area().abs();
    assert!(after < area - 5.0, "the corner is cut off: {area} → {after}");
    let b = obj.world_bounds();
    assert!((b.x0 - 100.0).abs() < 0.01 && (b.y0 - 100.0).abs() < 0.01, "the far corner stays: {b:?}");
    assert!(obj.world_path().segments().any(|s| matches!(s, vs_core::kurbo::PathSeg::Cubic(_))), "fitted back into curves");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(matches!(app.objects()[0].shape, Shape::Rectangle { .. }), "one undo step");
}

#[test]
fn roughen_adds_spikes_along_the_stroke_and_picks_the_object_under_it() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click_doc(500.0, 600.0);
    assert!(app.selection().is_empty());
    let before = nodes(&app);
    app.flyout_tool("Shape", "Roughen");
    app.fill("Spike size", "2");
    // Along the top edge, starting on it so the rectangle is picked.
    app.stroke_doc(&[(150.0, 100.0), (200.0, 100.0), (250.0, 100.0)]);

    assert_eq!(app.selection(), vec![app.objects()[0].id]);
    assert!(nodes(&app) > before + 20, "spikes: {} nodes", nodes(&app));
    let b = app.objects()[0].world_bounds();
    let spike = vs_core::Unit::Millimeters.to_points(2.0);
    assert!((b.y0 - (100.0 - spike)).abs() < 0.01, "spikes stand out of the top edge: {b:?}");
    assert!((b.y1 - 200.0).abs() < 0.01, "the bottom edge is untouched");
    let tops: Vec<f64> = app.objects()[0].world_path().elements().iter().filter_map(|e| e.end_point()).filter(|p| p.y < 99.0).map(|p| p.x).collect();
    assert!(tops.iter().any(|x| *x < 160.0) && tops.iter().any(|x| *x > 240.0), "spikes all along the stroke: {tops:?}");
    assert!(tops.iter().all(|x| *x > 135.0 && *x < 265.0), "and nowhere else: {tops:?}");
}

fn ends(app: &E2e) -> Vec<vs_core::kurbo::Point> {
    app.objects()[0].world_path().elements().iter().filter_map(|e| e.end_point()).collect()
}

#[test]
fn twirl_swirls_the_outline_while_held() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.flyout_tool("Shape", "Twirl");
    assert_eq!(app.value("Speed"), "50%");
    assert!(app.is_selected("Counterclockwise"));
    app.hold_doc(200.0, 100.0, 0.5);

    let obj = &app.objects()[0];
    assert!(matches!(obj.shape, Shape::Curve { .. }), "the rectangle became a curve");
    let points = ends(&app);
    // Counterclockwise on the page: right of the centre goes up, left of it down.
    assert!(points.iter().any(|p| p.x > 200.0 && p.y < 98.0), "{points:?}");
    assert!(points.iter().any(|p| p.x < 200.0 && p.y > 102.0), "{points:?}");
    let b = obj.world_bounds();
    assert!((b.x0 - 100.0).abs() < 0.01 && (b.x1 - 300.0).abs() < 0.01 && (b.y1 - 200.0).abs() < 0.01, "away from the brush stays: {b:?}");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(matches!(app.objects()[0].shape, Shape::Rectangle { .. }), "one undo step");

    app.click("Clockwise");
    app.hold_doc(200.0, 100.0, 0.5);
    let points = ends(&app);
    assert!(points.iter().any(|p| p.x > 200.0 && p.y > 102.0), "clockwise: right of the centre goes down: {points:?}");
}

#[test]
fn attract_pulls_the_outline_in_and_repel_pushes_it_away() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.flyout_tool("Shape", "Attract and Repel");
    app.fill("Brush size", "20");
    assert!(app.is_selected("Attract"));
    // Above the top edge: the edge is pulled up to the brush.
    app.hold_doc(200.0, 90.0, 0.5);
    let b = app.objects()[0].world_bounds();
    assert!(b.y0 < 96.0 && b.y0 >= 90.0 - 0.01, "pulled up towards the brush, not past it: {b:?}");
    assert!((b.y1 - 200.0).abs() < 0.01 && (b.x0 - 100.0).abs() < 0.01, "{b:?}");
    let pulled = app.objects()[0].world_path().area().abs();
    assert!(pulled > 200.0 * 100.0 + 50.0, "{pulled}");

    // Under the bottom edge, repelling: the edge is pushed up, into the rectangle.
    app.click("Repel");
    app.hold_doc(200.0, 210.0, 0.5);
    let pushed = app.objects()[0].world_path().area().abs();
    assert!(pushed < pulled - 50.0, "{pulled} → {pushed}");
    assert!((app.objects()[0].world_bounds().y1 - 200.0).abs() < 0.01, "nothing pushed out past the edge");
}

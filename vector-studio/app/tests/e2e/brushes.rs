//! Smooth and Roughen brush tools (Shape tool flyout).

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

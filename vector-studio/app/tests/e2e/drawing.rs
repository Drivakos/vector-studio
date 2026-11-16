//! Drawing tools: Rectangle, Ellipse, Polygon, Star, Freehand, Bézier, Pen.

use egui::{Key, Modifiers};
use vs_core::Shape;

use crate::support::{E2e, close, rect_close};

#[test]
fn rectangle_tool_draws_selects_and_shows_on_the_canvas() {
    let mut app = E2e::new();
    app.key(Key::F6);
    assert!(app.is_selected("Rectangle"), "F6 picks the Rectangle tool");
    app.drag_doc((100.0, 100.0), (200.0, 150.0));
    assert_eq!(app.objects().len(), 1);
    assert!(matches!(app.objects()[0].shape, Shape::Rectangle { .. }));
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 150.0), "{:?}", app.bounds(0));
    assert_eq!(app.selection(), app.ids(), "the new object is selected");

    // The palette fills it; the GPU canvas then shows that colour inside it.
    app.click("Swatch Red");
    let px = app.pixel(120.0, 110.0); // off the centre mark of the selection
    assert!(px[0] > 200 && px[1] < 60 && px[2] < 60, "canvas shows the red fill: {px:?}");
    assert_eq!(app.pixel(300.0, 300.0)[..3], [255, 255, 255], "page stays white elsewhere");
}

#[test]
fn rectangle_tool_with_cmd_draws_a_square_and_with_shift_from_the_centre() {
    let mut app = E2e::new();
    app.key(Key::F6);
    app.hold(Modifiers::COMMAND);
    app.drag_doc((100.0, 100.0), (200.0, 140.0));
    let b = app.bounds(0);
    assert!(close(b.width(), b.height()), "square: {b:?}");

    app.hold(Modifiers::SHIFT);
    app.drag_doc((300.0, 300.0), (340.0, 320.0));
    app.release_mods();
    assert!(rect_close(app.bounds(1), 260.0, 280.0, 340.0, 320.0), "{:?}", app.bounds(1));
}

#[test]
fn ellipse_tool_draws_an_ellipse() {
    let mut app = E2e::new();
    app.key(Key::F7);
    app.drag_doc((100.0, 100.0), (220.0, 160.0));
    assert!(matches!(app.objects()[0].shape, Shape::Ellipse { .. }));
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 220.0, 160.0), "{:?}", app.bounds(0));
}

#[test]
fn polygon_tool_uses_the_sides_from_the_property_bar() {
    let mut app = E2e::new();
    app.key(Key::Y);
    app.fill("Points or sides", "7");
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    assert!(matches!(app.objects()[0].shape, Shape::Polygon { sides: 7, .. }), "{:?}", app.objects()[0].shape);
}

#[test]
fn star_tool_uses_points_and_sharpness_from_the_property_bar() {
    let mut app = E2e::new();
    app.flyout_tool("Polygon", "Star");
    assert_eq!(app.state().tool, vector_studio::toolbox::Tool::Star);
    app.fill("Points or sides", "6");
    app.fill("Sharpness", "30");
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    match app.objects()[0].shape {
        Shape::Star { points, inner_ratio, .. } => {
            assert_eq!(points, 6);
            assert!(close(inner_ratio, 0.7), "{inner_ratio}");
        }
        ref s => panic!("not a star: {s:?}"),
    }
}

#[test]
fn freehand_draws_an_open_curve_and_closes_when_it_ends_on_the_start() {
    let mut app = E2e::new();
    app.key(Key::F5);
    app.stroke_doc(&[(100.0, 100.0), (130.0, 120.0), (160.0, 110.0), (200.0, 140.0)]);
    assert_eq!(app.objects().len(), 1);
    assert!(!app.objects()[0].shape.is_closed());

    app.stroke_doc(&[(300.0, 300.0), (360.0, 300.0), (360.0, 360.0), (300.0, 360.0), (300.5, 300.5)]);
    assert!(app.objects()[1].shape.is_closed(), "ending on the start closes the curve");
}

#[test]
fn bezier_tool_clicks_corners_and_enter_finishes() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Bézier");
    app.click_doc(100.0, 100.0);
    app.click_doc(200.0, 100.0);
    app.click_doc(200.0, 200.0);
    app.key(Key::Enter);
    assert_eq!(app.objects().len(), 1);
    let Shape::Curve { path } = &app.objects()[0].shape else { panic!("curve") };
    assert_eq!(vs_core::nodes::nodes(path).len(), 3);
    assert!(!app.objects()[0].shape.is_closed());
}

#[test]
fn pen_tool_closes_the_path_when_the_first_node_is_clicked() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Pen");
    app.click_doc(100.0, 100.0);
    app.click_doc(200.0, 100.0);
    app.click_doc(150.0, 180.0);
    app.click_doc(100.0, 100.0);
    assert_eq!(app.objects().len(), 1);
    assert!(app.objects()[0].shape.is_closed());
}

#[test]
fn escape_finishes_a_path_then_clears_the_selection_then_picks_the_pick_tool() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Bézier");
    app.click_doc(100.0, 100.0);
    app.click_doc(200.0, 100.0);
    app.key(Key::Escape);
    assert_eq!(app.objects().len(), 1, "Esc finishes the open path");
    assert!(!app.selection().is_empty());
    app.key(Key::Escape);
    assert!(app.selection().is_empty(), "second Esc clears the selection");
    app.key(Key::Escape);
    assert_eq!(app.state().tool, vector_studio::toolbox::Tool::Pick, "third Esc picks the Pick tool");
}

#[test]
fn graph_paper_draws_a_group_of_equal_rectangles() {
    let mut app = E2e::new();
    app.flyout_tool("Polygon", "Graph Paper");
    assert_eq!(app.value("Graph paper columns"), "4");
    assert_eq!(app.value("Graph paper rows"), "3");
    app.fill("Graph paper columns", "5");
    app.fill("Graph paper rows", "2");
    app.drag_doc((100.0, 100.0), (350.0, 200.0));
    assert_eq!(app.objects().len(), 1, "one group");
    let group = &app.objects()[0];
    let vs_core::Shape::Group { children } = &group.shape else { panic!("a group: {:?}", group.shape) };
    assert_eq!(children.len(), 10);
    assert!(children.iter().all(|c| matches!(c.shape, vs_core::Shape::Rectangle { .. })));
    let first = children[0].world_bounds();
    assert!(crate::support::rect_close(first, 100.0, 100.0, 150.0, 150.0), "{first:?}");
    assert_eq!(app.selection(), vec![group.id]);
    assert!(crate::support::rect_close(app.bounds(0), 100.0, 100.0, 350.0, 200.0), "{:?}", app.bounds(0));
}

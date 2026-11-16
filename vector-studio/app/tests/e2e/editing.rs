//! Pick tool (select, move, scale, rotate) and Shape tool (nodes, rounded corners).

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::Shape;

use crate::support::{E2e, close, rect_close};

#[test]
fn pick_selects_on_the_outline_not_inside_an_unfilled_shape() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click_doc(500.0, 500.0);
    assert!(app.selection().is_empty(), "empty click clears");
    app.click_doc(150.0, 150.0);
    assert!(app.selection().is_empty(), "inside an unfilled shape is not a hit");
    app.click_doc(100.0, 150.0);
    assert_eq!(app.selection(), vec![id]);
}

#[test]
fn shift_click_adds_and_marquee_selects_enclosed_objects() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    let b = app.draw_rect(300.0, 300.0, 350.0, 350.0);
    app.click_doc(100.0, 125.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(300.0, 325.0);
    app.release_mods();
    assert_eq!(app.selection(), vec![a, b]);

    app.click_doc(500.0, 600.0);
    app.drag_doc((50.0, 50.0), (200.0, 200.0));
    assert_eq!(app.selection(), vec![a], "marquee takes only what it fully encloses");
}

#[test]
fn dragging_moves_and_cmd_keeps_one_axis() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.drag_doc((100.0, 150.0), (130.0, 170.0));
    assert!(rect_close(app.bounds(0), 130.0, 120.0, 230.0, 220.0), "{:?}", app.bounds(0));

    app.hold(Modifiers::COMMAND);
    app.drag_doc((130.0, 150.0), (190.0, 160.0));
    app.release_mods();
    assert!(rect_close(app.bounds(0), 190.0, 120.0, 290.0, 220.0), "Cmd keeps the main axis: {:?}", app.bounds(0));
}

#[test]
fn corner_handle_scales_proportionally_and_side_handle_one_axis() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 150.0);
    let g = app.handle_gap();
    app.drag_doc((200.0 + g, 150.0 + g), (300.0 + g, 200.0 + g));
    let b = app.bounds(0);
    assert!(close(b.x0, 100.0) && close(b.y0, 100.0), "{b:?}");
    assert!((b.width() / b.height() - 2.0).abs() < 0.01, "proportions kept: {b:?}");

    let height = b.height();
    app.drag_doc((b.x1 + g, b.center().y), (b.x1 + g + 50.0, b.center().y + 30.0));
    assert!(close(app.bounds(0).height(), height));
    assert!(close(app.bounds(0).width(), b.width() + 50.0), "{:?}", app.bounds(0));
}

#[test]
fn clicking_a_selected_object_again_switches_to_rotate_handles() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    assert_eq!(app.state().tools.pick_mode, vector_studio::tools::PickMode::Scale);
    // The new rectangle is already selected: one more click shows rotate handles.
    app.click_doc(100.0, 150.0);
    assert_eq!(app.state().tools.pick_mode, vector_studio::tools::PickMode::Rotate);
    let g = app.handle_gap();
    // Drag the top-right rotate handle a quarter turn around the centre (150, 150).
    app.hold(Modifiers::COMMAND);
    app.drag_doc((200.0 + g, 100.0 - g), (200.0 + g, 200.0 + g));
    app.release_mods();
    let b = app.bounds(0);
    assert!(rect_close(b, 100.0, 100.0, 200.0, 200.0), "a square turned 90° covers the same box: {b:?}");
    let t = app.objects()[0].transform.as_coeffs();
    assert!(close(t[0].abs(), 0.0) && close(t[1].abs(), 1.0), "rotated 90° (Cmd snaps to 15°): {t:?}");
}

#[test]
fn shape_tool_moves_a_node_and_double_click_adds_and_deletes_nodes() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Convert to Curves (property bar)");
    app.key(Key::F10);
    assert_eq!(app.state().tool, Tool::Shape);
    let nodes = |app: &E2e| match &app.objects()[0].shape {
        Shape::Curve { path } => vs_core::nodes::nodes(path).len(),
        s => panic!("not a curve: {s:?}"),
    };
    assert_eq!(nodes(&app), 4);

    app.drag_doc((200.0, 200.0), (240.0, 230.0));
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 240.0, 230.0), "{:?}", app.bounds(0));

    app.double_click_doc(150.0, 100.0);
    assert_eq!(nodes(&app), 5, "double-click on a segment adds a node");
    app.double_click_doc(150.0, 100.0);
    assert_eq!(nodes(&app), 4, "double-click on a node deletes it");

    app.click_doc(100.0, 100.0);
    app.key(Key::Delete);
    assert_eq!(nodes(&app), 3, "Delete removes the selected node");
    assert_eq!(app.objects().len(), 1, "and not the object");
}

#[test]
fn double_clicking_an_object_with_the_pick_tool_shows_its_nodes() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.click("Convert to Curves (property bar)");
    app.click_doc(600.0, 700.0);
    app.double_click_doc(600.0, 700.0);
    assert_eq!(app.state().tool, Tool::Pick, "double-clicking empty page keeps the Pick tool");

    // 2 screen points in from the top-left corner: inside the fill, under the node's square.
    let off = 2.0 / app.state().viewport.zoom();
    let before = app.pixel(100.0 + off, 100.0 + off);
    assert!(before[0] > 200 && before[1] < 60, "no node shown yet, only the red fill: {before:?}");

    app.double_click_doc(100.0, 150.0);
    assert_eq!(app.state().tool, Tool::Shape, "double-click switches to the Shape tool");
    assert!(app.is_selected("Shape"), "the toolbox shows the Shape tool");
    assert_eq!(app.selection(), vec![id]);
    let after = app.pixel(100.0 + off, 100.0 + off);
    assert!(after[..3] == [255, 255, 255], "the node's white square is drawn on the corner: {after:?}");

    app.drag_doc((200.0, 200.0), (240.0, 230.0));
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 240.0, 230.0), "the node can be dragged: {:?}", app.bounds(0));
}

#[test]
fn shape_tool_rounds_rectangle_corners() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.key(Key::F10);
    app.drag_doc((100.0, 100.0), (120.0, 100.0));
    let Shape::Rectangle { corner_radius, .. } = app.objects()[0].shape else { panic!("rectangle") };
    assert!(close(corner_radius, 20.0), "{corner_radius}");
}

#[test]
fn shift_scales_from_the_centre_and_cmd_frees_the_proportions() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    let g = app.handle_gap();
    app.hold(Modifiers::SHIFT);
    app.drag_doc((200.0 + g, 200.0 + g), (250.0 + g, 250.0 + g));
    app.release_mods();
    assert!(rect_close(app.bounds(0), 50.0, 50.0, 250.0, 250.0), "about the centre: {:?}", app.bounds(0));

    app.hold(Modifiers::COMMAND);
    app.drag_doc((250.0 + g, 250.0 + g), (350.0 + g, 260.0 + g));
    app.release_mods();
    assert!(rect_close(app.bounds(0), 50.0, 50.0, 350.0, 260.0), "free proportions: {:?}", app.bounds(0));
}

#[test]
fn shape_tool_marquee_selects_nodes_and_drags_bezier_handles() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Convert to Curves (property bar)");
    app.key(Key::F10);
    app.drag_doc((90.0, 90.0), (210.0, 120.0));
    assert_eq!(app.state().tools.nodes.len(), 2, "the marquee takes the two top nodes");
    app.key(Key::Delete);
    let count = |app: &E2e| match &app.objects()[0].shape {
        Shape::Curve { path } => vs_core::nodes::nodes(path).len(),
        _ => 0,
    };
    assert_eq!(count(&app), 2);
}

#[test]
fn bezier_handles_can_be_dragged_with_the_shape_tool() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Bézier");
    app.click_doc(100.0, 300.0);
    app.drag_doc((200.0, 300.0), (250.0, 300.0)); // dragging makes a smooth node
    app.key(Key::Enter);
    let Shape::Curve { path } = &app.objects()[0].shape else { panic!("curve") };
    let node = vs_core::nodes::nodes(path)[1];
    let (_, handle) = node.handle_in.expect("a smooth node has handles");

    app.key(Key::F10);
    app.click_doc(200.0, 300.0);
    assert_eq!(app.state().tools.nodes.len(), 1, "node selected, handles shown");
    app.drag_doc((handle.x, handle.y), (handle.x, handle.y - 60.0));
    let Shape::Curve { path } = &app.objects()[0].shape else { panic!("curve") };
    let (_, moved) = vs_core::nodes::nodes(path)[1].handle_in.unwrap();
    assert!(close(moved.y, handle.y - 60.0), "{handle:?} → {moved:?}");
}

#[test]
fn freehand_pick_selects_what_the_loop_encloses_and_shift_adds() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    let b = app.draw_rect(300.0, 300.0, 350.0, 350.0);
    app.flyout_tool("Pick", "Freehand Pick");
    assert_eq!(app.state().tool, Tool::FreehandPick);

    // A triangle whose bounding box holds both rectangles, but only `a` lies inside it.
    app.stroke_doc(&[(60.0, 60.0), (230.0, 60.0), (400.0, 60.0), (230.0, 230.0), (60.0, 400.0), (60.0, 230.0), (60.0, 61.0)]);
    assert_eq!(app.selection(), vec![a], "only what lies wholly inside the loop");

    app.hold(Modifiers::SHIFT);
    app.stroke_doc(&[(280.0, 280.0), (370.0, 280.0), (370.0, 370.0), (280.0, 370.0), (280.0, 281.0)]);
    app.release_mods();
    assert_eq!(app.selection(), vec![a, b], "Shift adds to the selection");

    app.stroke_doc(&[(500.0, 500.0), (600.0, 500.0), (600.0, 600.0), (500.0, 501.0)]);
    assert!(app.selection().is_empty(), "a loop around nothing clears the selection");

    app.click_doc(100.0, 125.0);
    assert_eq!(app.selection(), vec![a], "clicking an object still selects it");
}

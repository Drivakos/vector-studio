//! Editing every kind of object's nodes: Convert to Curves (Cmd+Q, groups too), and the
//! Shape tool on ellipses (pie/arc), polygons and stars.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::controls::{Control, controls};
use vs_core::kurbo::Point;
use vs_core::{ArcKind, EllipseArc, Shape};

use crate::support::{E2e, close};

const CONVERT: &str = "Convert to Curves (property bar)";

/// Where `control` of object `i` sits on the page.
fn node(app: &E2e, i: usize, control: Control) -> Point {
    let obj = &app.objects()[i];
    let at = controls(&obj.shape).into_iter().find(|(c, _)| *c == control).expect("node exists").1;
    obj.transform * at
}

fn arc(app: &E2e) -> Option<EllipseArc> {
    match app.objects()[0].shape {
        Shape::Ellipse { arc, .. } => arc,
        ref s => panic!("not an ellipse: {s:?}"),
    }
}

#[test]
fn cmd_q_converts_to_curves_including_every_member_of_a_group() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.key(Key::F7);
    app.drag_doc((200.0, 100.0), (260.0, 150.0));
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.shortcut(Modifiers::COMMAND, Key::G);
    assert!(app.is_enabled(CONVERT), "a group of shapes can be converted");

    app.shortcut(Modifiers::COMMAND, Key::Q);
    let Shape::Group { children } = &app.objects()[0].shape else { panic!("still one group") };
    assert!(children.iter().all(|c| matches!(c.shape, Shape::Curve { .. })), "{children:?}");
    assert!(!app.is_enabled(CONVERT), "nothing left to convert");

    // Ungrouped, each curve's nodes are editable with the Shape tool.
    app.shortcut(Modifiers::COMMAND, Key::U);
    app.click_doc(600.0, 700.0);
    app.double_click_doc(100.0, 125.0);
    assert_eq!(app.state().tool, Tool::Shape);
    app.drag_doc((150.0, 150.0), (170.0, 180.0));
    let b = app.bounds(0);
    assert!(close(b.x1, 170.0) && close(b.y1, 180.0), "a converted node moved: {b:?}");
}

#[test]
fn shape_tool_turns_an_ellipse_into_a_pie_or_an_arc() {
    let mut app = E2e::new();
    app.key(Key::F7);
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    app.pick_tool();
    app.click("Swatch Red");
    app.double_click_doc(100.0, 150.0);
    assert_eq!(app.state().tool, Tool::Shape);
    assert!(app.is_selected("Whole ellipse"));

    // The single node sits at the top; drag it inside, to 3 o'clock.
    let top = node(&app, 0, Control::Ellipse { end: true });
    assert!(close(top.x, 150.0) && close(top.y, 100.0), "{top:?}");
    app.drag_doc((150.0, 100.0), (180.0, 150.0));
    let a = arc(&app).expect("cut");
    assert_eq!(a.kind, ArcKind::Pie, "inside makes a pie");
    assert!(close(a.start, 90.0) && close(a.end, 0.0), "{a:?}");
    assert!(app.is_selected("Pie"));

    app.click_doc(600.0, 700.0);
    let cut = app.pixel(175.0, 125.0);
    let kept = app.pixel(125.0, 175.0);
    assert_eq!(cut[..3], [255, 255, 255], "the top-right quarter is cut away: {cut:?}");
    assert!(kept[0] > 200 && kept[1] < 60, "the rest is filled: {kept:?}");

    // Drag the end node outside: an arc.
    app.click_doc(100.0, 150.0);
    app.drag_doc((200.0, 150.0), (240.0, 150.0));
    assert_eq!(arc(&app).unwrap().kind, ArcKind::Arc, "outside makes an arc");
    assert!(!app.objects()[0].shape.is_closed());

    app.click("Pie");
    assert_eq!(arc(&app).unwrap().kind, ArcKind::Pie);
    let before = arc(&app).unwrap();
    app.click("Change direction (property bar)");
    let after = arc(&app).unwrap();
    assert_eq!((after.start, after.end), (before.end, before.start), "the other part is kept");
    app.click("Whole ellipse");
    assert_eq!(arc(&app), None);
    assert!(!app.is_enabled("Change direction (property bar)"));

    app.pick_tool();
    assert_eq!(app.selection().len(), 1);
    app.click("Arc");
    assert_eq!(arc(&app).map(|a| a.kind), Some(ArcKind::Arc), "the Pick tool's property bar has the same buttons");
}

#[test]
fn shape_tool_resizes_and_turns_a_polygon_from_one_node() {
    let mut app = E2e::new();
    app.key(Key::Y);
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    app.key(Key::F10);
    let top = node(&app, 0, Control::Outer(0));
    assert!(close(top.x, 150.0) && close(top.y, 100.0), "{top:?}");

    // Cmd: twice the radius, no turn.
    app.hold(Modifiers::COMMAND);
    app.drag_doc((150.0, 100.0), (150.0, 50.0));
    app.release_mods();
    let b = app.bounds(0);
    assert!(close(b.center().x, 150.0) && close(b.y0, 50.0), "grew about the centre: {b:?}");
    let Shape::Polygon { sides: 5, .. } = app.objects()[0].shape else { panic!("still a 5-sided polygon") };

    // Without Cmd the node follows the pointer, turning every side with it.
    app.drag_doc((150.0, 50.0), (250.0, 150.0));
    let moved = node(&app, 0, Control::Outer(0));
    assert!(close(moved.x, 250.0) && close(moved.y, 150.0), "{moved:?}");
}

#[test]
fn shape_tool_changes_a_stars_sharpness_from_an_inner_node() {
    let mut app = E2e::new();
    app.flyout_tool("Polygon", "Star");
    app.drag_doc((100.0, 100.0), (200.0, 200.0));
    app.key(Key::F10);
    let inner = node(&app, 0, Control::Inner(0));
    let c = Point::new(150.0, 150.0);
    let to = c + (inner - c) * 0.5;
    app.drag_doc((inner.x, inner.y), (to.x, to.y));
    let Shape::Star { inner_ratio, .. } = app.objects()[0].shape else { panic!("star") };
    assert!((inner_ratio - 0.25).abs() < 0.01, "half the inner radius: {inner_ratio}");
}

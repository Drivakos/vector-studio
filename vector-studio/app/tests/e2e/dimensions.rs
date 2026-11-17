//! Dimension tools and the 3-Point Callout tool.

use egui::{Key, Modifiers};
use vs_core::kurbo::Point;
use vs_core::{Object, Shape, TextAlign};

use crate::support::E2e;

/// The parts of a dimension group: lines, arrowheads and the label's words.
fn parts(obj: &Object) -> (&Object, &Object, String) {
    assert_eq!(obj.name.as_deref(), Some("Dimension"));
    let Shape::Group { children } = &obj.shape else { panic!("a group: {:?}", obj.shape) };
    let curves: Vec<&Object> = children.iter().filter(|c| matches!(c.shape, Shape::Curve { .. })).collect();
    let words = children.iter().find_map(|c| match &c.shape {
        Shape::Text(t) => Some(t.content.clone()),
        _ => None,
    });
    let [lines, arrows] = curves[..] else { panic!("lines and arrowheads: {children:?}") };
    (lines, arrows, words.expect("a label"))
}

fn label(app: &E2e, points: f64) -> String {
    let unit = app.doc().display_unit;
    format!("{:.*} {}", unit.decimals(), unit.from_points(points), unit.suffix())
}

#[test]
fn parallel_dimension_measures_along_the_line_and_labels_it() {
    let mut app = E2e::new();
    app.click("Parallel Dimension");
    app.drag_doc((100.0, 100.0), (160.0, 180.0));
    assert!(app.objects().is_empty(), "waits for the click that places the line");
    // 10 pt to the left of the 100 pt line, square to it.
    app.click_doc(130.0 - 8.0, 140.0 + 6.0);

    assert_eq!(app.objects().len(), 1);
    let group = &app.objects()[0];
    assert_eq!(app.selection(), vec![group.id]);
    let (lines, arrows, words) = parts(group);
    assert_eq!(words, label(&app, 100.0));
    assert!(arrows.style.fill.is_some() && lines.style.outline.is_some());
    // The dimension line runs from (92, 106) to (152, 186).
    let path = lines.world_path();
    let ends: Vec<Point> = path.elements().iter().filter_map(|e| e.end_point()).collect();
    assert!(ends.iter().any(|p| (*p - Point::new(92.0, 106.0)).hypot() < 1e-3), "{ends:?}");
    assert!(ends.iter().any(|p| (*p - Point::new(152.0, 186.0)).hypot() < 1e-3), "{ends:?}");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects().is_empty(), "one undo step");
}

#[test]
fn horizontal_or_vertical_dimension_follows_the_pull() {
    let mut app = E2e::new();
    app.flyout_tool("Parallel Dimension", "Horizontal or Vertical Dimension");
    app.drag_doc((100.0, 300.0), (180.0, 340.0));
    app.click_doc(140.0, 280.0);
    let (_, _, words) = parts(&app.objects()[0]);
    assert_eq!(words, label(&app, 80.0), "above: the horizontal distance");

    app.drag_doc((300.0, 300.0), (380.0, 340.0));
    app.click_doc(420.0, 320.0);
    assert_eq!(app.objects().len(), 2);
    let (_, _, words) = parts(&app.objects()[1]);
    assert_eq!(words, label(&app, 40.0), "beside: the vertical distance");

    // Esc after the drag drops it.
    app.drag_doc((300.0, 500.0), (380.0, 540.0));
    app.key(Key::Escape);
    app.click_doc(420.0, 520.0);
    assert_eq!(app.objects().len(), 2);
}

#[test]
fn angular_dimension_measures_the_angle_on_the_side_of_the_arc() {
    let mut app = E2e::new();
    app.flyout_tool("Parallel Dimension", "Angular Dimension");
    // Vertex (200, 200), one arm to the right, the other straight up.
    app.drag_doc((200.0, 200.0), (300.0, 200.0));
    app.click_doc(200.0, 100.0);
    assert!(app.objects().is_empty(), "waits for the click that places the arc");
    app.click_doc(240.0, 160.0);

    assert_eq!(app.objects().len(), 1);
    let (lines, _, words) = parts(&app.objects()[0]);
    assert_eq!(words, "90.0°");
    // The arc passes through the click, 40√2 from the vertex.
    let b = lines.world_bounds();
    let r = 3200f64.sqrt();
    assert!((b.x1 - (200.0 + r + 2.0)).abs() < 0.01 && (b.y0 - (200.0 - r - 2.0)).abs() < 0.01, "{b:?}");

    // The same arms with the arc pulled round the other side: the outside angle.
    app.drag_doc((500.0, 200.0), (600.0, 200.0));
    app.click_doc(500.0, 100.0);
    app.click_doc(450.0, 250.0);
    let (_, _, words) = parts(&app.objects()[1]);
    assert_eq!(words, "270.0°");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 1, "one undo step");
}

#[test]
fn segment_dimension_measures_the_clicked_segment() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.flyout_tool("Parallel Dimension", "Segment Dimension");
    app.click_doc(400.0, 400.0);
    app.click_doc(400.0, 450.0);
    assert_eq!(app.objects().len(), 1, "a click off every segment does nothing");

    // The bottom side, then a dimension line 20 pt under it.
    app.click_doc(160.0, 200.0);
    app.click_doc(160.0, 220.0);
    assert_eq!(app.objects().len(), 2);
    let (lines, _, words) = parts(&app.objects()[1]);
    assert_eq!(words, label(&app, 200.0));
    let b = lines.world_bounds();
    assert!((b.x0 - 100.0).abs() < 1e-3 && (b.x1 - 300.0).abs() < 1e-3 && (b.y1 - 222.0).abs() < 1e-3, "{b:?}");

    // The right side.
    app.click_doc(300.0, 150.0);
    app.click_doc(330.0, 150.0);
    let (_, _, words) = parts(&app.objects()[2]);
    assert_eq!(words, label(&app, 100.0));
}

#[test]
fn callout_draws_a_leader_and_takes_typing() {
    let mut app = E2e::new();
    app.flyout_tool("Parallel Dimension", "3-Point Callout");
    app.drag_doc((100.0, 200.0), (150.0, 150.0));
    assert!(app.objects().is_empty(), "waits for the click that ends the leader");
    app.click_doc(220.0, 150.0);
    app.type_text("Note");
    app.key(Key::Escape);

    let [leader, text] = app.objects() else { panic!("a leader and text: {:?}", app.objects()) };
    assert_eq!(leader.name.as_deref(), Some("Callout"));
    let ends: Vec<Point> = leader.world_path().elements().iter().filter_map(|e| e.end_point()).collect();
    let want = [Point::new(100.0, 200.0), Point::new(150.0, 150.0), Point::new(220.0, 150.0)];
    assert!(ends.len() == 3 && ends.iter().zip(want).all(|(p, q)| (*p - q).hypot() < 1e-3), "{ends:?}");
    let Shape::Text(t) = &text.shape else { panic!("{:?}", text.shape) };
    assert_eq!(t.content, "Note");
    let b = text.world_bounds();
    assert!(b.x0 > 220.0 && b.x0 < 226.0 && b.y0 < 150.0 && b.y1 > 150.0, "just after the leader, level with it: {b:?}");

    // Bent back to the left, the text ends at the leader instead.
    app.drag_doc((400.0, 300.0), (350.0, 250.0));
    app.click_doc(280.0, 250.0);
    app.type_text("Left");
    app.key(Key::Escape);
    let text = &app.objects()[3];
    let Shape::Text(t) = &text.shape else { panic!("{:?}", text.shape) };
    assert_eq!(t.align, TextAlign::Right);
    assert!(text.world_bounds().x1 < 280.0 && text.world_bounds().x1 > 274.0, "{:?}", text.world_bounds());

    // A callout left without text keeps its leader.
    app.drag_doc((100.0, 500.0), (150.0, 450.0));
    app.click_doc(220.0, 450.0);
    app.key(Key::Escape);
    assert_eq!(app.objects().len(), 5);
    assert_eq!(app.objects()[4].name.as_deref(), Some("Callout"));
}

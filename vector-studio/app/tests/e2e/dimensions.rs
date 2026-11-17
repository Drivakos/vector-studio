//! Parallel and Horizontal or Vertical Dimension tools.

use egui::{Key, Modifiers};
use vs_core::kurbo::Point;
use vs_core::{Object, Shape};

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

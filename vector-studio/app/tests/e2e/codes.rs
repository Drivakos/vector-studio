//! Object > Insert > QR Code and Barcode.

use egui::{Key, Modifiers};
use vs_core::kurbo::Shape as _;
use vs_core::{Shape, Unit};

use crate::support::E2e;

#[test]
fn insert_qr_code_adds_a_black_code_in_the_middle_of_the_page() {
    let mut app = E2e::new();
    app.menu(&["Object", "Insert", "QR Code…"]);
    assert!(!app.is_enabled("OK (Insert QR Code)"), "nothing to encode yet");
    app.fill("QR code content", "https://example.com");
    assert_eq!(app.value("QR code size"), "30.0 mm");
    app.click("OK (Insert QR Code)");

    assert_eq!(app.objects().len(), 1);
    let obj = &app.objects()[0];
    assert_eq!(obj.name.as_deref(), Some("QR Code"));
    assert!(matches!(obj.shape, Shape::Curve { .. }));
    assert_eq!(app.selection(), vec![obj.id]);
    let r = obj.world_bounds();
    let side = Unit::Millimeters.to_points(30.0);
    assert!((r.width() - side).abs() < 1e-6 && (r.height() - side).abs() < 1e-6, "{r:?}");
    let page = app.doc().active_page().bounds();
    assert!((r.center() - page.center()).hypot() < 1e-6);
    assert!((obj.world_path().area().abs() - side * side).abs() > 1.0, "modules, not a solid square");

    // The top-left finder square: dark ring, light ring, dark centre (25 modules across).
    let module = side / 25.0;
    app.click_doc(10.0, 10.0);
    let px = |app: &mut E2e, mx: f64, my: f64| app.pixel(r.x0 + module * mx, r.y0 + module * my)[0];
    assert!(px(&mut app, 0.5, 3.5) < 60, "outer ring");
    assert!(px(&mut app, 1.5, 3.5) > 200, "light ring");
    assert!(px(&mut app, 3.5, 3.5) < 60, "centre");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects().is_empty(), "undoable");
}

#[test]
fn insert_barcode_adds_bars_and_the_text_under_them() {
    let mut app = E2e::new();
    app.menu(&["Object", "Insert", "Barcode…"]);
    app.fill("Barcode content", "Café");
    assert!(!app.is_enabled("OK (Insert Barcode)"), "é is not in Code 128");
    app.fill("Barcode content", "VS-123");
    app.click("OK (Insert Barcode)");

    assert_eq!(app.objects().len(), 1);
    let group = &app.objects()[0];
    assert_eq!(group.name.as_deref(), Some("Barcode"));
    assert_eq!(app.selection(), vec![group.id]);
    let Shape::Group { children } = &group.shape else { panic!("a group: {:?}", group.shape) };
    let bars = children.iter().find(|c| matches!(c.shape, Shape::Curve { .. })).expect("bars");
    let text = children.iter().find(|c| matches!(c.shape, Shape::Text(_))).expect("text");
    let Shape::Text(t) = &text.shape else { unreachable!() };
    assert_eq!(t.content, "VS-123");

    let b = bars.world_bounds();
    let modules = vs_core::codes::code128_widths("VS-123").unwrap().iter().sum::<u32>();
    let module = Unit::Millimeters.to_points(0.4);
    assert!((b.width() - f64::from(modules) * module).abs() < 1e-6, "{b:?}");
    assert!((b.height() - Unit::Millimeters.to_points(15.0)).abs() < 1e-6);
    let tb = text.world_bounds();
    assert!(tb.y0 > b.y1 && (tb.center().x - b.center().x).abs() < 1e-6, "centred under the bars: {tb:?} {b:?}");

    // Without the text: just the bars.
    app.menu(&["Object", "Insert", "Barcode…"]);
    app.fill("Barcode content", "12");
    app.click("Show text");
    app.click("OK (Insert Barcode)");
    assert_eq!(app.objects().len(), 2);
    assert!(matches!(app.objects()[1].shape, Shape::Curve { .. }));
}

//! Layers in the Objects docker: add, show/hide, lock, move objects, rename, delete.

use egui::Key;

use crate::support::E2e;

#[test]
fn layers_hide_lock_take_objects_and_can_be_renamed_and_deleted() {
    let mut app = E2e::new();
    let red = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.docker("Objects");
    app.click("New layer (docker)");
    let yellow = app.draw_rect(150.0, 100.0, 250.0, 200.0);
    app.click("Swatch Yellow");
    let page = app.doc().active_page();
    let (lower, upper) = (page.layers()[0].id, page.layers()[1].id);
    assert_eq!(page.layers()[1].name, "Layer 2");
    assert_eq!((page.object(red).unwrap().layer, page.object(yellow).unwrap().layer), (lower, upper));

    // Hidden: not drawn, not hit; the red square shows through.
    app.click("Visible: Layer 2");
    assert!(!app.is_selected("Visible: Layer 2"));
    let px = app.pixel(225.0, 150.0);
    assert!(px[0] > 200 && px[2] > 200, "hidden layer is not drawn: {px:?}");
    app.click_doc(175.0, 150.0);
    assert_eq!(app.selection(), vec![red]);
    app.click("Visible: Layer 2");

    // Locked: drawn, but clicks go through to nothing.
    app.click("Locked: Layer 1");
    app.click_doc(600.0, 700.0);
    app.click_doc(125.0, 180.0);
    assert!(app.selection().is_empty(), "locked layer objects cannot be picked");
    let px = app.pixel(125.0, 150.0);
    assert!(px[0] > 200 && px[1] < 60, "locked layer still draws: {px:?}");
    app.click("Locked: Layer 1");

    // Move the red square onto Layer 2, on top of the yellow one.
    app.click_doc(125.0, 150.0);
    app.click("Layer Layer 2");
    app.click("Move to active layer (docker)");
    assert_eq!(app.ids(), vec![yellow, red]);
    assert_eq!(app.doc().active_page().object(red).unwrap().layer, upper);

    // Layer 1 is empty now; delete it.
    app.click("Layer Layer 1");
    app.click("Delete layer (docker)");
    assert_eq!(app.doc().active_page().layers().len(), 1);
    assert_eq!(app.ids(), vec![yellow, red], "objects on other layers stay");
    assert!(!app.is_enabled("Delete layer (docker)"), "the last layer stays");

    app.double_click("Layer Layer 2");
    for _ in 0.."Layer 2".len() {
        app.key(Key::Backspace);
    }
    app.type_into("Layer name", "Artwork");
    assert_eq!(app.doc().active_page().layers()[0].name, "Artwork");
    app.shortcut(egui::Modifiers::COMMAND, Key::Z);
    assert_eq!(app.doc().active_page().layers()[0].name, "Layer 2", "renaming is undoable");
}

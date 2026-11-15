//! Objects docker: select, add to the selection, rename, order, delete.

use egui::Modifiers;
use egui_kittest::kittest::Queryable;

use crate::support::E2e;

#[test]
fn objects_docker_selects_renames_orders_and_deletes() {
    let mut app = E2e::new();
    let rect = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.key(egui::Key::F7);
    app.drag_doc((200.0, 100.0), (260.0, 150.0));
    app.pick_tool();
    let ellipse = app.ids()[1];
    app.docker("Objects");

    app.click("Object Rectangle");
    assert_eq!(app.selection(), vec![rect]);
    app.h.get_by_label("Object Ellipse").click_modifiers(Modifiers::SHIFT);
    app.run();
    assert_eq!(app.selection(), vec![rect, ellipse], "Shift-click adds");

    app.click("Object Rectangle");
    app.double_click("Object Rectangle");
    app.type_into("Object name", "Logo box");
    assert_eq!(app.objects()[0].name.as_deref(), Some("Logo box"), "double-click renames");
    assert!(app.exists("Object Logo box"));

    app.click("Object Logo box");
    app.click("Forward one (docker)");
    assert_eq!(app.ids(), vec![ellipse, rect]);
    app.click("Delete (docker)");
    assert_eq!(app.ids(), vec![ellipse]);
}

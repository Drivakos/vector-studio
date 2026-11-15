//! Group, Ungroup, Ungroup All, Lock, Unlock, Unlock All.

use egui::{Key, Modifiers};
use vs_core::Shape;

use crate::support::{E2e, rect_close};

const GROUP: &str = "Group (⌘G)";
const UNGROUP: &str = "Ungroup (⌘U)";

#[test]
fn toolbar_group_and_ungroup_follow_the_selection() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    assert!(!app.is_enabled(GROUP), "one object: nothing to group");
    let b = app.draw_rect(200.0, 100.0, 250.0, 150.0);
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert!(app.is_enabled(GROUP) && !app.is_enabled(UNGROUP));

    app.click(GROUP);
    assert_eq!(app.objects().len(), 1);
    assert!(matches!(app.objects()[0].shape, Shape::Group { .. }));
    assert_eq!(app.selection(), vec![app.ids()[0]], "the group is selected");
    assert!(app.is_enabled(UNGROUP));

    // Clicking a member selects the group; dragging moves both members.
    app.click_doc(600.0, 600.0);
    app.drag_doc((100.0, 125.0), (110.0, 135.0));
    assert!(rect_close(app.bounds(0), 110.0, 110.0, 260.0, 160.0), "{:?}", app.bounds(0));

    app.click(UNGROUP);
    assert_eq!(app.ids(), vec![a, b], "members come back in place, in order");
    assert_eq!(app.selection(), vec![a, b]);
    assert!(rect_close(app.bounds(1), 210.0, 110.0, 260.0, 160.0), "{:?}", app.bounds(1));
}

#[test]
fn cmd_g_and_cmd_u_and_ungroup_all_dissolve_nested_groups() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.draw_rect(200.0, 100.0, 250.0, 150.0);
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.shortcut(Modifiers::COMMAND, Key::G);
    app.draw_rect(300.0, 100.0, 350.0, 150.0);
    app.shortcut(Modifiers::COMMAND, Key::A);
    app.shortcut(Modifiers::COMMAND, Key::G);
    assert_eq!(app.objects().len(), 1, "a group holding a group and a rectangle");

    app.shortcut(Modifiers::COMMAND, Key::U);
    assert_eq!(app.objects().len(), 2, "Cmd+U ungroups one level");
    app.shortcut(Modifiers::COMMAND, Key::G);
    app.menu(&["Object", "Group", "Ungroup All"]);
    assert_eq!(app.objects().len(), 3, "Ungroup All dissolves nested groups");
    assert!(app.objects().iter().all(|o| matches!(o.shape, Shape::Rectangle { .. })));
}

#[test]
fn locked_objects_can_be_selected_but_not_moved_restyled_or_deleted() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    assert!(app.is_enabled("Lock") && !app.is_enabled("Unlock"));
    app.click("Lock");
    assert!(app.objects()[0].locked);
    assert!(!app.is_enabled("Lock") && app.is_enabled("Unlock"));

    app.drag_doc((100.0, 150.0), (150.0, 180.0));
    assert_eq!(app.selection(), vec![id], "still selectable");
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 200.0), "did not move");
    app.click("Swatch Red");
    assert_eq!(app.objects()[0].style.fill, None, "not restyled");
    app.key(Key::Delete);
    assert_eq!(app.objects().len(), 1, "not deleted");
    app.key(Key::ArrowRight);
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 200.0), "not nudged");

    app.click("Unlock");
    app.key(Key::Delete);
    assert!(app.objects().is_empty(), "deleted once unlocked");
}

#[test]
fn unlock_all_from_the_menu_unlocks_every_object() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.click("Lock");
    app.draw_rect(200.0, 100.0, 250.0, 150.0);
    app.click("Lock");
    app.click_doc(600.0, 600.0);
    app.menu(&["Object", "Lock", "Unlock All"]);
    assert!(app.objects().iter().all(|o| !o.locked));
}

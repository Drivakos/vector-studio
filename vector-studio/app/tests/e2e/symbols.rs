//! Object > Symmetry (live mirror, Edit Symmetry, lines, Remove Symmetry) and Object >
//! Symbol (New Symbol, Edit Symbol, Revert to Objects, Symbol Manager).

use egui::{Key, Modifiers};
use vs_core::Shape;

use crate::support::{E2e, rect_close};

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[1] < 60 && px[2] < 60
}

/// The palette's Blue, 0 113 188.
fn is_blue(px: [u8; 4]) -> bool {
    px[0] < 40 && (90..140).contains(&px[1]) && px[2] > 160
}

fn members(app: &E2e, i: usize) -> usize {
    match &app.objects()[i].shape {
        Shape::Group { children } => children.len(),
        _ => 0,
    }
}

#[test]
fn symmetry_mirrors_live_while_editing_and_can_be_removed() {
    let mut app = E2e::new();
    // Together they span x 100..300, so the mirror line is x = 200.
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.click("Swatch Red");
    app.draw_rect(290.0, 100.0, 300.0, 110.0);
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert!(!is_red(app.pixel(275.0, 125.0)));

    // Create New Symmetry opens it for editing: the members are out on the page and
    // their mirror is drawn as they change.
    app.menu(&["Object", "Symmetry", "Create New Symmetry"]);
    let group = app.ids()[0];
    assert_eq!(app.objects().len(), 3);
    assert_eq!(app.objects()[0].label(), "Symmetry");
    assert!(app.exists("Finish editing symmetry"));
    assert!(is_red(app.pixel(275.0, 125.0)), "the mirror shows while editing");
    app.click_doc(500.0, 500.0);
    app.drag_doc((125.0, 125.0), (125.0, 325.0));
    assert!(is_red(app.pixel(275.0, 325.0)), "the mirror follows the move");
    assert!(!is_red(app.pixel(275.0, 125.0)));
    app.fill("Symmetry lines", "2");

    app.click("Finish editing symmetry");
    assert_eq!(app.ids(), vec![group]);
    assert_eq!(app.selection(), vec![group]);
    assert!(!app.exists("Finish editing symmetry"));
    assert_eq!(app.objects()[0].symmetry.map(|s| s.lines), Some(2));
    assert!(is_red(app.pixel(275.0, 325.0)), "still mirrored after Finish");
    // Two lines: the second, horizontal, through y = 125 sends the square to y -100..-50.
    assert!(rect_close(app.bounds(0), 100.0, -100.0, 300.0, 350.0), "{:?}", app.bounds(0));

    // Edit again: Finish with nothing changed keeps it as it was.
    app.menu(&["Object", "Symmetry", "Edit Symmetry"]);
    assert_eq!(app.objects().len(), 3);
    assert!(app.state().group_edit.is_some());
    app.click("Finish editing symmetry");
    assert_eq!(app.ids(), vec![group]);

    app.menu(&["Object", "Symmetry", "Remove Symmetry"]);
    assert_eq!(app.objects()[0].label(), "Group of 8 objects", "two members, each in four places");
    assert!(is_red(app.pixel(275.0, 325.0)), "the copies stay");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects()[0].label(), "Symmetry", "undo");
    assert_eq!(app.objects()[0].leaves().len(), 8);
}

#[test]
fn symbol_instances_follow_edit_symbol_and_the_manager_inserts_renames_and_deletes() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.menu(&["Object", "Symbol", "New Symbol"]);
    let first = app.ids()[0];
    assert_eq!(app.objects().len(), 1);
    assert_eq!(app.objects()[0].label(), "Symbol");
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 200.0), "in place");
    assert_eq!(app.doc().symbols.len(), 1);
    app.docker("Objects");
    assert!(app.exists("Object Symbol 1 (symbol)"), "named by its symbol in the Objects docker");

    // The manager lists it with its use, and inserts another in the middle of the page.
    app.menu(&["Object", "Symbol", "Symbol Manager…"]);
    assert_eq!(app.value("Name of symbol 1 (Symbol Manager)"), "Symbol 1");
    assert!(app.exists("1 in use"));
    assert!(!app.is_enabled("Delete symbol 1 (Symbol Manager)"), "in use");
    app.click("Insert symbol 1 (Symbol Manager)");
    let center = app.doc().active_page().bounds().center();
    assert_eq!(app.objects().len(), 2);
    assert!(rect_close(app.bounds(1), center.x - 50.0, center.y - 50.0, center.x + 50.0, center.y + 50.0), "{:?}", app.bounds(1));
    assert!(app.exists("2 in use"));
    app.fill("Name of symbol 1 (Symbol Manager)", "Badge");
    assert_eq!(app.doc().symbols[0].name, "Badge");
    app.click("Close (Symbol Manager)");
    assert!(!app.exists("Insert symbol 1 (Symbol Manager)"));

    // Edit Symbol: the members come out; the change reaches every instance on Finish.
    app.click_doc(150.0, 150.0);
    app.menu(&["Object", "Symbol", "Edit Symbol"]);
    assert_eq!(app.objects().len(), 3);
    assert!(app.exists("Finish editing symbol"));
    app.click("Swatch Blue");
    assert!(is_red(app.pixel(center.x - 30.0, center.y - 30.0)), "the other instance waits for Finish");
    app.click("Finish editing symbol");
    assert_eq!(app.ids().len(), 2);
    assert_eq!(app.selection(), vec![first]);
    assert!(is_blue(app.pixel(120.0, 120.0)));
    assert!(is_blue(app.pixel(center.x - 30.0, center.y - 30.0)), "every instance changed");

    // Revert to Objects leaves a plain group; an unused symbol can then be deleted.
    app.menu(&["Object", "Symbol", "Revert to Objects"]);
    assert_eq!(app.objects()[0].label(), "Group of 1 objects");
    assert_eq!(members(&app, 0), 1);
    app.click_doc(center.x, center.y);
    app.key(Key::Delete);
    app.menu(&["Object", "Symbol", "Symbol Manager…"]);
    assert!(app.exists("0 in use"));
    app.click("Delete symbol 1 (Symbol Manager)");
    assert!(app.doc().symbols.is_empty());
    assert!(app.exists("No symbols yet. Object > Symbol > New Symbol makes one from the selection."));
}

//! Undo/redo, clipboard, duplicate, delete, select all, nudge.

use egui::{Event, Key, Modifiers};
use vs_core::Unit;

use crate::support::{E2e, close, rect_close};

#[test]
fn undo_and_redo_step_through_finished_gestures() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.drag_doc((100.0, 150.0), (150.0, 150.0));
    assert!(close(app.bounds(0).x0, 150.0));
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(close(app.bounds(0).x0, 100.0), "undo the move");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects().is_empty(), "undo the drawing");
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert_eq!(app.objects().len(), 1, "redo");
    app.menu(&["Edit", "Redo"]);
    assert!(close(app.bounds(0).x0, 150.0), "Edit › Redo");
    app.menu(&["Edit", "Undo"]);
    assert!(close(app.bounds(0).x0, 100.0), "Edit › Undo");
}

#[test]
fn copy_puts_svg_on_the_clipboard_and_paste_adds_a_copy() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.h.event(Event::Copy);
    app.run();
    let svg = app.state().system_clipboard_text().to_owned();
    assert!(svg.starts_with("<?xml") && svg.contains("<path "), "SVG on the system clipboard: {svg}");
    app.h.event(Event::Paste(svg));
    app.run();
    assert_eq!(app.objects().len(), 2, "pasting our own copy adds the object");
    assert_ne!(app.ids()[1], id);
    assert_eq!(app.selection(), vec![app.ids()[1]], "the pasted copy is selected");

    app.h.event(Event::Paste("some other text".into()));
    app.run();
    assert_eq!(app.objects().len(), 2, "foreign clipboard text is ignored");
}

#[test]
fn cut_and_paste_from_the_edit_menu() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.menu(&["Edit", "Cut"]);
    assert!(app.objects().is_empty());
    app.menu(&["Edit", "Paste"]);
    assert_eq!(app.objects().len(), 1);
    assert!(rect_close(app.bounds(0), 100.0, 100.0, 200.0, 200.0), "pasted in place");
}

#[test]
fn duplicate_uses_the_offset_from_the_property_bar() {
    let mut app = E2e::new();
    app.fill("Duplicate distance, horizontal", "10");
    app.fill("Duplicate distance, vertical", "-5");
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.shortcut(Modifiers::COMMAND, Key::D);
    assert_eq!(app.objects().len(), 2);
    let (dx, dy) = (Unit::Millimeters.to_points(10.0), Unit::Millimeters.to_points(-5.0));
    assert!(rect_close(app.bounds(1), 100.0 + dx, 100.0 + dy, 200.0 + dx, 200.0 + dy), "{:?}", app.bounds(1));
    assert_eq!(app.selection(), vec![app.ids()[1]]);
}

#[test]
fn delete_backspace_and_select_all() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.draw_rect(200.0, 100.0, 250.0, 150.0);
    app.draw_rect(300.0, 100.0, 350.0, 150.0);
    app.key(Key::Delete);
    assert_eq!(app.objects().len(), 2);
    app.key(Key::Backspace);
    assert!(app.selection().is_empty() && app.objects().len() == 2, "nothing selected: nothing deleted");
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert_eq!(app.selection().len(), 2);
    app.key(Key::Backspace);
    assert!(app.objects().is_empty());
}

#[test]
fn arrow_keys_nudge_by_the_nudge_distance_and_shift_by_ten_times() {
    let mut app = E2e::new();
    app.fill("Nudge distance (arrow keys)", "2");
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    let step = Unit::Millimeters.to_points(2.0);
    app.key(Key::ArrowRight);
    assert!(close(app.bounds(0).x0, 100.0 + step));
    app.key(Key::ArrowUp);
    assert!(close(app.bounds(0).y0, 100.0 - step));
    app.shortcut(Modifiers::SHIFT, Key::ArrowLeft);
    assert!(close(app.bounds(0).x0, 100.0 + step - 10.0 * step));
}

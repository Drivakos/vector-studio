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

#[test]
fn step_and_repeat_docker_makes_spaced_and_offset_copies() {
    let mut app = E2e::new();
    let unit = app.doc().display_unit;
    app.draw_rect(100.0, 100.0, 150.0, 130.0);
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    assert!(app.exists("Apply (Step and Repeat)"), "Cmd+Shift+D opens the docker");

    // Spacing between objects: each copy starts 10 units past the previous one's edge.
    app.fill("Number of copies (Step and Repeat)", "2");
    app.click("Horizontal spacing between objects");
    app.fill("Horizontal distance (Step and Repeat)", "10");
    app.click("Apply (Step and Repeat)");
    assert_eq!(app.objects().len(), 3);
    let gap = unit.to_points(10.0);
    assert!(close(app.bounds(1).x0, 150.0 + gap) && close(app.bounds(2).x0, 200.0 + 2.0 * gap), "{:?}", app.bounds(2));
    assert_eq!(app.selection(), vec![app.ids()[1], app.ids()[2]], "the copies are selected");

    // Offset: copies move by the distance itself.
    app.click_doc(100.0, 100.0);
    app.click("Horizontal offset");
    app.fill("Horizontal distance (Step and Repeat)", "0");
    app.fill("Vertical distance (Step and Repeat)", "20");
    app.fill("Number of copies (Step and Repeat)", "1");
    app.click("Apply (Step and Repeat)");
    assert_eq!(app.objects().len(), 4);
    assert!(close(app.bounds(3).x0, 100.0) && close(app.bounds(3).y0, 100.0 + unit.to_points(20.0)), "{:?}", app.bounds(3));
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 3, "one undo step per Apply");
}

#[test]
fn copy_properties_from_copies_the_chosen_properties_from_the_clicked_object() {
    let mut app = E2e::new();
    let source = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.right_click("Swatch Blue");
    app.fill("Outline width (property bar)", "8");
    let target = app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.click("Swatch Yellow");
    let style = |app: &E2e, id| app.objects().iter().find(|o| o.id == id).unwrap().style.clone();
    let source_style = style(&app, source);

    app.menu(&["Edit", "Copy Properties From…"]);
    app.click("Fill (Copy Properties)"); // leave the fill out
    app.click("OK (Copy Properties)");
    assert!(app.state().copy_properties.picking);
    app.click_doc(150.0, 150.0);
    assert!(!app.state().copy_properties.picking);
    assert_eq!(app.selection(), vec![target], "the click picks the source; the selection stays");
    let got = style(&app, target);
    assert_eq!(got.outline, source_style.outline, "outline pen and colour copied");
    assert_eq!(got.fill, Some(vs_core::Rgba::rgb(0xFF, 0xED, 0x00).into()), "fill left alone");

    app.menu(&["Edit", "Copy Properties From…"]);
    app.click("Fill (Copy Properties)");
    app.click("OK (Copy Properties)");
    app.key(Key::Escape);
    assert!(!app.state().copy_properties.picking, "Esc cancels");
    assert_eq!(app.selection(), vec![target]);
    app.menu(&["Edit", "Copy Properties From…"]);
    app.click("OK (Copy Properties)");
    app.click_doc(150.0, 150.0);
    assert_eq!(style(&app, target).fill, source_style.fill, "now the fill too");
}

#[test]
fn select_all_nodes_from_the_menu_and_cmd_a_with_the_shape_tool() {
    let mut app = E2e::new();
    app.flyout_tool("Freehand", "Bézier");
    for (x, y) in [(100.0, 100.0), (200.0, 100.0), (200.0, 200.0), (100.0, 200.0)] {
        app.click_doc(x, y);
    }
    app.key(Key::Enter);
    // The new curve is selected; with the Shape tool, click one node, then Cmd+A.
    app.key(Key::F10);
    assert_eq!(app.state().tool, vector_studio::toolbox::Tool::Shape);
    app.click_doc(200.0, 200.0);
    assert_eq!(app.state().tools.nodes.len(), 1);
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert_eq!(app.state().tools.nodes.len(), 4, "Cmd+A in the Shape tool selects every node");

    // From the Pick tool, Edit › Select All › Nodes switches to the Shape tool.
    app.pick_tool();
    app.click_doc(150.0, 100.0);
    app.menu(&["Edit", "Select All", "Nodes"]);
    assert_eq!(app.state().tool, vector_studio::toolbox::Tool::Shape, "switches to the Shape tool");
    assert_eq!(app.state().tools.nodes.len(), 4);
    assert_eq!(app.objects().len(), 1);
}

#[test]
fn repeat_does_the_last_move_rotation_or_command_again() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 120.0);
    app.drag_doc((100.0, 110.0), (150.0, 110.0));
    let b = app.bounds(0);
    assert!(close(b.x0, 150.0), "{b:?}");
    app.shortcut(egui::Modifiers::COMMAND, egui::Key::R);
    let b = app.bounds(0);
    assert!(close(b.x0, 200.0) && close(b.y0, 100.0), "moved the same distance again: {b:?}");

    app.click("Rotate 90° left (property bar)");
    let once = app.bounds(0);
    assert!(close(once.width(), 20.0), "{once:?}");
    app.shortcut(egui::Modifiers::COMMAND, egui::Key::R);
    let twice = app.bounds(0);
    assert!(close(twice.width(), 50.0) && close(twice.center().x, once.center().x), "turned again about its centre: {twice:?}");

    app.shortcut(egui::Modifiers::COMMAND, egui::Key::D);
    app.shortcut(egui::Modifiers::COMMAND, egui::Key::R);
    assert_eq!(app.objects().len(), 3, "Duplicate ran again");
}

#[test]
fn clones_follow_their_master_until_edited_themselves() {
    let mut app = E2e::new();
    let master = app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.click("Swatch Red");
    app.menu(&["Edit", "Clone"]);
    assert_eq!(app.objects().len(), 2);
    let clone = app.ids()[1];
    assert_eq!(app.selection(), vec![clone]);
    assert_eq!(app.objects()[1].clone_of, Some(master));
    assert_eq!(app.objects()[1].label(), "Clone Rectangle");
    let offset = app.bounds(1).x0 - app.bounds(0).x0;
    let (cx, cy) = (app.bounds(1).center().x, app.bounds(1).center().y);
    assert!(offset.abs() > 1.0, "placed like a duplicate");

    // The master's new fill shows on the clone.
    app.click_doc(103.0, 103.0);
    assert_eq!(app.selection(), vec![master]);
    app.click("Swatch Yellow");
    let px = app.pixel(cx + 20.0, cy + 20.0);
    assert!(px[0] > 200 && px[1] > 200 && px[2] < 60, "the clone turned yellow: {px:?}");

    // Its own fill breaks the link.
    app.click_doc(cx + 20.0, cy + 20.0);
    assert_eq!(app.selection(), vec![clone]);
    app.click("Swatch Red");
    assert_eq!(app.objects()[1].clone_of, None);
    app.click_doc(103.0, 103.0);
    app.click("Swatch Yellow");
    let px = app.pixel(cx + 20.0, cy + 20.0);
    assert!(px[0] > 200 && px[1] < 60, "stays red: {px:?}");
}

//! Pages: page navigator, Pages docker, page size and orientation, Layout menu.

use egui::Key;
use vs_core::{Orientation, PagePreset};

use crate::support::E2e;

fn active(app: &E2e) -> usize {
    app.doc().active_page_index()
}

#[test]
fn page_navigator_inserts_and_moves_between_pages() {
    let mut app = E2e::new();
    let first = app.doc().pages()[0].id;
    app.click("Insert page after");
    assert_eq!((app.doc().page_count(), active(&app)), (2, 1));
    app.click("Insert page before");
    assert_eq!((app.doc().page_count(), active(&app)), (3, 1), "inserted before page 2");
    assert_eq!(app.doc().pages()[0].id, first);

    app.click("First page");
    assert_eq!(active(&app), 0);
    assert!(!app.is_enabled("Previous page"));
    app.click("Next page");
    assert_eq!(active(&app), 1);
    app.click("Last page");
    assert_eq!(active(&app), 2);
    app.click("Previous page");
    assert_eq!(active(&app), 1);
    app.click("Page tab Page 1");
    assert_eq!(active(&app), 0);
    app.key(Key::PageDown);
    assert_eq!(active(&app), 1, "PageDown: next page");
    app.key(Key::PageUp);
    assert_eq!(active(&app), 0, "PageUp: previous page");
}

#[test]
fn objects_belong_to_their_page() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.click("Insert page after");
    assert!(app.objects().is_empty(), "the new page is empty");
    app.click("Page tab Page 1");
    assert_eq!(app.objects().len(), 1);
}

#[test]
fn pages_docker_duplicates_renames_and_deletes() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.docker("Pages");
    app.click("Duplicate page (docker)");
    assert_eq!(app.doc().page_count(), 2);
    assert_eq!(app.doc().pages()[1].objects.len(), 1, "the copy has the objects");
    assert_ne!(app.doc().pages()[1].objects[0].id, app.doc().pages()[0].objects[0].id, "with new ids");

    app.double_click("Page 2");
    app.type_into("Page name", "Back");
    assert_eq!(app.doc().page_label(1), "Back", "double-click renames");
    assert!(app.exists("Page tab Back"));

    app.click("Delete page (docker)");
    assert_eq!(app.doc().page_count(), 1);
    assert!(!app.is_enabled("Delete page (docker)"), "the last page cannot be deleted");
    app.click("Insert page (docker)");
    assert_eq!(app.doc().page_count(), 2);
}

#[test]
fn page_size_preset_orientation_and_width_from_the_property_bar() {
    let mut app = E2e::new();
    app.click("Page size");
    app.click("Letter");
    assert_eq!(PagePreset::matching(app.doc().active_page().size).map(|p| p.name), Some("Letter"));
    app.click("Landscape");
    assert_eq!(app.doc().active_page().orientation(), Orientation::Landscape);
    app.menu(&["Layout", "Switch Page Orientation"]);
    assert_eq!(app.doc().active_page().orientation(), Orientation::Portrait);
    app.fill("Page width", "100");
    let w = vs_core::Unit::Millimeters.from_points(app.doc().active_page().size.width);
    assert!((w - 100.0).abs() < 0.01, "{w}");
    assert_eq!(app.value("Page size").as_str(), "Custom");
}

#[test]
fn layout_menu_inserts_duplicates_and_deletes_pages() {
    let mut app = E2e::new();
    app.menu(&["Layout", "Insert Page"]);
    app.menu(&["Layout", "Duplicate Page"]);
    assert_eq!(app.doc().page_count(), 3);
    app.menu(&["Layout", "Delete Page"]);
    assert_eq!(app.doc().page_count(), 2);
}

#[test]
fn layout_menu_renames_the_page_and_goes_to_a_page() {
    let mut app = E2e::new();
    app.click("Insert page after");
    app.click("Insert page after");
    app.click("First page");

    app.menu(&["Layout", "Rename Page…"]);
    assert!(app.is_selected("Pages docker"), "renaming happens in the Pages docker");
    app.type_into("Page name", "Cover");
    assert_eq!(app.doc().pages()[0].name, "Cover");
    assert!(app.exists("Page tab Cover"));

    app.menu(&["Layout", "Go to Page…"]);
    assert_eq!(app.value("Page number (go to page)"), "1", "starts at the active page");
    app.fill("Page number (go to page)", "3");
    app.click("Go");
    assert_eq!(active(&app), 2);
    assert!(!app.exists("Page number (go to page)"), "the window closes");

    app.menu(&["Layout", "Go to Page…"]);
    app.fill("Page number (go to page)", "9");
    assert_eq!(app.value("Page number (go to page)"), "3", "limited to the pages there are");
    app.click("Cancel");
    assert_eq!(active(&app), 2);
}

fn text_content(app: &E2e, i: usize) -> String {
    match &app.objects()[i].shape {
        vs_core::Shape::Text(t) => t.content.clone(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn page_numbers_show_and_follow_their_page() {
    let mut app = E2e::new();
    app.click("Insert page after");
    app.menu(&["Layout", "Insert Page Number"]);
    assert_eq!(app.objects().len(), 1);
    assert_eq!(app.selection(), vec![app.objects()[0].id], "the new page number is selected");
    assert_eq!(text_content(&app, 0), "2");
    let size = app.doc().active_page().size;
    let b = app.bounds(0);
    assert!((b.center().x - size.width / 2.0).abs() < 1.0 && b.y1 > size.height - 40.0, "centred at the bottom: {b:?}");

    app.click("Insert page before");
    app.click("Next page");
    assert_eq!(text_content(&app, 0), "3", "renumbered when a page is added before it");

    // Typing into it makes it plain text, which stays as typed.
    app.key(Key::F8);
    let c = app.bounds(0).center();
    app.click_doc(c.x, c.y);
    app.type_text("0");
    app.key(Key::Escape);
    let typed = text_content(&app, 0);
    assert_eq!(typed.len(), 2, "{typed}");
    app.click("First page");
    app.menu(&["Layout", "Delete Page"]);
    app.click("Last page");
    assert_eq!(app.doc().active_page_index(), 1);
    assert_eq!(text_content(&app, 0), typed, "no longer a page number");
}

#[test]
fn page_background_shows_exports_when_asked_and_undoes() {
    let dir = crate::support::temp_dir("background");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.menu(&["Layout", "Page Background…"]);
    assert!(app.is_selected("No background"));
    app.click("Solid colour");
    app.fill("Background colour (hex)", "#ffcc00");
    app.click("OK");
    assert!(!app.exists("Background colour (hex)"));
    let paper = app.pixel(300.0, 400.0);
    assert_eq!(&paper[..3], &[0xff, 0xcc, 0x00], "the page shows the background");
    assert!(app.state().history.is_dirty(), "an unsaved change");

    let export_svg = |app: &mut E2e, file: &str| {
        app.shortcut(egui::Modifiers::COMMAND, Key::E);
        app.click("Export format");
        app.click("SVG - Scalable Vector Graphics");
        app.script.borrow_mut().save_paths.push_back(dir.join(file));
        app.click("Export…");
        std::fs::read_to_string(dir.join(file)).unwrap()
    };
    let svg = export_svg(&mut app, "with.svg");
    assert!(svg.contains(r##"<rect width="595.276" height="841.89" fill="#ffcc00"/>"##), "{svg}");

    app.menu(&["Layout", "Page Background…"]);
    assert_eq!(app.value("Background colour (hex)"), "#ffcc00");
    app.click("Print and export background");
    app.click("OK");
    assert_eq!(app.doc().background.map(|b| b.print_and_export), Some(false));
    let svg = export_svg(&mut app, "screen-only.svg");
    assert!(!svg.contains("<rect"), "shown on screen only: {svg}");
    assert_eq!(&app.pixel(300.0, 400.0)[..3], &[0xff, 0xcc, 0x00]);

    // Undo: printing back on, then the white Solid colour picked first, then none.
    let undo = |app: &mut E2e| {
        app.shortcut(egui::Modifiers::COMMAND, Key::Z);
        app.doc().background.map(|b| (b.color.to_hex(), b.print_and_export))
    };
    assert_eq!(undo(&mut app), Some(("#ffcc00".into(), true)));
    assert_eq!(undo(&mut app), Some(("#ffffff".into(), true)));
    assert_eq!(undo(&mut app), None, "each change is an undo step");
    assert_eq!(&app.pixel(300.0, 400.0)[..3], &[0xff, 0xff, 0xff]);
}

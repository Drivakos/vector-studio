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

//! CorelDRAW files: File > Open and File > Import read `.cdr` and `.cmx` through libcdr.
//! The files are written by `vs_io::cdr_testing` (and `crates/io/testdata`), so the tests
//! need no CorelDRAW file of anyone else's.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use vector_studio::Startup;
use vector_studio::files::Unsaved;
use vs_core::{Fill, Rgba, Shape};
use vs_io::cdr_testing::{Page, Rect, cdr_file};

use crate::support::{E2e, temp_dir};

/// A copy of the two-page test file (4 × 3 in with a red rectangle; 6 × 2 in with a green
/// and a blue one) named `name` in a fresh folder.
fn two_pages(test: &str, name: &str) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/io/testdata/two-pages.cdr");
    let path = temp_dir(test).join(name);
    std::fs::copy(fixture, &path).unwrap();
    path
}

fn near(px: [u8; 4], c: [u8; 3]) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 6)
}

#[test]
fn open_turns_each_coreldraw_page_into_a_page_and_save_asks_for_a_new_file() {
    if !vs_io::CAN_READ_CDR {
        return;
    }
    let path = two_pages("corel-open", "Poster.cdr");
    let original = std::fs::read(&path).unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND, Key::O);

    let offered = app.script.borrow().offered.last().cloned().unwrap();
    assert!(offered.iter().any(|e| e == "cdr") && offered.iter().any(|e| e == "cmx"), "Open offers Corel files: {offered:?}");
    assert!(app.script.borrow().errors.is_empty(), "{:?}", app.script.borrow().errors);
    assert_eq!(app.doc().title, "Poster");
    assert_eq!(app.doc().page_count(), 2);
    assert_eq!(app.state().file_path, None, "not saved over the CorelDRAW file");
    let sizes: Vec<_> = app.doc().pages().iter().map(|p| (p.size.width.round(), p.size.height.round())).collect();
    assert_eq!(sizes, [(288.0, 216.0), (432.0, 144.0)], "4 × 3 and 6 × 2 inches");

    let red = app.objects()[0].clone();
    assert!(matches!(red.shape, Shape::Curve { .. }));
    assert_eq!(red.style.fill, Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
    let b = red.world_bounds();
    assert!((b.x0 - 72.0).abs() < 0.01 && (b.y0 - 36.0).abs() < 0.01 && (b.x1 - 216.0).abs() < 0.01 && (b.y1 - 108.0).abs() < 0.01, "{b:?}");
    assert!(near(app.pixel(144.0, 72.0), [255, 0, 0]), "drawn: {:?}", app.pixel(144.0, 72.0));
    assert!(near(app.pixel(30.0, 180.0), [255, 255, 255]), "the rest of the page is paper");

    // Every object can be picked and edited on its own.
    app.click_doc(144.0, 72.0);
    assert_eq!(app.selection(), vec![red.id]);

    let saved = temp_dir("corel-open-save").join("Poster.vstudio");
    app.script.borrow_mut().save_paths.push_back(saved.clone());
    app.shortcut(Modifiers::COMMAND, Key::S);
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Save Drawing"));
    assert_eq!(vs_io::open_document(&saved).unwrap().page_count(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), original, "the CorelDRAW file is left as it was");

    let app = E2e::with_startup(Startup { native_menu: false, open: Some(path) });
    assert_eq!(app.doc().page_count(), 2, "vector-studio <file.cdr> opens it");
}

#[test]
fn a_coreldraw_file_dropped_on_the_window_opens() {
    if !vs_io::CAN_READ_CDR {
        return;
    }
    let path = two_pages("corel-drop", "Poster.cdr");
    let mut app = E2e::new();
    app.drop_files(&[path]);
    assert!(app.script.borrow().errors.is_empty(), "{:?}", app.script.borrow().errors);
    assert_eq!(app.doc().title, "Poster");
    assert_eq!(app.doc().page_count(), 2);
    assert_eq!(app.state().file_path, None);
}

#[test]
fn import_places_a_coreldraw_page_as_one_selected_group() {
    if !vs_io::CAN_READ_CDR {
        return;
    }
    let path = temp_dir("corel-import").join("Badge.cdr");
    let rects = vec![
        Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0, rgb: 0x0000ff },
        Rect { x: -1.0, y: -1.0, width: 1.0, height: 1.0, rgb: 0x00ff00 },
    ];
    std::fs::write(&path, cdr_file(&[Page { width: 4.0, height: 4.0, rects }])).unwrap();
    let mut app = E2e::new();
    app.draw_rect(10.0, 10.0, 40.0, 40.0);
    app.script.borrow_mut().open_paths.push_back(path);
    app.shortcut(Modifiers::COMMAND, Key::I);

    let offered = app.script.borrow().offered.last().cloned().unwrap();
    assert!(offered.iter().any(|e| e == "cdr"), "Import offers Corel files: {offered:?}");
    assert_eq!(app.doc().page_count(), 1);
    assert_eq!(app.objects().len(), 2, "added to the drawing");
    let group = app.objects()[1].clone();
    assert_eq!(app.selection(), vec![group.id]);
    let Shape::Group { children } = &group.shape else { panic!("a group: {:?}", group.shape) };
    let fills: Vec<_> = children.iter().map(|c| c.style.fill.clone()).collect();
    assert_eq!(fills, [Some(Fill::Solid(Rgba::rgb(0, 255, 0))), Some(Fill::Solid(Rgba::rgb(0, 0, 255)))]);
    // Two 1-inch squares touching at a corner: 144 pt across, in the middle of the page.
    let b = group.world_bounds();
    let page = app.doc().active_page().bounds();
    assert!((b.width() - 144.0).abs() < 0.01 && (b.height() - 144.0).abs() < 0.01, "{b:?}");
    assert!((b.center() - page.center()).hypot() < 0.01, "{b:?} centred on {page:?}");
}

#[test]
fn a_file_that_is_not_coreldraw_cannot_be_opened() {
    let path = temp_dir("corel-bad").join("notes.cdr");
    std::fs::write(&path, "just some text").unwrap();
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.script.borrow_mut().unsaved.push_back(Unsaved::Discard);
    app.script.borrow_mut().open_paths.push_back(path);
    app.menu(&["File", "Open…"]);
    let errors = app.script.borrow().errors.clone();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let why = if vs_io::CAN_READ_CDR { "not a CorelDRAW or CMX file" } else { "without libcdr" };
    assert!(errors[0].starts_with("Could not open") && errors[0].contains(why), "{errors:?}");
    assert_eq!(app.objects().len(), 1, "the drawing stays");
    assert_eq!(app.doc().title, "Untitled-1");
}

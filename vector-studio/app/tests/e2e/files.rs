//! New, Open, Save, Save As, Export, the unsaved-changes prompt, opening from the command line.

use egui::{Key, Modifiers};
use vector_studio::Startup;
use vector_studio::files::Unsaved;

use crate::support::{E2e, temp_dir};

#[test]
fn save_as_then_save_writes_the_file_and_clears_the_unsaved_mark() {
    let dir = temp_dir("save");
    let path = dir.join("logo.vstudio");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    assert!(app.state().history.is_dirty());

    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert_eq!(app.script.borrow().shown, vec!["Save Drawing"], "Cmd+Shift+S asks where to save");
    assert!(path.exists());
    assert!(!app.state().history.is_dirty());
    assert_eq!(app.doc().title, "logo");
    assert_eq!(vs_io::open_document(&path).unwrap(), *app.doc());

    app.key(Key::ArrowRight);
    app.shortcut(Modifiers::COMMAND, Key::S);
    assert_eq!(app.script.borrow().shown.len(), 1, "Cmd+S saves to the same file without asking");
    assert_eq!(vs_io::open_document(&path).unwrap(), *app.doc());
}

#[test]
fn open_replaces_the_document_and_command_line_files_open_at_start() {
    let dir = temp_dir("open");
    let path = dir.join("two.vstudio");
    let mut doc = vs_core::Document::new("two");
    doc.insert_page_after(0).unwrap();
    vs_io::save_document(&doc, &path).unwrap();

    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND, Key::O);
    assert_eq!(app.doc().page_count(), 2);
    assert_eq!(app.state().file_path.as_deref(), Some(path.as_path()));

    let app = E2e::with_startup(Startup { native_menu: false, open: Some(path) });
    assert_eq!(app.doc().page_count(), 2, "vector-studio <file> opens it");
}

#[test]
fn a_damaged_file_shows_an_error_and_keeps_the_document() {
    let dir = temp_dir("damaged");
    let path = dir.join("bad.vstudio");
    std::fs::write(&path, "not json").unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(path);
    app.menu(&["File", "Open…"]);
    assert_eq!(app.script.borrow().errors.len(), 1);
    assert!(app.script.borrow().errors[0].starts_with("Could not open"), "{:?}", app.script.borrow().errors);
    assert_eq!(app.doc().title, "Untitled-1");
}

#[test]
fn new_with_unsaved_changes_asks_and_respects_cancel_discard_and_save() {
    let dir = temp_dir("new");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);

    app.script.borrow_mut().unsaved.push_back(Unsaved::Cancel);
    app.shortcut(Modifiers::COMMAND, Key::N);
    assert_eq!(app.objects().len(), 1, "Cancel keeps the drawing");

    app.script.borrow_mut().unsaved.push_back(Unsaved::Save);
    app.script.borrow_mut().save_paths.push_back(dir.join("kept.vstudio"));
    app.shortcut(Modifiers::COMMAND, Key::N);
    assert!(dir.join("kept.vstudio").exists(), "Save writes it first");
    assert!(app.objects().is_empty(), "then a new document opens");
    assert_eq!(app.doc().title, "Untitled-2");

    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.script.borrow_mut().unsaved.push_back(Unsaved::Discard);
    app.menu(&["File", "New"]);
    assert!(app.objects().is_empty(), "Discard drops it");

    let asked = app.script.borrow().shown.iter().filter(|t| *t == "Unsaved changes").count();
    app.menu(&["File", "New"]);
    assert_eq!(app.script.borrow().shown.iter().filter(|t| *t == "Unsaved changes").count(), asked, "nothing to save: no question");
}

#[test]
fn export_window_writes_svg_png_and_pdf() {
    let dir = temp_dir("export");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");

    let export = |app: &mut E2e, format: &str, file: &str| {
        app.shortcut(Modifiers::COMMAND, Key::E);
        app.click("Export format");
        app.click(format);
        app.script.borrow_mut().save_paths.push_back(dir.join(file));
        app.click("Export…");
        assert!(!app.exists("Export…"), "the window closes after exporting");
    };

    export(&mut app, "SVG - Scalable Vector Graphics", "page.svg");
    let svg = std::fs::read_to_string(dir.join("page.svg")).unwrap();
    assert!(svg.contains(r##"fill="#e30613""##), "{svg}");

    app.shortcut(Modifiers::COMMAND, Key::E);
    app.click("Export format");
    app.click("PNG - Portable Network Graphics");
    app.fill("Resolution", "72");
    app.click("White");
    app.script.borrow_mut().save_paths.push_back(dir.join("page"));
    app.click("Export…");
    let png = std::fs::read(dir.join("page.png")).expect("the extension is added");
    assert_eq!(&png[1..4], b"PNG");
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    assert_eq!(w, 596, "A4 at 72 dpi");

    export(&mut app, "PDF - Portable Document Format", "doc.pdf");
    assert!(std::fs::read(dir.join("doc.pdf")).unwrap().starts_with(b"%PDF-"));

    app.shortcut(Modifiers::COMMAND, Key::E);
    app.click("Cancel");
    assert!(!app.exists("Export…"));
}

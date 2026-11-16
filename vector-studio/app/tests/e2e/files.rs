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

#[test]
fn import_places_an_svg_in_the_middle_of_the_page_selected_and_undoable() {
    let dir = temp_dir("import");
    let path = dir.join("logo.svg");
    std::fs::write(
        &path,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <rect width="96" height="96" fill="#ff0000"/>
            <circle cx="150" cy="48" r="48" fill="#0000ff" stroke="#000" stroke-width="4"/>
        </svg>"##,
    )
    .unwrap();
    let mut app = E2e::new();
    app.draw_rect(10.0, 10.0, 20.0, 20.0);
    app.script.borrow_mut().open_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND, Key::I);
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Import"));

    assert_eq!(app.objects().len(), 2, "the drawing comes in as one group");
    let group = &app.objects()[1];
    assert_eq!(app.selection(), vec![group.id], "and is selected");
    let vs_core::Shape::Group { children } = &group.shape else { panic!("group") };
    assert_eq!(children.len(), 2);
    let page = app.doc().active_page().bounds();
    let b = group.world_bounds();
    assert!((b.center() - page.center()).hypot() < 0.01, "centred on the page: {b:?}");
    assert!((b.width() - 148.5).abs() < 0.01, "the circle ends at 198 px, which is 148.5 pt: {b:?}");

    // The red square shows on the canvas where it landed.
    app.click_doc(10.0, 400.0);
    let px = app.pixel(b.x0 + 10.0, b.center().y);
    assert!(px[0] > 200 && px[1] < 60 && px[2] < 60, "red: {px:?}");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 1, "Undo takes the import back");

    std::fs::write(&path, "not svg").unwrap();
    app.script.borrow_mut().open_paths.push_back(path);
    app.menu(&["File", "Import…"]);
    assert!(app.script.borrow().errors.last().is_some_and(|e| e.starts_with("Could not import")), "{:?}", app.script.borrow().errors);
    assert_eq!(app.objects().len(), 1);
}

fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn import_places_a_png_as_a_bitmap_that_draws_and_fits_the_page() {
    let dir = temp_dir("import_png");
    let small = dir.join("blue.png");
    std::fs::write(&small, png(160, 80, [0, 0, 255])).unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(small);
    app.shortcut(Modifiers::COMMAND, Key::I);

    assert_eq!(app.objects().len(), 1);
    let obj = &app.objects()[0];
    let vs_core::Shape::Image(b) = &obj.shape else { panic!("a bitmap: {:?}", obj.shape) };
    assert_eq!((b.width, b.height), (160, 80));
    assert_eq!(app.selection(), vec![obj.id]);
    let page = app.doc().active_page().bounds();
    let r = obj.world_bounds();
    assert!((r.width() - 120.0).abs() < 0.01 && (r.center() - page.center()).hypot() < 0.01, "160 px = 120 pt, centred: {r:?}");

    app.click_doc(10.0, 10.0);
    let px = app.pixel(page.center().x, page.center().y);
    assert!(px[2] > 200 && px[0] < 60 && px[1] < 60, "the bitmap shows on the canvas: {px:?}");
    app.click_doc(page.center().x, page.center().y);
    assert_eq!(app.selection().len(), 1, "a click anywhere on it selects it");

    let wide = dir.join("wide.png");
    std::fs::write(&wide, png(4000, 100, [0, 255, 0])).unwrap();
    app.script.borrow_mut().open_paths.push_back(wide);
    app.menu(&["File", "Import…"]);
    let r = app.bounds(1);
    assert!((r.width() - page.width()).abs() < 0.01, "too wide for the page: scaled to fit it: {r:?}");
}

#[test]
fn revert_reloads_the_saved_file_and_close_starts_a_new_drawing() {
    let dir = temp_dir("revert");
    let path = dir.join("card.vstudio");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    assert!(!vector_studio::files::can_revert(app.state()), "never saved: nothing to revert to");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert!(!vector_studio::files::can_revert(app.state()), "no changes since saving");

    app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.script.borrow_mut().confirms.push_back(false);
    app.menu(&["File", "Revert"]);
    assert_eq!(app.objects().len(), 2, "Cancel keeps the changes");
    app.script.borrow_mut().confirms.push_back(true);
    app.menu(&["File", "Revert"]);
    assert_eq!(app.objects().len(), 1, "back to the saved version");
    assert!(!app.state().history.is_dirty());
    assert_eq!(app.state().file_path.as_deref(), Some(path.as_path()));

    // Close asks about unsaved changes, then starts a new drawing.
    app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.script.borrow_mut().unsaved.push_back(Unsaved::Cancel);
    app.shortcut(Modifiers::COMMAND, Key::W);
    assert_eq!(app.objects().len(), 2, "Cancel keeps the drawing open");
    app.script.borrow_mut().unsaved.push_back(Unsaved::Discard);
    app.menu(&["File", "Close All"]);
    assert!(app.objects().is_empty());
    assert_eq!(app.doc().title, "Untitled-2");
    assert_eq!(app.state().file_path, None);
    assert_eq!(vs_io::open_document(&path).unwrap().active_page().objects.len(), 1, "the file is untouched");
}

#[test]
fn document_properties_are_saved_exported_and_undoable() {
    let dir = temp_dir("properties");
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.menu(&["File", "Document Properties…"]);
    app.fill("Title (document properties)", "Shop sign");
    app.fill("Author (document properties)", "A. Person");
    app.fill("Keywords (document properties)", "sign, red");
    app.click("OK (Document Properties)");
    assert!(!app.exists("Title (document properties)"));
    let p = &app.doc().properties;
    assert_eq!((p.title.as_str(), p.author.as_str(), p.keyword_list()), ("Shop sign", "A. Person", vec!["sign".to_owned(), "red".to_owned()]));

    app.shortcut(Modifiers::COMMAND, Key::E);
    app.click("Export format");
    app.click("SVG - Scalable Vector Graphics");
    app.script.borrow_mut().save_paths.push_back(dir.join("sign.svg"));
    app.click("Export…");
    let svg = std::fs::read_to_string(dir.join("sign.svg")).unwrap();
    assert!(svg.contains("<title>Shop sign</title>") && svg.contains("<dc:creator>A. Person</dc:creator>"), "{svg}");

    let path = dir.join("sign.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND, Key::S);
    app.menu(&["File", "Document Properties…"]);
    assert_eq!(app.value("Title (document properties)"), "Shop sign");
    app.click("OK (Document Properties)");
    assert_eq!(vs_io::open_document(&path).unwrap().properties.author, "A. Person", "saved in the drawing");

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.doc().properties.keywords, "", "the last field typed is undone");
    assert_eq!(app.doc().properties.author, "A. Person");
}

#[test]
fn print_sends_the_chosen_pages_to_the_chosen_printer() {
    let mut app = E2e::new();
    app.shortcut(Modifiers::COMMAND, Key::P);
    assert!(!app.is_enabled("Print (Print)"), "no printer to print to");
    app.click("Cancel (Print)");
    assert!(!app.exists("Copies"));

    {
        let mut s = app.script.borrow_mut();
        s.printers = vec!["Office".into(), "Studio".into()];
        s.default_printer = Some("Studio".into());
    }
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Insert page after");
    app.click("Insert page after");
    app.menu(&["File", "Print…"]);
    app.fill("Copies", "2");
    app.fill("Page range", "5");
    assert!(!app.is_enabled("Print (Print)"), "there is no page 5");
    app.fill("Page range", "1, 3");
    app.click("Print (Print)");
    assert!(!app.exists("Copies"), "the window closes");
    let (job, pdf) = app.script.borrow().printed[0].clone();
    assert_eq!((job.printer.as_str(), job.copies, job.title.as_str()), ("Studio", 2, "Untitled-1"), "the default printer");
    assert!(pdf.starts_with(b"%PDF-") && String::from_utf8_lossy(&pdf).contains("/Count 2"), "pages 1 and 3");

    app.shortcut(Modifiers::COMMAND, Key::P);
    app.click("Printer");
    app.click("Office");
    app.click("Current page");
    app.click("Print (Print)");
    let (job, pdf) = app.script.borrow().printed[1].clone();
    assert_eq!((job.printer.as_str(), job.copies), ("Office", 2), "settings are kept between prints");
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 1"));
}

#[test]
fn save_as_template_then_new_from_template_starts_an_untitled_copy() {
    let mut app = E2e::new();
    let dir = temp_dir("template");
    app.draw_rect(100.0, 100.0, 200.0, 150.0);
    app.script.borrow_mut().save_paths.push_back(dir.join("mine.vstudio"));
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);

    app.menu(&["File", "Save as Template…"]);
    assert_eq!(app.value("Template name"), "mine", "the drawing's name to start with");
    app.fill("Template name", "Poster");
    app.click("Save (Save as Template)");
    assert!(!app.exists("Template name"), "the window closes");
    let templates = app.state().templates_dir.clone().unwrap();
    assert!(templates.join("Poster.vstemplate").exists());
    assert_eq!(app.state().file_path.as_deref(), Some(dir.join("mine.vstudio").as_path()), "the drawing keeps its own file");

    // Saving under the same name asks first; No keeps the old one.
    app.menu(&["File", "Save as Template…"]);
    app.fill("Template name", "Poster");
    app.script.borrow_mut().confirms.push_back(false);
    app.click("Save (Save as Template)");
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Replace template"));
    assert!(app.exists("Template name"), "still open after No");
    app.click("Cancel (Save as Template)");

    app.menu(&["File", "New from Template…"]);
    assert!(app.exists("Template Poster"));
    app.click("Open (New from Template)");
    assert!(!app.exists("Template Poster"));
    assert_eq!(app.objects().len(), 1, "the template's rectangle");
    assert!(app.state().file_path.is_none(), "a new, unsaved drawing");
    assert!(app.doc().title.starts_with("Untitled-"), "{}", app.doc().title);

    // Browse… opens any template file.
    app.menu(&["File", "New from Template…"]);
    app.script.borrow_mut().open_paths.push_back(templates.join("Poster.vstemplate"));
    app.click("Browse… (New from Template)");
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("New from Template"));
    assert_eq!(app.objects().len(), 1);
}

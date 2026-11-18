//! File > Print Merge: importing data, putting fields into text, printing and merging to a
//! new drawing.

use egui::{Key, Modifiers};
use vs_core::{Object, Shape};
use vector_studio::files::Unsaved;

use crate::support::{E2e, temp_dir};

fn content(obj: &Object) -> &str {
    match &obj.shape {
        Shape::Text(t) => &t.content,
        _ => panic!("not text"),
    }
}

#[test]
fn print_merge_fills_fields_from_each_record() {
    let mut app = E2e::new();
    app.menu(&["File", "Print Merge"]);
    assert!(app.exists("Import Data… (Print Merge)"));
    assert!(!app.is_enabled("Merge to New Document (Print Merge)") && !app.is_enabled("Print… (Print Merge)"), "no data yet");

    let dir = temp_dir("print-merge");
    let (bad, good) = (dir.join("bad.csv"), dir.join("people.csv"));
    std::fs::write(&bad, "Name,Name\nA,B\n").unwrap();
    std::fs::write(&good, "Name;City\nAnn;Oslo\n\"Bo; Jr\";Rome\n").unwrap();
    app.script.borrow_mut().open_paths.extend([bad, good]);
    app.click("Import Data… (Print Merge)");
    assert!(app.script.borrow().errors[0].contains("two columns are called “Name”"), "{:?}", app.script.borrow().errors);
    assert!(app.doc().merge.is_none());
    app.click("Import Data… (Print Merge)");
    assert_eq!(app.doc().merge.as_ref().unwrap().records.len(), 2);
    assert!(app.exists("Record 1 of 2") && app.exists("Oslo"));
    app.click("Next (Print Merge)");
    assert!(app.exists("Record 2 of 2") && app.exists("Bo; Jr"));
    assert!(!app.is_enabled("Next (Print Merge)"));

    // A field goes in at the caret of the text being typed…
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("Dear ");
    app.click("Insert field Name");
    assert_eq!(content(&app.objects()[0]), "Dear «Name»");
    app.key(Key::Escape);
    // …or on the page as new text when nothing is being typed.
    app.click("Insert field City");
    assert_eq!(app.objects().len(), 2);
    assert_eq!(content(&app.objects()[1]), "«City»");
    assert_eq!(app.selection(), vec![app.ids()[1]]);
    let page = app.doc().active_page().bounds().center();
    assert!((app.bounds(1).center().x - page.x).abs() < 1.0, "centred on the page");

    // Remove Data is undoable, and the data is saved with the drawing.
    app.click("Remove Data (Print Merge)");
    assert!(app.doc().merge.is_none());
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.doc().merge.is_some(), "undo brings it back");
    let saved = dir.join("letter.vstudio");
    vs_io::save_document(app.doc(), &saved).unwrap();
    assert_eq!(vs_io::open_document(&saved).unwrap().merge, app.doc().merge);

    // Print… prints one copy of the page per record.
    {
        let mut s = app.script.borrow_mut();
        s.printers = vec!["Office".into()];
        s.default_printer = Some("Office".into());
    }
    app.click("Print… (Print Merge)");
    assert!(!app.exists("Import Data… (Print Merge)"), "the Print window takes over");
    assert!(app.exists("Print Merge: 2 records, one copy of the pages each."));
    assert!(!app.exists("Current page"));
    app.click("Print (Print)");
    let (_, pdf) = app.script.borrow().printed[0].clone();
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 2"), "two records, one page each");
    assert_eq!(app.doc().page_count(), 1, "the drawing itself is unchanged");
    assert_eq!(content(&app.objects()[0]), "Dear «Name»");
    app.menu(&["File", "Print…"]);
    assert!(app.exists("Current page"), "plain printing again");
    app.click("Cancel (Print)");

    // Merge to New Document asks to save, then opens the merged, unsaved drawing.
    app.menu(&["File", "Print Merge"]);
    app.script.borrow_mut().unsaved.push_back(Unsaved::Discard);
    app.click("Merge to New Document (Print Merge)");
    assert!(!app.exists("Import Data… (Print Merge)"), "the window closes");
    let doc = app.doc();
    assert_eq!(doc.page_count(), 2);
    assert!(doc.merge.is_none());
    assert_eq!(doc.title, "Untitled-2");
    let texts = |p: usize| doc.pages()[p].objects.iter().map(|o| content(o).to_owned()).collect::<Vec<_>>();
    assert_eq!(texts(0), ["Dear Ann", "Oslo"]);
    assert_eq!(texts(1), ["Dear Bo; Jr", "Rome"]);
    assert!(app.state().history.is_dirty(), "not saved yet");
    assert_eq!(app.state().status.as_deref(), Some("Merged 2 records into 2 pages"));
}

//! Find and Replace docker (Edit > Find and Replace, Cmd+F).

use egui::{Key, Modifiers};
use vs_core::{ObjectId, Shape};

use crate::support::E2e;

fn write(app: &mut E2e, x: f64, y: f64, text: &str) -> ObjectId {
    app.key(Key::F8);
    app.click_doc(x, y);
    app.type_text(text);
    app.key(Key::Escape);
    app.pick_tool();
    *app.ids().last().unwrap()
}

fn content(app: &E2e, page: usize, id: ObjectId) -> String {
    match &app.doc().pages()[page].object(id).unwrap().shape {
        Shape::Text(t) => t.content.clone(),
        _ => panic!("not text"),
    }
}

#[test]
fn find_next_walks_every_page_and_replace_changes_one_or_all() {
    let mut app = E2e::new();
    let apple = write(&mut app, 100.0, 100.0, "Red apple");
    write(&mut app, 100.0, 200.0, "Green pear");
    let cherry = write(&mut app, 100.0, 300.0, "red cherry");
    app.click("Insert page after");
    let sky = write(&mut app, 100.0, 100.0, "RED sky");
    app.click("First page");

    app.shortcut(Modifiers::COMMAND, Key::F);
    assert!(app.is_selected("Find and Replace docker"));
    app.fill("Find (find and replace)", "red");
    assert_eq!(app.selection(), vec![apple], "Enter finds the first match");
    assert!(app.exists("1 of 3"));
    app.click("Find next");
    assert_eq!(app.selection(), vec![cherry]);
    app.click("Find next");
    assert_eq!((app.doc().active_page_index(), app.selection()), (1, vec![sky]), "on to the next page");
    app.click("Find next");
    assert_eq!((app.doc().active_page_index(), app.selection()), (0, vec![apple]), "and round to the first");

    app.click("Match case");
    app.click("Find next");
    assert_eq!(app.selection(), vec![cherry], "only the lower-case one");
    assert!(app.exists("1 of 1"));
    app.fill("Find (find and replace)", "plum");
    assert!(app.exists("Not found"));
    assert_eq!(app.selection(), vec![cherry], "the selection stays");
    app.click("Match case");
    app.fill("Find (find and replace)", "red");
    assert_eq!(app.selection(), vec![sky]);
    app.click("Find next");
    assert_eq!(app.selection(), vec![apple]);

    app.fill("Replace with (find and replace)", "Blue");
    app.click("Replace");
    assert_eq!(content(&app, 0, apple), "Blue apple", "the selected match");
    assert_eq!(app.selection(), vec![cherry], "then the next one is found");
    app.click("Replace all");
    assert!(app.exists("Replaced 2 times"));
    assert_eq!((content(&app, 0, cherry), content(&app, 1, sky)), ("Blue cherry".into(), "Blue sky".into()));

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(content(&app, 1, sky), "RED sky", "Replace All is one undo step");
    assert_eq!(content(&app, 0, cherry), "red cherry");
}

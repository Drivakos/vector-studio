//! The Text tool (F8): typing artistic text, editing it, font controls, and text as an
//! ordinary object (undo, Convert to Curves, save and open).

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use vector_studio::toolbox::Tool;
use vs_core::kurbo::Shape as _;
use vs_core::{Shape, Text, TextAlign};

use crate::support::{E2e, close};

fn text(app: &E2e, i: usize) -> Text {
    match &app.objects()[i].shape {
        Shape::Text(t) => t.clone(),
        s => panic!("not text: {s:?}"),
    }
}

/// Whether any pixel in the box from (x0, y0) to (x1, y1) (page points) is dark.
fn any_dark(app: &mut E2e, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let px = app.pixel(x, y);
            if px[0] < 80 && px[1] < 80 && px[2] < 80 {
                return true;
            }
            x += 1.5;
        }
        y += 1.5;
    }
    false
}

#[test]
fn text_tool_types_lines_edits_at_the_caret_and_esc_finishes() {
    let mut app = E2e::new();
    app.key(Key::F8);
    assert_eq!(app.state().tool, Tool::Text);
    app.click_doc(100.0, 200.0);
    app.type_text("Hello");
    assert_eq!(app.objects().len(), 1);
    assert_eq!(text(&app, 0).content, "Hello");
    assert_eq!(app.selection(), vec![app.ids()[0]]);
    assert_eq!(app.objects()[0].style.fill, Some(vs_core::Rgba::BLACK.into()), "new text is filled black");
    let b = app.bounds(0);
    assert!(close(b.x0, 100.0) && b.y0 < 200.0 && b.y1 > 200.0, "first baseline starts at the click: {b:?}");

    // Plain keys type, they do not run their shortcuts.
    app.key(Key::F6);
    app.key(Key::L);
    assert_eq!(app.state().tool, Tool::Text, "F6 did not switch tools");
    assert!(close(app.bounds(0).x0, 100.0), "L did not align");

    app.key(Key::Enter);
    app.type_text("World");
    app.key(Key::Backspace);
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);
    app.type_text("X");
    assert_eq!(text(&app, 0).content, "Hello\nWoXrl");
    assert!(app.bounds(0).y1 > b.y1 + 10.0, "a second line below the first");

    app.key(Key::Escape);
    assert!(app.state().tools.text.is_none(), "Esc finishes typing");
    assert_eq!(app.selection().len(), 1, "the text stays selected");
    app.click_doc(500.0, 700.0);
    app.pick_tool();
    let b = app.bounds(0);
    assert!(any_dark(&mut app, b.x0, b.y0, b.x0 + 30.0, 200.0), "the glyphs render in black");

    // One undo step takes back the whole text.
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects().is_empty());
}

#[test]
fn empty_text_is_removed_and_clicking_text_edits_it() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.pick_tool();
    assert!(app.objects().is_empty(), "text left empty is removed");

    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("abc");
    app.click_doc(100.0, 400.0);
    assert_eq!(app.objects().len(), 2, "a click elsewhere starts new text");
    app.type_text("second");

    // Click just before "c" of the first text: the caret goes there.
    let layout = text(&app, 0).layout();
    let x = 100.0 + layout.lines[0].carets[2] - 0.5;
    app.click_doc(x, 195.0);
    app.type_text("Z");
    assert_eq!(text(&app, 0).content, "abZc");
    assert_eq!(text(&app, 1).content, "second");

    // Pick tool: a double-click on text types into it.
    app.pick_tool();
    app.double_click_doc(100.0 + text(&app, 1).layout().lines[0].carets.last().unwrap() - 1.0, 395.0);
    assert_eq!(app.state().tool, Tool::Text);
    app.key(Key::End);
    app.type_text("!");
    assert_eq!(text(&app, 1).content, "second!");
}

#[test]
fn font_size_and_alignment_from_the_property_bar() {
    let mut app = E2e::new();
    app.key(Key::F8);
    // Nothing selected: the settings are for new text.
    app.fill("Font size", "48");
    app.click("Align text center");
    app.click_doc(300.0, 200.0);
    app.type_text("Mid");
    let t = text(&app, 0);
    assert_eq!(t.size, 48.0);
    assert_eq!(t.align, TextAlign::Center);
    let b = app.bounds(0);
    assert!((b.center().x - 300.0).abs() < 1.0, "centred on the click: {b:?}");

    // With the Pick tool, the controls change the selected text.
    app.key(Key::Escape);
    app.pick_tool();
    app.click_doc(300.0, 190.0);
    assert!(app.exists("Font list"));
    let before = app.bounds(0).height();
    app.fill("Font size", "24");
    assert_eq!(text(&app, 0).size, 24.0);
    assert!((app.bounds(0).height() / before - 0.5).abs() < 0.01);
    app.click("Align text right");
    assert_eq!(text(&app, 0).align, TextAlign::Right);
    assert!((app.bounds(0).x1 - 300.0).abs() < 1.0);
}

#[test]
fn text_moves_converts_to_curves_and_survives_save_and_open() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("Save me");
    app.pick_tool();
    app.click_doc(110.0, 195.0);
    app.drag_doc((110.0, 195.0), (160.0, 245.0));
    assert!(close(app.bounds(0).x0, 150.0), "moved like any object");

    let dir = crate::support::temp_dir("text");
    let path = dir.join("text.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert_eq!(vs_io::open_document(&path).expect("saved file opens"), *app.doc(), "text round-trips");

    let glyphs = app.objects()[0].world_path().bounding_box();
    app.shortcut(Modifiers::COMMAND, Key::Q);
    assert!(matches!(app.objects()[0].shape, Shape::Curve { .. }), "converted to curves");
    let b = app.objects()[0].world_path().bounding_box();
    assert!((b.x0 - glyphs.x0).abs() < 0.01 && (b.y1 - glyphs.y1).abs() < 0.01, "same outlines: {b:?} vs {glyphs:?}");
}

#[test]
fn paragraph_text_wraps_in_a_dragged_frame_converts_and_overflows() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.drag_doc((100.0, 100.0), (220.0, 300.0));
    app.type_text("one two three four five six seven eight");
    let t = text(&app, 0);
    assert_eq!(t.content, "one two three four five six seven eight");
    let frame = t.frame.expect("a drag makes paragraph text");
    assert!(close(frame.width, 120.0) && close(frame.height, 200.0), "{frame:?}");
    assert!(t.layout().lines.len() > 1, "words wrap to the frame");
    let b = app.objects()[0].world_path().bounding_box();
    assert!(b.x0 >= 99.5 && b.x1 <= 220.5 && b.y0 >= 99.5, "glyphs stay in the frame: {b:?}");
    assert!(any_dark(&mut app, 100.0, 100.0, 220.0, 130.0), "the first line is drawn at the top");
    app.key(Key::Escape);
    assert_eq!(app.objects()[0].shape.kind_label(), "Paragraph Text");

    // To artistic text and back, without moving the glyphs.
    app.pick_tool();
    app.menu(&["Text", "Convert to Artistic/Paragraph Text"]);
    let artistic = text(&app, 0);
    assert_eq!(artistic.frame, None);
    assert!(artistic.content.contains('\n'), "wrapped lines become line breaks: {:?}", artistic.content);
    let after = app.objects()[0].world_path().bounding_box();
    assert!(close(after.x0, b.x0) && close(after.y0, b.y0) && close(after.x1, b.x1), "{b:?} → {after:?}");
    app.menu(&["Text", "Convert to Artistic/Paragraph Text"]);
    assert!(text(&app, 0).frame.is_some());
    let back = app.objects()[0].world_path().bounding_box();
    assert!(close(back.x0, b.x0) && close(back.y0, b.y0), "{b:?} → {back:?}");

    // A short frame hides the lines that do not fit.
    app.docker("Properties");
    app.fill("Frame height (properties)", "12");
    let t = text(&app, 0);
    assert!(close(t.frame.unwrap().height, vs_core::Unit::Millimeters.to_points(12.0)));
    assert!(t.layout().overflow, "the text no longer fits");

    // An empty frame stays when typing ends.
    app.key(Key::F8);
    app.drag_doc((300.0, 100.0), (400.0, 200.0));
    app.key(Key::Escape);
    assert_eq!(app.objects().len(), 2);
    assert!(text(&app, 1).content.is_empty() && text(&app, 1).frame.is_some());
}

#[test]
fn fit_text_to_path_runs_it_along_a_curve_and_straighten_undoes_it() {
    let mut app = E2e::new();
    app.key(Key::F7);
    app.drag_doc((100.0, 100.0), (300.0, 300.0));
    app.key(Key::F8);
    app.click_doc(100.0, 450.0);
    app.type_text("Round");
    app.key(Key::Escape);
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::A);
    assert_eq!(app.selection().len(), 2);
    app.menu(&["Text", "Fit Text to Path"]);
    assert_eq!(app.objects().len(), 2, "the ellipse stays");
    let t = text(&app, 1);
    assert!(t.on_path.is_some());
    assert_eq!(app.selection(), vec![app.ids()[1]]);
    let b = app.objects()[1].world_bounds();
    assert!(b.x1 > 290.0 && b.y0 > 180.0 && b.y1 < 300.0, "runs from the ellipse's right edge: {b:?}");
    assert!(any_dark(&mut app, b.x0, b.y0, b.x1, b.y1), "drawn on the curve");
    assert!(!any_dark(&mut app, 100.0, 420.0, 200.0, 460.0), "no longer where it was typed");

    // The text stays editable in the Properties docker.
    app.docker("Properties");
    app.click("Text (properties)");
    app.h.get_by_label("Text (properties)").type_text("!");
    app.run();
    assert_eq!(text(&app, 1).content.chars().count(), 6);
    assert!(text(&app, 1).on_path.is_some());

    app.menu(&["Text", "Straighten Text"]);
    let t = text(&app, 1);
    assert!(t.on_path.is_none());
    let start = app.objects()[1].transform * vs_core::kurbo::Point::ORIGIN;
    assert!(close(start.x, 300.0) && (start.y - 200.0).abs() < 1.0, "starts where it started on the curve: {start:?}");
}

#[test]
fn edit_text_window_changes_the_words_or_leaves_them() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 100.0);
    app.type_text("Hello");
    app.key(Key::Escape);
    app.pick_tool();
    app.click_doc(110.0, 95.0);
    assert_eq!(app.selection().len(), 1);

    let retype = |app: &mut E2e, words: &str| {
        app.click("Text (edit text)");
        app.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        app.h.get_by_label("Text (edit text)").type_text(words);
        app.run();
    };
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::T);
    assert_eq!(app.value("Text (edit text)"), "Hello");
    retype(&mut app, "Goodbye");
    app.click("Cancel (Edit Text)");
    assert_eq!(text(&app, 0).content, "Hello", "Cancel leaves it");

    app.menu(&["Text", "Edit Text…"]);
    retype(&mut app, "Goodbye");
    assert_eq!(text(&app, 0).content, "Hello", "nothing changes before OK");
    app.click("OK (Edit Text)");
    assert!(!app.exists("Text (edit text)"));
    assert_eq!(text(&app, 0).content, "Goodbye");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(text(&app, 0).content, "Hello");
}

#[test]
fn resizing_paragraph_text_rewraps_it_instead_of_stretching_it() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.drag_doc((100.0, 100.0), (220.0, 300.0));
    app.type_text("one two three four five six seven eight");
    app.key(Key::Escape);
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::A);
    let lines = text(&app, 0).layout().lines.len();
    let scale = |app: &E2e| app.objects()[0].transform.as_coeffs()[0];

    // The right-hand handle doubles the width: the frame grows, the glyphs do not.
    let gap = app.handle_gap();
    app.drag_doc((220.0 + gap, 200.0), (340.0 + gap, 200.0));
    let t = text(&app, 0);
    let frame = t.frame.unwrap();
    assert!(close(frame.width, 240.0) && close(frame.height, 200.0), "{frame:?}");
    assert!(close(scale(&app), 1.0), "glyphs keep their size");
    assert!(t.layout().lines.len() < lines, "fewer, longer lines: {} → {}", lines, t.layout().lines.len());
    let b = app.objects()[0].world_bounds();
    assert!(close(b.x0, 100.0) && close(b.x1, 340.0), "{b:?}");

    // So does the property bar's width.
    app.fill("Object width", "50");
    let width = text(&app, 0).frame.unwrap().width;
    assert!((width - vs_core::Unit::Millimeters.to_points(50.0)).abs() < 0.1, "{width}");
    assert!(close(scale(&app), 1.0));

    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(close(text(&app, 0).frame.unwrap().width, 240.0), "undo");
}

#[test]
fn select_all_text_selects_only_the_text_objects() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 150.0);
    app.key(Key::F8);
    app.click_doc(100.0, 300.0);
    app.type_text("one");
    app.key(Key::Escape);
    app.click_doc(100.0, 400.0);
    app.type_text("two");
    app.key(Key::Escape);
    app.pick_tool();
    app.click_doc(500.0, 800.0);
    assert!(app.selection().is_empty());
    app.menu(&["Edit", "Select All", "Text"]);
    let ids = app.ids();
    assert_eq!(app.selection(), vec![ids[1], ids[2]]);
}

#[test]
fn change_case_rewrites_the_selected_text_or_the_text_being_typed() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("the cat. a dog");
    app.key(Key::Escape);
    assert_eq!(app.selection(), vec![app.ids()[0]]);

    app.menu(&["Text", "Change Case…"]);
    app.click("Title Case");
    app.click("OK (Change Case)");
    assert_eq!(text(&app, 0).content, "The Cat. A Dog");

    app.menu(&["Text", "Change Case…"]);
    app.click("UPPERCASE");
    app.click("Cancel (Change Case)");
    assert_eq!(text(&app, 0).content, "The Cat. A Dog", "Cancel leaves it");

    // While typing, it changes the text being typed into, and typing goes on at the caret.
    app.click_doc(100.0, 400.0);
    app.type_text("ab");
    app.menu(&["Text", "Change Case…"]);
    app.click("UPPERCASE");
    app.click("OK (Change Case)");
    app.type_text("c");
    assert_eq!(text(&app, 1).content, "ABc");
    assert_eq!(text(&app, 0).content, "The Cat. A Dog", "other text is left alone");
}

#[test]
fn text_statistics_count_the_selected_text_or_the_whole_drawing() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("one two");
    app.key(Key::Enter);
    app.type_text("three");
    app.key(Key::Escape);
    app.click_doc(100.0, 400.0);
    app.type_text("four");
    app.key(Key::Escape);
    app.pick_tool();
    app.click_doc(500.0, 800.0);

    app.menu(&["Text", "Text Statistics"]);
    assert!(app.exists("Whole drawing"));
    assert!(app.exists("Text objects: 2"));
    assert!(app.exists("Words: 4"));
    assert!(app.exists("Lines: 3"));
    assert!(app.exists("Characters: 16"));
    app.click("Close (Text Statistics)");
    assert!(!app.exists("Words: 4"));

    app.click_doc(110.0, 395.0);
    assert_eq!(app.selection().len(), 1);
    app.menu(&["Text", "Text Statistics"]);
    assert!(app.exists("Selected text"));
    assert!(app.exists("Words: 1"));
}

#[test]
fn text_properties_opens_the_properties_docker_on_the_selected_text() {
    let mut app = E2e::new();
    app.key(Key::F8);
    app.click_doc(100.0, 200.0);
    app.type_text("Hi");
    app.key(Key::Escape);
    app.docker("Objects");
    assert!(!app.exists("Text (properties)"));
    app.menu(&["Text", "Text Properties…"]);
    assert!(app.is_selected("Properties docker"));
    assert_eq!(app.value("Text (properties)"), "Hi");
}

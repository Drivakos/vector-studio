//! PowerClip (Object > Clip Frame): place inside, clipping on screen, edit and extract.

use egui::Key;

use crate::support::E2e;

const MENU: &str = "Clip Frame";

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[1] < 60 && px[2] < 60
}

#[test]
fn place_inside_clips_on_screen_and_contents_can_be_edited_and_extracted() {
    let mut app = E2e::new();
    let frame = app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Yellow");
    let red = app.draw_rect(200.0, 180.0, 400.0, 220.0);
    app.click("Swatch Red");

    // Esc cancels the pick.
    app.menu(&["Object", MENU, "Place Inside Frame…"]);
    app.key(Key::Escape);
    app.click_doc(150.0, 150.0);
    assert_eq!(app.ids(), vec![frame, red], "nothing placed after Esc");

    app.click_doc(350.0, 200.0);
    app.menu(&["Object", MENU, "Place Inside Frame…"]);
    app.click_doc(150.0, 150.0);
    assert_eq!(app.ids(), vec![frame]);
    assert_eq!(app.objects()[0].label(), "PowerClip Rectangle");
    assert_eq!(app.selection(), vec![frame]);
    assert!(is_red(app.pixel(250.0, 205.0)), "inside the frame the red bar shows");
    assert!(!is_red(app.pixel(350.0, 205.0)), "outside it is clipped away");

    // Edit Contents takes them out; Finish puts them back.
    app.menu(&["Object", MENU, "Edit Contents"]);
    assert_eq!(app.ids(), vec![frame, red]);
    assert_eq!(app.selection(), vec![red]);
    assert!(is_red(app.pixel(350.0, 205.0)), "while editing, the whole bar shows");
    app.click("Finish editing PowerClip");
    assert_eq!(app.ids(), vec![frame]);
    assert!(!app.exists("Finish editing PowerClip"));

    // Moving the frame moves its contents.
    app.drag_doc((120.0, 120.0), (140.0, 120.0));
    assert!(is_red(app.pixel(315.0, 205.0)), "the bar moved with the frame");

    app.menu(&["Object", MENU, "Extract Contents"]);
    assert_eq!(app.ids(), vec![frame, red]);
    assert_eq!(app.selection(), vec![red]);
    assert!(is_red(app.pixel(410.0, 205.0)), "extracted where it showed");
    assert!(app.objects()[0].contents.is_empty());
}

#[test]
fn an_empty_frame_shows_an_x_and_takes_an_object_dropped_on_it() {
    let mut app = E2e::new();
    let frame = app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Yellow");
    app.menu(&["Object", MENU, "Create Empty Frame"]);
    assert_eq!(app.objects()[0].label(), "Empty PowerClip Rectangle");
    // Grey over yellow: blue shows only where the X's lines are.
    let (on, off) = (app.pixel(150.0, 150.0), app.pixel(150.0, 200.0));
    assert!(on[2] > 60 && off[2] < 30, "the X crosses the frame: {on:?} vs {off:?}");

    let red = app.draw_rect(400.0, 180.0, 500.0, 220.0);
    app.click("Swatch Red");
    app.drag_doc((450.0, 200.0), (200.0, 200.0));
    assert_eq!(app.ids(), vec![frame], "dropped inside the frame");
    assert_eq!(app.objects()[0].contents[0].id, red);
    assert_eq!(app.objects()[0].label(), "PowerClip Rectangle");
    assert_eq!(app.selection(), vec![frame]);
    assert!(is_red(app.pixel(200.0, 205.0)), "it shows where it was dropped");

    app.shortcut(egui::Modifiers::COMMAND, Key::Z);
    assert_eq!(app.ids(), vec![frame, red], "undo takes it out again");
}

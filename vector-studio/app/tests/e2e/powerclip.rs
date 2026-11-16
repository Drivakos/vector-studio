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

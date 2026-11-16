//! Zoom, pan, rulers, dockers, palette and status bar.

use egui::{Key, Modifiers};

use crate::support::E2e;

fn zoom(app: &E2e) -> f64 {
    app.state().viewport.zoom_percent()
}

#[test]
fn zoom_keys_toolbar_menu_and_zoom_tool_bar() {
    let mut app = E2e::new();
    let fit = zoom(&app);
    app.shortcut(Modifiers::COMMAND, Key::Equals);
    assert!((zoom(&app) - fit * 2.0).abs() < 0.5, "Cmd+= doubles");
    app.shortcut(Modifiers::COMMAND, Key::Minus);
    assert!((zoom(&app) - fit).abs() < 0.5, "Cmd+- halves");
    app.shortcut(Modifiers::COMMAND, Key::Num1);
    assert!((zoom(&app) - 100.0).abs() < 0.01, "Cmd+1: actual size");
    app.shortcut(Modifiers::SHIFT, Key::F4);
    assert!((zoom(&app) - fit).abs() < 0.5, "Shift+F4: zoom to page");

    app.menu(&["Zoom levels", "Zoom In"]);
    assert!((zoom(&app) - fit * 2.0).abs() < 0.5, "toolbar zoom menu");
    app.key(Key::Z);
    app.click("Zoom to Page (property bar)");
    assert!((zoom(&app) - fit).abs() < 0.5, "Zoom tool property bar");
}

#[test]
fn pan_tool_moves_the_view_not_the_drawing() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.key(Key::H);
    let before = app.at(0.0, 0.0);
    let (a, b) = (app.at(300.0, 300.0), app.at(300.0, 300.0) + egui::vec2(40.0, 25.0));
    app.drag_screen(a, b);
    let after = app.at(0.0, 0.0);
    assert!((after - before - egui::vec2(40.0, 25.0)).length() < 1.0, "{before:?} → {after:?}");
    assert_eq!(app.bounds(0).x0.round(), 100.0, "objects stay where they are on the page");
}

#[test]
fn rulers_toggle_from_the_toolbar() {
    let mut app = E2e::new();
    assert!(!app.state().prefs.show_rulers, "off by default, as in the reference app");
    let canvas = app.state().canvas_rect;
    app.click("Rulers");
    assert!(app.state().prefs.show_rulers);
    assert!(app.is_selected("Rulers"));
    assert!(app.state().canvas_rect.min.x > canvas.min.x, "the rulers take room from the canvas");
}

#[test]
fn dockers_strip_close_list_and_show_hide() {
    let mut app = E2e::new();
    assert!(app.is_selected("Properties docker"));
    app.click("Objects docker");
    assert!(app.is_selected("Objects docker") && !app.is_selected("Properties docker"));
    app.click("Close Objects docker");
    assert!(!app.exists("Objects docker"), "closing removes it from the strip");
    app.menu(&["Docker list", "Objects"]);
    assert!(app.exists("Objects docker"), "the list opens it again");

    app.click("Show or hide dockers");
    assert!(!app.state().dockers.has_content());
    app.click("Show or hide dockers");
    assert!(app.state().dockers.has_content());
}

#[test]
fn palette_and_status_bar_toggle_from_the_view_menu() {
    let mut app = E2e::new();
    assert!(app.exists("Swatch Red"));
    app.menu(&["View", "Color Palette"]);
    assert!(!app.exists("Swatch Red"));
    app.menu(&["View", "Status Bar"]);
    assert!(app.state().prefs.show_status_bar);
}

#[test]
fn wireframe_draws_outlines_only_and_enhanced_brings_fills_back() {
    use vs_render::ViewMode;
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.click_doc(500.0, 700.0);
    let filled = app.pixel(150.0, 150.0);
    assert!(filled[0] > 200 && filled[1] < 60, "{filled:?}");

    app.menu(&["View", "Wireframe"]);
    assert_eq!(app.state().view_mode, ViewMode::Wireframe);
    let inside = app.pixel(150.0, 150.0);
    assert!(inside.iter().take(3).all(|c| *c > 240), "no fill in wireframe: {inside:?}");
    let edge = app.pixel(100.0, 150.0);
    assert!(edge[0] < 200 && edge[0] == edge[1], "a grey outline, not red: {edge:?}");

    app.menu(&["View modes", "Enhanced (view modes)"]);
    assert_eq!(app.state().view_mode, ViewMode::Enhanced);
    let back = app.pixel(150.0, 150.0);
    assert!(back[0] > 200 && back[1] < 60, "{back:?}");
}

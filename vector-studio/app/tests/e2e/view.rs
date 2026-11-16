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

/// Screen pixels along document row `y` from `x0` to `x1`, grouped by the bitmap pixel
/// (at `dpi`) each falls in. Pixels on a group's edge are left out.
fn by_bitmap_pixel(app: &mut E2e, y: f64, x0: f64, x1: f64, dpi: f64) -> Vec<Vec<[u8; 4]>> {
    let ppp = app.h.ctx.pixels_per_point();
    let img = app.h.render().expect("render");
    let (a, b) = (app.at(x0, y), app.at(x1, y));
    let row = (a.y * ppp) as u32;
    let mut groups: Vec<(i64, Vec<[u8; 4]>)> = Vec::new();
    for px in (a.x * ppp) as u32..(b.x * ppp) as u32 {
        let centre = egui::pos2((px as f32 + 0.5) / ppp, a.y);
        let doc_x = app.state().viewport.screen_to_doc(vs_core::kurbo::Point::new((centre.x - app.state().canvas_rect.min.x) as f64, 0.0)).x;
        let cell = (doc_x * dpi / 72.0).floor() as i64;
        let inside = ((doc_x * dpi / 72.0).fract() - 0.5).abs() < 0.35;
        match groups.last_mut() {
            Some((c, colours)) if *c == cell => {
                if inside {
                    colours.push(img.get_pixel(px, row).0);
                }
            }
            _ => groups.push((cell, if inside { vec![img.get_pixel(px, row).0] } else { Vec::new() })),
        }
    }
    groups.into_iter().map(|(_, c)| c).collect()
}

#[test]
fn pixels_view_shows_the_page_as_export_resolution_pixels() {
    let mut app = E2e::new();
    app.key(Key::F7);
    app.drag_doc((100.0, 100.0), (300.0, 250.0));
    app.click("Swatch Red");
    app.pick_tool();
    app.key(Key::Escape); // no selection frame drawn over the edge
    // A low export resolution makes the pixels several screen pixels wide.
    app.shortcut(Modifiers::COMMAND, Key::E);
    app.click("Export format");
    app.click("PNG - Portable Network Graphics");
    app.fill("Resolution", "18");
    app.click("Cancel");
    let uniform = |groups: &[Vec<[u8; 4]>]| groups.iter().all(|g| g.windows(2).all(|w| w[0] == w[1]));
    // Rows across the ellipse's left edge, which crosses each at a different place.
    let rows = |app: &mut E2e| (0..12).map(|i| by_bitmap_pixel(app, 130.0 + i as f64 * 8.0, 85.0, 140.0, 18.0)).collect::<Vec<_>>();

    assert!(!rows(&mut app).iter().all(|r| uniform(r)), "Enhanced draws a smooth edge inside a bitmap pixel");

    app.menu(&["View", "Pixels"]);
    assert_eq!(app.state().view_mode, vs_render::ViewMode::Pixels);
    let pixels = rows(&mut app);
    assert!(pixels[0].iter().filter(|g| g.len() >= 2).count() >= 5, "several screen pixels wide: {:?}", pixels[0]);
    for row in &pixels {
        assert!(uniform(row), "one colour per bitmap pixel: {row:?}");
    }
    let px = app.pixel(200.0, 175.0);
    assert!(px[0] > 200 && px[1] < 60, "still filled: {px:?}");

    app.click("View modes");
    app.click("Enhanced (view modes)");
    assert!(!rows(&mut app).iter().all(|r| uniform(r)));
}

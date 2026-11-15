//! Fill and outline: palette, property bar outline controls, Properties docker.

use vs_core::{LineCap, LineJoin, LineStyle, Outline, Rgba};

use crate::support::E2e;

const YELLOW: Rgba = Rgba::rgb(0xFF, 0xED, 0x00);
const BLUE: Rgba = Rgba::rgb(0x00, 0x71, 0xBC);

fn outline(app: &E2e, i: usize) -> Outline {
    app.objects()[i].style.outline.clone().expect("has an outline")
}

#[test]
fn palette_click_fills_right_click_outlines_and_no_color_removes() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Yellow");
    assert_eq!(app.objects()[0].style.fill, Some(YELLOW), "click sets the fill");
    app.right_click("Swatch Blue");
    assert_eq!(outline(&app, 0).color, BLUE, "right-click sets the outline colour");

    app.click("Swatch No color");
    assert_eq!(app.objects()[0].style.fill, None, "click removes the fill");
    app.right_click("Swatch No color");
    assert_eq!(app.objects()[0].style.outline, None, "right-click removes the outline");
    app.right_click("Swatch Red");
    assert_eq!(outline(&app, 0).width, vector_studio::outline::DEFAULT_WIDTH, "a palette colour adds an outline back");
}

#[test]
fn property_bar_outline_width_presets_typing_and_line_style() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Outline width presets (property bar)");
    app.click("4 pt");
    assert_eq!(outline(&app, 0).width, 4.0);
    app.fill("Outline width (property bar)", "2.5");
    assert_eq!(outline(&app, 0).width, 2.5);
    app.fill("Outline width (property bar)", "hairline");
    assert_eq!(outline(&app, 0).width, 0.0);

    app.click("Line style (property bar)");
    app.click("Dashed");
    assert_eq!(outline(&app, 0).style, LineStyle::Dash);

    app.click("Outline width presets (property bar)");
    app.click("None");
    assert_eq!(app.objects()[0].style.outline, None);
}

#[test]
fn properties_docker_sets_every_outline_option() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.docker("Properties");
    app.fill("Outline width (Properties docker)", "6");
    app.click("Line style (Properties docker)");
    app.click("Dash dot");
    app.click("Corners Round");
    app.click("Caps Square");
    app.click("Behind fill");
    app.click("Scale with object");
    let o = outline(&app, 0);
    assert_eq!(o.width, 6.0);
    assert_eq!(o.style, LineStyle::DashDot);
    assert_eq!(o.join, LineJoin::Round);
    assert_eq!(o.cap, LineCap::Square);
    assert!(o.behind_fill && o.scale_with_object);

    app.click("Uniform fill");
    assert!(app.objects()[0].style.fill.is_some(), "Uniform fill turns the fill on");
}

#[test]
fn editing_several_objects_changes_only_the_setting_touched() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 150.0, 150.0);
    app.click("Swatch Red");
    app.draw_rect(300.0, 100.0, 350.0, 150.0);
    app.click("Swatch Blue");
    app.shortcut(egui::Modifiers::COMMAND, egui::Key::A);
    app.click("Line style (property bar)");
    app.click("Dotted");
    assert_ne!(app.objects()[0].style.fill, app.objects()[1].style.fill, "fills stay different");
    assert!(app.objects().iter().all(|o| o.style.outline.as_ref().unwrap().style == LineStyle::Dot));
}

#[test]
fn outline_behind_fill_shows_on_the_canvas() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 300.0);
    app.click("Swatch Red");
    app.right_click("Swatch Black");
    app.fill("Outline width (property bar)", "40");
    app.click_doc(600.0, 700.0); // deselect so no handles cover the canvas
    // 10 pt inside the edge: inside the 40 pt outline's inner half.
    let over = app.pixel(110.0, 200.0);
    assert!(over[0] < 60, "outline on top covers the fill there: {over:?}");

    app.click_doc(100.0, 200.0);
    app.docker("Properties");
    app.click("Behind fill");
    app.click_doc(600.0, 700.0);
    let under = app.pixel(110.0, 200.0);
    assert!(under[0] > 200 && under[1] < 60, "behind fill, the fill covers its inner half: {under:?}");
    let outside = app.pixel(90.0, 200.0);
    assert!(outside[0] < 60, "the outer half still shows: {outside:?}");
}

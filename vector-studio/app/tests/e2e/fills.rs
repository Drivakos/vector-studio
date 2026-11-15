//! Fountain fills (Properties docker and Interactive Fill tool), transparency (docker and
//! Transparency tool), and the Color and Attributes Eyedroppers.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::kurbo::Point;
use vs_core::{Fill, GradientKind, Rgba};

use crate::support::{E2e, close};

const RED: Rgba = Rgba::rgb(0xE3, 0x06, 0x13);
const BLUE: Rgba = Rgba::rgb(0x00, 0x71, 0xBC);

fn fill(app: &E2e, i: usize) -> Option<Fill> {
    app.objects()[i].style.fill.clone()
}

fn gradient(app: &E2e, i: usize) -> vs_core::Gradient {
    match fill(app, i) {
        Some(Fill::Gradient(g)) => g,
        f => panic!("not a fountain fill: {f:?}"),
    }
}

fn near(px: [u8; 4], c: [u8; 3], tolerance: u8) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

#[test]
fn fountain_fill_and_transparency_from_the_properties_docker() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.docker("Properties");
    app.click("Fountain fill (docker)");
    let g = gradient(&app, 0);
    assert_eq!(g.kind, GradientKind::Linear);
    assert_eq!((g.stops[0].color, g.stops[1].color), (RED, Rgba::WHITE), "from the fill colour to white");
    assert!(close(g.start.x, 100.0) && close(g.end.x, 300.0), "across the object: {g:?}");

    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(103.0, 150.0), [0xE3, 0x0D, 0x19], 12), "{:?}", app.pixel(103.0, 150.0));
    assert!(near(app.pixel(200.0, 150.0), [0xF1, 0x82, 0x89], 12), "halfway: {:?}", app.pixel(200.0, 150.0));
    assert!(near(app.pixel(297.0, 150.0), [255, 255, 255], 12));

    app.click_doc(100.0, 150.0);
    app.click("Elliptical fountain fill (docker)");
    assert_eq!(gradient(&app, 0).kind, GradientKind::Radial);
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(200.0, 150.0), [0xE3, 0x06, 0x13], 12), "red in the middle");

    app.click_doc(100.0, 150.0);
    app.click("Uniform fill (docker)");
    assert_eq!(fill(&app, 0), Some(RED.into()));
    app.fill("Transparency (docker)", "50");
    assert!(close(app.objects()[0].style.transparency, 0.5));
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(200.0, 150.0), [0xF1, 0x82, 0x89], 12), "half see-through: {:?}", app.pixel(200.0, 150.0));

    app.click_doc(100.0, 150.0);
    app.click("No fill (docker)");
    assert_eq!(fill(&app, 0), None);
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(fill(&app, 0), Some(RED.into()), "undo");
}

#[test]
fn interactive_fill_drags_a_fountain_and_its_ends() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Blue");
    app.click_doc(600.0, 800.0);
    app.key(Key::G);
    assert_eq!(app.state().tool, Tool::InteractiveFill);
    app.drag_doc((150.0, 150.0), (250.0, 150.0));
    let g = gradient(&app, 0);
    assert!(g.start.distance(Point::new(150.0, 150.0)) < 0.1 && g.end.distance(Point::new(250.0, 150.0)) < 0.1, "{g:?}");
    assert_eq!((g.stops[0].color, g.stops[1].color), (BLUE, Rgba::WHITE));
    assert!(near(app.pixel(120.0, 150.0), [0x00, 0x71, 0xBC], 12), "before the start: the start colour");
    assert!(near(app.pixel(280.0, 150.0), [255, 255, 255], 12), "past the end: the end colour");

    // Drag the end handle.
    app.drag_doc((250.0, 150.0), (200.0, 180.0));
    let g = gradient(&app, 0);
    assert!(g.end.distance(Point::new(200.0, 180.0)) < 0.1, "{g:?}");
    assert!(g.start.distance(Point::new(150.0, 150.0)) < 0.1);

    // The property bar edits the same fill.
    app.click("Elliptical fountain fill (property bar)");
    assert_eq!(gradient(&app, 0).kind, GradientKind::Radial);
    app.click("Uniform fill (property bar)");
    assert_eq!(fill(&app, 0), Some(BLUE.into()));

    // The fill moves with the object.
    app.click("Fountain fill (property bar)");
    let before = gradient(&app, 0);
    app.pick_tool();
    app.drag_doc((200.0, 150.0), (250.0, 160.0));
    let t = app.objects()[0].transform;
    assert_eq!(gradient(&app, 0), before, "stored in the object's own space");
    assert!(close((t * before.start).x, before.start.x + 50.0));
}

#[test]
fn transparency_tool_sets_uniform_transparency() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.click_doc(600.0, 800.0);
    app.click("Transparency");
    assert_eq!(app.state().tool, Tool::Transparency);
    assert!(!app.exists("Transparency (property bar)"), "nothing selected yet");
    app.click_doc(200.0, 150.0);
    assert_eq!(app.selection().len(), 1);
    app.fill("Transparency (property bar)", "75");
    assert!(close(app.objects()[0].style.transparency, 0.75));
    app.click_doc(600.0, 800.0);
    let px = app.pixel(200.0, 150.0);
    assert!(near(px, [0xF8, 0xC1, 0xC4], 12), "a quarter of the red shows: {px:?}");
}

#[test]
fn eyedroppers_pick_and_apply_colours_and_attributes() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.click("Swatch Blue");
    app.click_doc(600.0, 800.0);

    app.click("Color Eyedropper");
    assert_eq!(app.state().tool, Tool::ColorEyedropper);
    app.click_doc(150.0, 150.0);
    assert_eq!(app.state().tools.eyedropper.color, Some(RED), "picked the red fill");
    assert!(app.is_selected("Apply color"), "switches to applying");
    app.click_doc(350.0, 150.0);
    assert_eq!(fill(&app, 1), Some(RED.into()), "applied to the fill");
    app.click_doc(300.0, 150.0);
    assert_eq!(app.objects()[1].style.outline.as_ref().unwrap().color, RED, "on the outline: applied to the outline");

    app.click("Select color");
    app.click_doc(500.0, 500.0);
    assert_eq!(app.state().tools.eyedropper.color, Some(Rgba::WHITE), "the empty page is white");

    // Attributes: fill, outline and transparency.
    app.pick_tool();
    app.click_doc(150.0, 150.0);
    app.docker("Properties");
    app.fill("Transparency (docker)", "40");
    app.click_doc(600.0, 800.0);
    app.flyout_tool("Color Eyedropper", "Attributes Eyedropper");
    assert_eq!(app.state().tool, Tool::AttributesEyedropper);
    app.click_doc(150.0, 150.0);
    assert!(app.is_selected("Apply attributes"));
    app.click_doc(350.0, 150.0);
    let s = &app.objects()[1].style;
    assert_eq!(s.fill, Some(RED.into()));
    assert!(close(s.transparency, 0.4));
}

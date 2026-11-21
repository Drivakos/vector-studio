//! Artistic Media: strokes drawn with the tool, preset and calligraphic, set from the
//! property bar; Effects > Artistic Media on a selected curve; the Shape tool editing the
//! curve under a stroke; Break Apart and Convert to Curves; and the other Effects menu
//! items that pick their tool.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::{ArtisticMedia, Fill, MediaKind, MediaPreset, Rgba, Shape};

use crate::support::{E2e, temp_dir};

fn media(app: &E2e, i: usize) -> ArtisticMedia {
    app.objects()[i].effects.media.clone().expect("an Artistic Media stroke")
}

fn dark(px: [u8; 4]) -> bool {
    px[..3].iter().all(|v| *v < 60)
}

fn white(px: [u8; 4]) -> bool {
    px[..3].iter().all(|v| *v > 240)
}

#[test]
fn artistic_media_tool_draws_preset_and_calligraphic_strokes() {
    let mut app = E2e::new();
    let dir = temp_dir("artistic-media");
    app.key(Key::I);
    assert_eq!(app.state().tool, Tool::ArtisticMedia);
    assert_eq!(app.value("Stroke preset"), "Tapered both ends");

    // A level stroke, thin at its ends and 10 pt wide in the middle.
    let line: Vec<(f64, f64)> = (0..=20).map(|i| (100.0 + i as f64 * 10.0, 200.0)).collect();
    app.stroke_doc(&line);
    assert_eq!(app.objects().len(), 1);
    let obj = &app.objects()[0];
    assert!(matches!(obj.shape, Shape::Curve { .. }), "the curve drawn stays the control curve");
    assert_eq!(obj.style.fill, Some(Fill::Solid(Rgba::BLACK)));
    assert!(obj.style.outline.is_none());
    assert_eq!(media(&app, 0), ArtisticMedia::preset(MediaPreset::TaperBoth, 10.0));
    let b = app.bounds(0);
    assert!((b.height() - 10.0).abs() < 0.5, "as wide as the stroke: {b:?}");
    app.click_doc(600.0, 800.0);
    assert_eq!(app.objects().len(), 1, "a click draws nothing");
    app.pick_tool();
    app.click_doc(600.0, 800.0);
    assert!(dark(app.pixel(200.0, 203.0)), "inside the stroke: {:?}", app.pixel(200.0, 203.0));
    assert!(white(app.pixel(200.0, 207.0)), "past its edge: {:?}", app.pixel(200.0, 207.0));
    assert!(white(app.pixel(101.0, 203.0)), "thin at the start: {:?}", app.pixel(101.0, 203.0));

    // With the stroke selected, the property bar changes it, one undo step at a time.
    app.click_doc(200.0, 200.0);
    assert_eq!(app.selection(), app.ids(), "a click on the stroke selects it");
    app.key(Key::I);
    app.click("Stroke preset");
    app.click("Even");
    assert_eq!(media(&app, 0).kind, MediaKind::Preset { preset: MediaPreset::Even });
    let unit = app.doc().display_unit;
    app.fill("Stroke width", &unit.from_points(20.0).to_string());
    assert!((media(&app, 0).width - 20.0).abs() < 1e-6);
    let b = app.bounds(0);
    assert!((b.height() - 20.0).abs() < 0.5 && (b.x0 - 90.0).abs() < 0.5, "round ends: {b:?}");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!((media(&app, 0).width - 10.0).abs() < 1e-6, "undo");

    // A calligraphic nib lying flat: a level stroke is hardly there, an upright one is wide.
    app.click("Calligraphic stroke");
    app.fill("Nib angle", "0");
    assert_eq!(media(&app, 0).kind, MediaKind::Calligraphic { angle: 0.0 });
    assert!(app.bounds(0).height() < 2.0, "{:?}", app.bounds(0));
    app.pick_tool();
    app.click_doc(600.0, 800.0);
    app.key(Key::I);
    let down: Vec<(f64, f64)> = (0..=10).map(|i| (400.0, 300.0 + i as f64 * 10.0)).collect();
    app.stroke_doc(&down);
    assert_eq!(media(&app, 1).kind, MediaKind::Calligraphic { angle: 0.0 }, "new strokes take the last settings");
    assert!((app.bounds(1).width() - 10.0).abs() < 0.5, "{:?}", app.bounds(1));

    let path = dir.join("strokes.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let back = vs_io::open_document(&path).unwrap();
    assert_eq!(back.active_page().objects[1].effects.media, Some(media(&app, 1)), "saved with the drawing");

    // SVG has no Artistic Media: the stroke is written as the curve it draws.
    let svg = dir.join("strokes.svg");
    vs_io::export(app.doc(), &svg, &vs_io::ExportOptions::default()).unwrap();
    let text = std::fs::read_to_string(&svg).unwrap();
    assert_eq!(text.matches("<path").count(), 2);
    assert!(text.contains(r##"fill="#000000""##) && !text.contains("stroke=\""), "filled, not stroked");
}

#[test]
fn effects_artistic_media_strokes_a_selected_curve_that_stays_editable() {
    let mut app = E2e::new();
    app.key(Key::F5);
    assert_eq!(app.state().tool, Tool::Freehand);
    let line: Vec<(f64, f64)> = (0..=20).map(|i| (100.0 + i as f64 * 10.0, 200.0)).collect();
    app.stroke_doc(&line);
    app.click("Swatch Red");
    app.right_click("Swatch Blue");
    let blue = app.objects()[0].style.outline.as_ref().unwrap().color;

    app.menu(&["Effects", "Artistic Media"]);
    assert_eq!(app.state().tool, Tool::ArtisticMedia);
    let obj = &app.objects()[0];
    assert_eq!(media(&app, 0), ArtisticMedia::preset(MediaPreset::TaperBoth, 10.0));
    assert!(obj.style.outline.is_none());
    assert_eq!(obj.style.fill, Some(Fill::Solid(blue)), "the line's colour fills the stroke");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects()[0].effects.media.is_none(), "one undo step");
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert!(app.objects()[0].effects.media.is_some());

    // The Shape tool moves the curve's end node; the stroke follows.
    app.key(Key::F10);
    let before = app.bounds(0);
    app.drag_doc((300.0, 200.0), (300.0, 260.0));
    let after = app.bounds(0);
    assert!(after.y1 > before.y1 + 40.0, "the stroke bent with the curve: {before:?} → {after:?}");
    assert!(app.objects()[0].effects.media.is_some(), "still a stroke");

    // Break Apart: the stroke as a curve, and its control curve on top.
    app.pick_tool();
    assert_eq!(app.selection(), app.ids(), "still selected");
    app.shortcut(Modifiers::COMMAND, Key::K);
    assert_eq!(app.objects().len(), 2);
    assert!(app.objects().iter().all(|o| o.effects.media.is_none()));
    assert!(app.objects()[0].shape.is_closed() && app.objects()[0].style.fill.is_some(), "the stroke");
    assert!(!app.objects()[1].shape.is_closed() && app.objects()[1].style.outline.is_some(), "the control curve");
    assert_eq!(app.selection().len(), 2);

    // Convert to Curves makes a stroke one closed curve.
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(app.objects().len(), 1);
    assert_eq!(app.selection(), app.ids());
    let shown = app.bounds(0);
    app.shortcut(Modifiers::COMMAND, Key::Q);
    let obj = &app.objects()[0];
    assert!(obj.effects.media.is_none() && obj.shape.is_closed());
    let b = app.bounds(0);
    assert!((b.x0 - shown.x0).abs() < 0.2 && (b.y1 - shown.y1).abs() < 0.2, "looks the same: {shown:?} vs {b:?}");
}

#[test]
fn effects_menu_picks_the_effect_tools() {
    let mut app = E2e::new();
    for (item, tool) in [("Blend", Tool::Blend), ("Contour", Tool::Contour), ("Envelope", Tool::Envelope), ("Extrude", Tool::Extrude)] {
        app.menu(&["Effects", item]);
        assert_eq!(app.state().tool, tool, "{item}");
    }
    assert!(!app.menu_enabled(&["Effects", "Lens"]), "not built yet");
}

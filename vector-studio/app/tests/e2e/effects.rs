//! Contour, Blend, Distort, Envelope, Extrude, Block Shadow and Mesh Fill: each tool makes
//! its effect from a drag or click, the property bar edits or clears it, and the effect
//! stays editable, is saved, exported and cleared by Object > Clear Effect.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::kurbo::{Point, Vec2};
use vs_core::{ContourDirection, DistortionKind, EnvelopeMode, ExtrudeShading, Shape};

use crate::support::{E2e, temp_dir};

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [0xE3, 0x06, 0x13];

/// Pick an effect tool from the Drop Shadow flyout, whose button shows `shown`.
fn effect_tool(app: &mut E2e, shown: &str, tool: &str) {
    app.flyout_tool(shown, tool);
}

fn rgb(px: [u8; 4]) -> [u8; 3] {
    [px[0], px[1], px[2]]
}

#[test]
fn contour_drags_out_or_in_and_the_property_bar_edits_it() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    effect_tool(&mut app, "Drop Shadow", "Contour");
    assert_eq!(app.state().tool, Tool::Contour);

    // From inside the square to 60 pt above it: an outside contour of three copies.
    app.drag_doc((150.0, 150.0), (150.0, 40.0));
    let c = app.objects()[0].effects.contour.clone().expect("a contour");
    assert_eq!(c.direction, ContourDirection::Outside);
    assert!((c.offset - 110.0 / 3.0).abs() < 0.01, "{}", c.offset);
    assert_eq!(app.selection(), vec![id]);
    let ring = rgb(app.pixel(130.0, 90.0));
    assert!(ring != WHITE && ring != RED, "the first copy, between red and white: {ring:?}");
    assert_eq!(rgb(app.pixel(160.0, 170.0)), RED, "the square over its contour");

    // The property bar: inside, five steps of 10 pt.
    app.click("Inside contour");
    app.fill("Contour steps", "5");
    let unit = app.doc().display_unit;
    app.fill("Contour offset", &format!("{}", unit.from_points(10.0)));
    let c = app.objects()[0].effects.contour.clone().unwrap();
    assert_eq!((c.direction, c.steps), (ContourDirection::Inside, 5));
    assert!((c.offset - 10.0).abs() < 1e-6);
    assert_eq!(rgb(app.pixel(130.0, 90.0)), WHITE, "nothing outside now");
    let inner = rgb(app.pixel(130.0, 160.0));
    assert!(inner != RED, "copies are drawn over the middle: {inner:?}");

    // Saved, exported, cleared.
    let path = temp_dir("contour-save").join("Contour.vstudio");
    vs_io::save_document(app.doc(), &path).unwrap();
    let opened = vs_io::open_document(&path).unwrap();
    assert_eq!(opened.active_page().objects[0].effects.contour.as_ref().map(|c| c.steps), Some(5));
    // 10, 20, 30 and 40 pt in: a fifth copy 50 pt in would have no area.
    assert_eq!(app.objects()[0].effects.contour.as_ref().unwrap().generated.objects().len(), 4);
    assert_eq!(vs_io::page_to_svg(app.doc().active_page()).matches("<path ").count(), 5, "the square and its four copies");
    app.click("Clear contour (property bar)");
    assert!(app.objects()[0].effects.contour.is_none());
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert!(app.objects()[0].effects.contour.is_some(), "undo brings it back");
    app.menu(&["Object", "Clear Effect"]);
    assert!(app.objects()[0].effects.contour.is_none(), "Clear Effect removes it");
}

#[test]
fn blend_drags_between_two_objects_and_follows_an_end() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 140.0, 140.0);
    app.click("Swatch Red");
    app.draw_rect(300.0, 100.0, 340.0, 140.0);
    app.click("Swatch Cyan");
    effect_tool(&mut app, "Drop Shadow", "Blend");
    app.drag_doc((120.0, 120.0), (320.0, 120.0));
    assert_eq!(app.objects().len(), 1, "one blend group");
    let group = &app.objects()[0];
    assert!(group.is_blend());
    assert_eq!(app.selection(), vec![group.id]);
    assert_eq!(group.blend_steps().len(), vs_core::Blend::STEPS as usize);
    assert_ne!(rgb(app.pixel(220.0, 112.0)), WHITE, "a step halfway");

    app.fill("Blend steps", "5");
    assert_eq!(app.objects()[0].blend_steps().len(), 5);
    // Drag the end down: the steps follow it.
    app.drag_doc((320.0, 120.0), (320.0, 320.0));
    assert_eq!(rgb(app.pixel(220.0, 112.0)), WHITE, "the steps moved");
    assert_ne!(rgb(app.pixel(212.0, 212.0)), WHITE, "the middle step is halfway to the moved end");
    assert_eq!(app.objects().len(), 1);

    let svg = vs_io::page_to_svg(app.doc().active_page());
    assert_eq!(svg.matches("<path ").count(), 7, "both ends and five steps");
    app.click("Clear blend (property bar)");
    assert!(app.objects()[0].effects.blend.is_none(), "a plain group of the two");
    assert_eq!(rgb(app.pixel(212.0, 212.0)), WHITE);
}

#[test]
fn distort_and_envelope_reshape_until_cleared() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    effect_tool(&mut app, "Drop Shadow", "Distort");

    // Push and Pull: 25 pt right of the centre pulls the corners out by half.
    app.drag_doc((150.0, 150.0), (175.0, 150.0));
    let d = app.objects()[0].effects.distortion.expect("a distortion");
    assert_eq!(d.kind, DistortionKind::PushPull);
    assert!((d.amplitude - 50.0).abs() < 0.5, "{}", d.amplitude);
    let b = app.objects()[0].world_bounds();
    assert!((b.x0 - 75.0).abs() < 1.0 && (b.x1 - 225.0).abs() < 1.0, "{b:?}");
    assert_eq!(rgb(app.pixel(85.0, 85.0)), RED, "a corner pulled out");

    app.click("Zipper distortion");
    app.fill("Zipper amplitude", "20");
    let d = app.objects()[0].effects.distortion.unwrap();
    assert_eq!((d.kind, d.amplitude), (DistortionKind::Zipper, 20.0));
    app.click("Clear distortion (property bar)");
    assert!(app.objects()[0].effects.distortion.is_none());

    // Envelope: drag the middle node of the top up 40 pt.
    effect_tool(&mut app, "Distort", "Envelope");
    assert_eq!(app.selection(), vec![id]);
    app.drag_doc((150.0, 100.0), (150.0, 60.0));
    let e = app.objects()[0].effects.envelope.expect("an envelope");
    assert_eq!(e.mode, EnvelopeMode::Straight);
    assert!((app.objects()[0].world_bounds().y0 - 60.0).abs() < 0.5);
    assert_eq!(rgb(app.pixel(150.0, 70.0)), RED, "the top peaks up");
    assert_eq!(rgb(app.pixel(105.0, 85.0)), WHITE, "the corners stay");
    app.click("Single arc envelope");
    assert_eq!(app.objects()[0].effects.envelope.unwrap().mode, EnvelopeMode::Arc);
    let svg = vs_io::page_to_svg(app.doc().active_page());
    assert!(svg.contains(" C"), "exported as the reshaped curves");

    // Convert to Curves bakes it in.
    app.pick_tool();
    app.shortcut(Modifiers::COMMAND, Key::Q);
    let obj = &app.objects()[0];
    assert!(obj.effects.envelope.is_none() && matches!(obj.shape, Shape::Curve { .. }));
    assert!((obj.world_bounds().y0 - 60.0).abs() < 0.5);
}

#[test]
fn extrude_and_block_shadow_drag_out_and_stay_editable() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    effect_tool(&mut app, "Drop Shadow", "Extrude");
    app.drag_doc((150.0, 150.0), (400.0, 150.0));
    let x = app.objects()[0].effects.extrude.clone().expect("an extrusion");
    assert!((x.vanishing_point - Vec2::new(250.0, 0.0)).hypot() < 0.01, "{:?}", x.vanishing_point);
    // 20% of the way to x = 400: the right side runs to x = 240.
    let side = rgb(app.pixel(220.0, 130.0));
    assert!(side != WHITE && side != RED, "a shaded side: {side:?}");
    assert_eq!(rgb(app.pixel(260.0, 130.0)), WHITE);

    app.fill("Extrude depth", "50");
    assert_ne!(rgb(app.pixel(280.0, 130.0)), WHITE, "deeper");
    app.click("Extrude with object fill");
    assert_eq!(app.objects()[0].effects.extrude.as_ref().unwrap().shading, ExtrudeShading::ObjectFill);
    assert_eq!(rgb(app.pixel(280.0, 130.0)), RED, "sides in the object's colour");
    // Drag the vanishing point down.
    app.drag_doc((400.0, 150.0), (150.0, 400.0));
    let vp = app.objects()[0].effects.extrude.as_ref().unwrap().vanishing_point;
    assert!((vp - Vec2::new(0.0, 250.0)).hypot() < 0.01, "{vp:?}");
    assert_eq!(rgb(app.pixel(130.0, 280.0)), RED, "now it runs down");

    // Block Shadow on a second square.
    let second = app.draw_rect(400.0, 400.0, 500.0, 500.0);
    app.click("Swatch Red");
    effect_tool(&mut app, "Extrude", "Block Shadow");
    app.drag_doc((450.0, 450.0), (480.0, 480.0));
    let obj = app.doc().active_page().object(second).unwrap();
    let b = obj.effects.block_shadow.clone().expect("a block shadow");
    assert!((b.offset - Vec2::new(30.0, 30.0)).hypot() < 0.01);
    assert!(rgb(app.pixel(515.0, 515.0)).iter().all(|c| *c < 40), "black beyond the square");
    assert_eq!(rgb(app.pixel(460.0, 470.0)), RED, "the square over it");
    let unit = app.doc().display_unit;
    app.fill("Block shadow depth", &format!("{}", unit.from_points(10.0)));
    let b = app.doc().active_page().object(second).unwrap().effects.block_shadow.clone().unwrap();
    assert!((b.offset.hypot() - 10.0).abs() < 0.01 && (b.offset.x - b.offset.y).abs() < 0.01, "same direction: {:?}", b.offset);
    assert_eq!(rgb(app.pixel(515.0, 515.0)), WHITE);

    let path = temp_dir("extrude-save").join("Extrude.vstudio");
    vs_io::save_document(app.doc(), &path).unwrap();
    let opened = vs_io::open_document(&path).unwrap();
    assert!(opened.active_page().objects[0].effects.extrude.is_some() && opened.active_page().objects[1].effects.block_shadow.is_some());
    app.click("Clear block shadow (property bar)");
    assert!(app.doc().active_page().object(second).unwrap().effects.block_shadow.is_none());
}

#[test]
fn mesh_fill_colours_its_nodes_from_the_palette_and_bends() {
    let mut app = E2e::new();
    let id = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");
    app.flyout_tool("Interactive Fill", "Mesh Fill");
    assert_eq!(app.state().tool, Tool::MeshFill);
    app.click_doc(150.0, 150.0);
    let mesh = app.objects()[0].effects.mesh.clone().expect("a mesh");
    assert_eq!((mesh.columns, mesh.rows, mesh.points.len()), (3, 3, 16));
    assert_eq!(app.selection(), vec![id]);
    assert_eq!(rgb(app.pixel(185.0, 150.0)), RED, "every node starts in the fill colour");

    // Click the top-left node and colour it yellow from the palette.
    app.click_doc(100.0, 100.0);
    assert_eq!(app.state().tools.effect.mesh_node, Some(0));
    app.click("Swatch Yellow");
    let mesh = app.objects()[0].effects.mesh.clone().unwrap();
    assert_eq!(mesh.colors[0], vs_core::Rgba::rgb(0xFF, 0xED, 0x00));
    assert_eq!(app.objects()[0].style.fill.as_ref().map(vs_core::Fill::color), Some(vs_core::Rgba::rgb(0xE3, 0x06, 0x13)), "the fill is left alone");
    let corner = rgb(app.pixel(108.0, 108.0));
    assert!(corner[1] > 80, "yellow near the node: {corner:?}");
    assert_eq!(rgb(app.pixel(185.0, 185.0)), RED, "red far from it");

    // Drag a middle node.
    let at = mesh.points[5];
    app.drag_doc((at.x, at.y), (at.x + 10.0, at.y + 5.0));
    let moved = app.objects()[0].effects.mesh.as_ref().unwrap().points[5];
    assert!((moved - Point::new(at.x + 10.0, at.y + 5.0)).hypot() < 0.01);

    // Fewer columns, then exported and cleared.
    app.fill("Mesh columns", "2");
    let mesh = app.objects()[0].effects.mesh.clone().unwrap();
    assert_eq!((mesh.columns, mesh.points.len()), (2, 12));
    assert!(vs_io::page_to_svg(app.doc().active_page()).contains("clip-path=\"url(#mesh"));
    app.click("Clear mesh (property bar)");
    assert!(app.objects()[0].effects.mesh.is_none());
    assert_eq!(rgb(app.pixel(108.0, 108.0)), RED);
}

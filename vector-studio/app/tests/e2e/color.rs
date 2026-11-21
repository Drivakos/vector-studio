//! CMYK and colour management: the Color docker (CMYK inks or RGB), the document's CMYK
//! profile and rendering intent (Document Options), the Color Proof Settings docker and
//! CMYK Print-Ready PDF.

use egui::{Key, Modifiers};
use vs_core::{RenderingIntent, Rgba};

use crate::support::{E2e, temp_dir};

const GENERIC: &str = "/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc";

fn near(px: [u8; 4], c: [u8; 3], tolerance: u8) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

fn fill_color(app: &E2e, i: usize) -> Rgba {
    app.objects()[i].leaves()[0].style.fill.as_ref().map(vs_core::Fill::color).expect("filled")
}

#[test]
fn color_docker_types_cmyk_inks_that_cmyk_output_keeps() {
    let mut app = E2e::new();
    let dir = temp_dir("cmyk-docker");
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.menu(&["Window", "Dockers", "Color"]);
    assert!(app.is_selected("CMYK model (Color docker)"), "CMYK to start");

    // Black ink only, typed one ink at a time.
    for (ink, v) in [("Cyan", "0"), ("Magenta", "0"), ("Yellow", "0"), ("Black", "100")] {
        app.fill(&format!("{ink} (Color docker)"), v);
    }
    assert_eq!(app.value("Black (Color docker)"), "100 %");
    app.click("Fill (Color docker)");
    let black = fill_color(&app, 0);
    assert_eq!(black.cmyk, Some([0, 0, 0, 255]), "the inks are kept: {black:?}");
    assert!(black.r < 90 && black.r == black.g, "black ink looks like dark grey on screen: {black:?}");
    app.click("Outline (Color docker)");
    assert_eq!(app.objects()[0].leaves()[0].style.outline.as_ref().unwrap().color.cmyk, Some([0, 0, 0, 255]));

    // RGB shows the same colour as screen values; typing RGB drops the inks.
    app.click("RGB model (Color docker)");
    assert_eq!(app.value("Red (Color docker)"), black.r.to_string());
    app.fill("Red (Color docker)", "255");
    app.click("Fill (Color docker)");
    assert_eq!(fill_color(&app, 0), Rgba::rgb(255, black.g, black.b), "plain RGB");

    // The docker takes up the selected object's colour.
    app.draw_rect(300.0, 100.0, 400.0, 200.0);
    app.click("Swatch Yellow");
    app.click_doc(150.0, 150.0);
    app.click_doc(350.0, 150.0);
    assert_eq!(app.value("Hex (Color docker)"), fill_color(&app, 1).to_hex());

    // Inks survive saving.
    app.click_doc(150.0, 150.0);
    app.click("CMYK model (Color docker)");
    app.fill("Cyan (Color docker)", "100");
    app.fill("Magenta (Color docker)", "0");
    app.fill("Yellow (Color docker)", "0");
    app.fill("Black (Color docker)", "0");
    app.click("Fill (Color docker)");
    let cyan = fill_color(&app, 0);
    assert_eq!(cyan.cmyk, Some([255, 0, 0, 0]));
    assert!(cyan.b > cyan.r, "cyan ink looks cyan: {cyan:?}");
    let path = dir.join("inks.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let back = vs_io::open_document(&path).unwrap();
    assert_eq!(back.active_page().objects[0].leaves()[0].style.fill.as_ref().map(vs_core::Fill::color), Some(cyan));
}

#[test]
fn print_ready_pdf_writes_cmyk_through_the_document_profile_or_rgb() {
    let mut app = E2e::new();
    let dir = temp_dir("cmyk-pdf");
    app.draw_rect(100.0, 100.0, 200.0, 200.0);
    app.click("Swatch Red");

    app.menu(&["File", "Export For", "Print-Ready PDF…"]);
    assert!(app.is_selected("CMYK colors (Print-Ready PDF)"), "CMYK to start");
    app.script.borrow_mut().save_paths.push_back(dir.join("cmyk.pdf"));
    app.click("Export… (Print-Ready PDF)");
    let pdf = String::from_utf8_lossy(&std::fs::read(dir.join("cmyk.pdf")).unwrap()).into_owned();
    assert!(pdf.contains("/ICCBased") && pdf.contains("/N 4"), "CMYK tied to the document's profile");

    app.menu(&["File", "Export For", "Print-Ready PDF…"]);
    app.click("RGB colors (Print-Ready PDF)");
    app.script.borrow_mut().save_paths.push_back(dir.join("rgb.pdf"));
    app.click("Export… (Print-Ready PDF)");
    let pdf = String::from_utf8_lossy(&std::fs::read(dir.join("rgb.pdf")).unwrap()).into_owned();
    assert!(!pdf.contains("/N 4"), "no CMYK in an RGB PDF");
}

#[test]
fn document_options_choose_the_cmyk_profile_and_rendering_intent() {
    let mut app = E2e::new();
    let dir = temp_dir("cmyk-profile");
    let press = dir.join("Press.icc");
    std::fs::copy(GENERIC, &press).unwrap();
    assert_eq!(app.doc().color, vs_core::ColorSettings::default(), "the generic profile to start");

    // An RGB profile is refused.
    app.menu(&["Layout", "Document Options…"]);
    app.click("CMYK profile (document options)");
    app.script.borrow_mut().open_paths.push_back("/System/Library/ColorSync/Profiles/sRGB Profile.icc".into());
    app.click("Other…");
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Not a CMYK profile"));

    app.click("CMYK profile (document options)");
    app.script.borrow_mut().open_paths.push_back(press.clone());
    app.click("Other…");
    app.click("Rendering intent (document options)");
    app.click("Perceptual");
    app.click("OK (Document Options)");
    assert_eq!(app.doc().color.cmyk_profile.as_deref(), Some(press.as_path()));
    assert_eq!(app.doc().color.intent, RenderingIntent::Perceptual);

    // The generic profile is back in the list, and Cancel keeps what was set.
    app.menu(&["Layout", "Document Options…"]);
    app.click("CMYK profile (document options)");
    app.click("Generic CMYK Profile");
    app.click("Cancel (Document Options)");
    assert_eq!(app.doc().color.cmyk_profile.as_deref(), Some(press.as_path()));

    let path = dir.join("profile.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    assert_eq!(vs_io::open_document(&path).unwrap().color, app.doc().color, "saved with the drawing");
}

#[test]
fn color_proof_settings_mark_colours_the_printing_cannot_reach() {
    let dir = temp_dir("gamut");
    let svg = dir.join("screen.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="96">
            <rect width="96" height="96" fill="#0000ff"/>
            <rect x="104" width="96" height="96" fill="#808080"/>
        </svg>"##,
    )
    .unwrap();
    let mut app = E2e::new();
    app.script.borrow_mut().open_paths.push_back(svg);
    app.shortcut(Modifiers::COMMAND, Key::I);
    let b = app.objects()[0].world_bounds();
    let (blue, grey) = ((b.x0 + 36.0, b.center().y), (b.x1 - 36.0, b.center().y));
    app.click_doc(10.0, 800.0);

    app.menu(&["Window", "Dockers", "Color Proof Settings"]);
    app.click("Proof colors (Color Proof Settings)");
    assert!(app.state().proof_colors, "the docker turns View > Proof Colors on");
    let proofed = app.pixel(blue.0, blue.1);
    assert!(proofed[2] < 235, "proofed: {proofed:?}");

    app.click("Out-of-gamut colors");
    assert!(near(app.pixel(blue.0, blue.1), [0, 255, 0], 2), "screen blue can't print: {:?}", app.pixel(blue.0, blue.1));
    let px = app.pixel(grey.0, grey.1);
    assert!(px[..3].iter().all(|v| v.abs_diff(128) < 20), "grey can: {px:?}");

    app.fill("Warning transparency", "50");
    let px = app.pixel(blue.0, blue.1);
    let half = Rgba::rgb(0, 255, 0).mix(Rgba::rgb(proofed[0], proofed[1], proofed[2]), 0.5);
    assert!(near(px, [half.r, half.g, half.b], 3), "half the proofed colour shows: {px:?} vs {half:?}");

    app.click("Out-of-gamut colors");
    app.click("Rendering intent (Color Proof Settings)");
    app.click("Absolute colorimetric");
    assert_eq!(app.state().prefs.proof.intent, RenderingIntent::AbsoluteColorimetric, "kept with the app's settings");
    assert_ne!(app.pixel(blue.0, blue.1), proofed, "the paper is simulated too");

    app.click("Proof colors (Color Proof Settings)");
    assert!(!app.state().proof_colors);
    assert!(near(app.pixel(blue.0, blue.1), [0, 0, 255], 2), "off again");
}

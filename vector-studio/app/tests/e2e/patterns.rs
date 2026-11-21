//! Pattern fills: two-color and bitmap patterns from the Properties docker, their tiles
//! dragged with the Interactive Fill tool, carried by the Attributes Eyedropper, and SVG
//! patterns imported as bitmap patterns.

use egui::{Key, Modifiers};
use vector_studio::toolbox::Tool;
use vs_core::kurbo::{Point, Size, Vec2};
use vs_core::{Fill, Pattern, PatternKind, Rgba, TWO_COLOR_PATTERNS};

use crate::support::{E2e, close, temp_dir};

const RED: Rgba = Rgba::rgb(0xE3, 0x06, 0x13);

fn pattern(app: &E2e, i: usize) -> Pattern {
    match &app.objects()[i].style.fill {
        Some(Fill::Pattern(p)) => p.clone(),
        f => panic!("not a pattern fill: {f:?}"),
    }
}

fn near(px: [u8; 4], c: [u8; 3], tolerance: u8) -> bool {
    px[..3].iter().zip(c).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

fn png(rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn two_color_pattern_fill_from_the_properties_docker() {
    let mut app = E2e::new();
    let dir = temp_dir("two-color");
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.docker("Properties");
    app.click("Two-color pattern fill (docker)");
    let p = pattern(&app, 0);
    assert_eq!(p.kind, PatternKind::TwoColor { cells: TWO_COLOR_PATTERNS[0].1, fore: RED, back: Rgba::WHITE }, "a checkerboard of the fill colour on white");
    assert_eq!(p.tile_size(), Size::new(18.0, 18.0));
    assert!((app.objects()[0].transform * p.transform * Point::ZERO).distance(Point::new(100.0, 100.0)) < 1e-3, "tiles start at the object's corner");
    assert_eq!(app.value("Pattern (docker)"), "Checkerboard");

    // An 18 pt checkerboard: red in the top-left quarter of each tile.
    app.click_doc(600.0, 800.0);
    for (x, y, red) in [(104.0, 104.0, true), (113.0, 104.0, false), (104.0, 113.0, false), (113.0, 113.0, true), (122.0, 104.0, true), (140.0, 140.0, true)] {
        let px = app.pixel(x, y);
        let want = if red { [0xE3, 0x06, 0x13] } else { [255, 255, 255] };
        assert!(near(px, want, 12), "({x}, {y}): {px:?}");
    }

    app.click_doc(150.0, 150.0);
    app.click("Pattern (docker)");
    app.click("Horizontal stripes");
    assert!(matches!(pattern(&app, 0).kind, PatternKind::TwoColor { cells, .. } if cells == TWO_COLOR_PATTERNS[2].1));
    let unit = app.doc().display_unit;
    app.fill("Tile width (docker)", &unit.from_points(36.0).to_string());
    let p = pattern(&app, 0);
    assert!(close(p.tile_size().width, 36.0) && close(p.tile_size().height, 18.0), "{:?}", p.tile_size());
    assert!((p.transform.translation() - Vec2::new(100.0, 100.0)).hypot() < 1e-3, "the tiles still start there");

    app.click("Uniform fill (docker)");
    assert_eq!(app.objects()[0].style.fill, Some(RED.into()), "back to the foreground colour");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(pattern(&app, 0), p, "undo");

    let path = dir.join("tiles.vstudio");
    app.script.borrow_mut().save_paths.push_back(path.clone());
    app.shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let back = vs_io::open_document(&path).unwrap();
    assert_eq!(back.active_page().objects[0].style.fill, Some(Fill::Pattern(p)), "saved with the drawing");
}

#[test]
fn bitmap_pattern_fill_repeats_a_chosen_image() {
    let mut app = E2e::new();
    let dir = temp_dir("bitmap-pattern");
    let blue = dir.join("blue.png");
    std::fs::write(&blue, png([0, 0, 255])).unwrap();
    let not_image = dir.join("notes.png");
    std::fs::write(&not_image, "not an image").unwrap();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.docker("Properties");

    // Cancelling the file dialog leaves the fill as it was.
    app.click("Bitmap pattern fill (docker)");
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Choose a Bitmap Pattern"));
    assert_eq!(app.objects()[0].style.fill, Some(RED.into()));

    app.script.borrow_mut().open_paths.push_back(blue.clone());
    app.click("Bitmap pattern fill (docker)");
    let p = pattern(&app, 0);
    let PatternKind::Bitmap { bitmap, average } = &p.kind else { panic!("bitmap pattern") };
    assert_eq!(bitmap.data.as_slice(), std::fs::read(&blue).unwrap().as_slice(), "the file's own bytes");
    assert_eq!(*average, Rgba::rgb(0, 0, 255));
    app.click_doc(600.0, 800.0);
    assert!(near(app.pixel(250.0, 180.0), [0, 0, 255], 4), "{:?}", app.pixel(250.0, 180.0));

    // A file that is not an image is refused, and the pattern stays.
    app.click_doc(150.0, 150.0);
    app.script.borrow_mut().open_paths.push_back(not_image);
    app.click("Choose bitmap… (docker)");
    assert_eq!(app.script.borrow().shown.last().map(String::as_str), Some("Can't use this bitmap"));
    assert_eq!(pattern(&app, 0), p);
}

#[test]
fn interactive_fill_drags_the_tiles_and_the_eyedropper_carries_them() {
    let mut app = E2e::new();
    app.draw_rect(100.0, 100.0, 300.0, 200.0);
    app.click("Swatch Red");
    app.docker("Properties");
    app.click("Two-color pattern fill (docker)");
    app.key(Key::G);
    assert_eq!(app.state().tool, Tool::InteractiveFill);
    assert!(app.exists("Two-color pattern fill (property bar)"), "the property bar has the fill types too");

    // The tile's corner moves the tiles; the end of its top side sizes and turns them.
    app.drag_doc((100.0, 100.0), (110.0, 105.0));
    let p = pattern(&app, 0);
    assert!((p.transform.translation() - Vec2::new(110.0, 105.0)).hypot() < 0.1, "{:?}", p.transform);
    assert!(close(p.tile_size().width, 18.0), "same size: {:?}", p.tile_size());
    app.drag_doc((128.0, 105.0), (146.0, 105.0));
    let p = pattern(&app, 0);
    assert!((p.tile_size().width - 36.0).abs() < 0.1 && close(p.tile_size().height, 18.0), "{:?}", p.tile_size());
    app.drag_doc((110.0, 123.0), (110.0, 133.0));
    assert!((pattern(&app, 0).tile_size().height - 28.0).abs() < 0.1, "{:?}", pattern(&app, 0).tile_size());
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(pattern(&app, 0), p, "each drag is one undo step");

    // The eyedropper picks a cell's colour and carries the pattern to another object.
    app.draw_rect(400.0, 100.0, 500.0, 200.0);
    app.click("Swatch Blue");
    app.click_doc(600.0, 800.0);
    app.click("Color Eyedropper");
    app.click_doc(112.0, 107.0);
    assert_eq!(app.state().tools.eyedropper.color, Some(RED), "a foreground cell");
    app.flyout_tool("Color Eyedropper", "Attributes Eyedropper");
    app.click_doc(150.0, 150.0);
    app.click_doc(450.0, 150.0);
    let carried = pattern(&app, 1);
    assert_eq!(carried.kind, p.kind);
    let (a, b) = (carried.tile_size(), p.tile_size());
    assert!(close(a.width, b.width) && close(a.height, b.height), "same tiles: {a:?}");
    assert!((app.objects()[1].transform * carried.transform * Point::ZERO).distance(Point::new(400.0, 100.0)) < 1e-3, "started from the new object's corner");
}

#[test]
fn svg_patterns_import_as_bitmap_pattern_fills() {
    let mut app = E2e::new();
    let dir = temp_dir("svg-pattern");
    let svg = dir.join("tiles.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96">
            <defs><pattern id="p" patternUnits="userSpaceOnUse" width="32" height="32"><rect width="16" height="32" fill="#0000ff"/></pattern></defs>
            <rect width="96" height="96" fill="url(#p)"/>
        </svg>"##,
    )
    .unwrap();
    app.script.borrow_mut().open_paths.push_back(svg);
    app.shortcut(Modifiers::COMMAND, Key::I);
    let p = pattern(&app, 0);
    assert!(matches!(p.kind, PatternKind::Bitmap { .. }), "a vector tile comes in as a bitmap");
    assert_eq!(p.tile_size(), Size::new(24.0, 24.0), "32 px in points");

    // Blue in the left half of every tile.
    let b = app.objects()[0].world_bounds();
    app.click_doc(10.0, 800.0);
    for (dx, blue) in [(6.0, true), (18.0, false), (30.0, true), (42.0, false)] {
        let px = app.pixel(b.x0 + dx, b.center().y);
        let want = if blue { [0, 0, 255] } else { [255, 255, 255] };
        assert!(near(px, want, 8), "{dx} pt in: {px:?}");
    }
}

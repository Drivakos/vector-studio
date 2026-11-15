//! Builds a small logo, saves it, reopens it and exports SVG, PNG and PDF.
//! Usage: cargo run -p vs-io --example sample_logo -- <output dir>

use std::path::PathBuf;

use vs_core::kurbo::{Affine, BezPath, Point, Rect};
use vs_core::{Document, LineCap, LineJoin, LineStyle, Outline, Rgba, Shape, Style};
use vs_io::ExportOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let mut doc = Document::new("Sample Logo");
    let fill = |c: Rgba| Style { fill: Some(c), outline: None };

    doc.add_object(
        Shape::Rectangle { rect: Rect::new(147.6, 270.9, 447.6, 570.9), corner_radius: 48.0 },
        fill(Rgba::rgb(33, 64, 154)),
    );
    let ring = doc.add_object(
        Shape::Ellipse { rect: Rect::new(197.6, 320.9, 397.6, 520.9), arc: None },
        Style { fill: None, outline: Some(Outline::new(Rgba::rgb(255, 237, 0), 12.0)) },
    );
    let star = doc.add_object(
        Shape::Star { rect: Rect::new(232.6, 355.9, 362.6, 485.9), points: 5, inner_ratio: 0.45 },
        fill(Rgba::rgb(227, 6, 19)),
    );
    doc.transform_objects(&[star], Affine::rotate_about(0.15, Point::new(297.6, 420.9)));
    doc.group_objects(&[ring, star]).expect("ring and star group");
    doc.add_object(
        Shape::Polygon { rect: Rect::new(160.0, 600.0, 220.0, 660.0), sides: 6 },
        Style {
            fill: Some(Rgba::rgb(0, 150, 64)),
            outline: Some(Outline { join: LineJoin::Round, behind_fill: true, ..Outline::new(Rgba::BLACK, 6.0) }),
        },
    );
    let mut swoosh = BezPath::new();
    swoosh.move_to((240.0, 640.0));
    swoosh.curve_to((300.0, 590.0), (380.0, 690.0), (440.0, 620.0));
    doc.add_object(
        Shape::Curve { path: swoosh },
        Style {
            fill: None,
            outline: Some(Outline { style: LineStyle::Dash, cap: LineCap::Round, ..Outline::new(Rgba::rgb(243, 146, 0), 6.0) }),
        },
    );

    let native = dir.join("sample-logo.vstudio");
    vs_io::save_document(&doc, &native)?;
    let reopened = vs_io::open_document(&native)?;
    assert_eq!(reopened, doc, "save/open round trip");

    for ext in ["svg", "png", "pdf"] {
        let path = dir.join(format!("sample-logo.{ext}"));
        vs_io::export(&reopened, &path, &ExportOptions { dpi: 150.0, background: Some(Rgba::WHITE) })?;
        println!("{} ({} bytes)", path.display(), std::fs::metadata(&path)?.len());
    }
    println!("{}", native.display());
    Ok(())
}

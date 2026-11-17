//! Object > Insert QR Code and Insert Barcode: a window for the content and size, then
//! the code as black vector shapes in the middle of the page, selected.

use vs_core::kurbo::{Affine, BezPath, Shape as _, Vec2};
use vs_core::{ObjectId, Rgba, Shape, Style, Text, Unit};

use crate::app::AppState;

/// The open Insert QR Code window; the size is in display units.
#[derive(Clone, Debug, PartialEq)]
pub struct QrDialog {
    content: String,
    size: f64,
}

/// The open Insert Barcode window; the sizes are in display units.
#[derive(Clone, Debug, PartialEq)]
pub struct BarcodeDialog {
    content: String,
    bar_width: f64,
    height: f64,
    show_text: bool,
}

pub fn open_qr(state: &mut AppState) {
    let unit = state.doc.display_unit;
    state.insert_qr = Some(QrDialog { content: String::new(), size: unit.from_points(Unit::Millimeters.to_points(30.0)) });
}

pub fn open_barcode(state: &mut AppState) {
    let unit = state.doc.display_unit;
    let mm = |v: f64| unit.from_points(Unit::Millimeters.to_points(v));
    state.insert_barcode = Some(BarcodeDialog { content: String::new(), bar_width: mm(0.4), height: mm(15.0), show_text: true });
}

/// Add `path` as a black curve named `name`, its centre on the page's.
fn add_centred(state: &mut AppState, path: BezPath, name: &str) -> ObjectId {
    let centre = state.doc.active_page().bounds().center() - path.bounding_box().center();
    let id = state.doc.add_object(Shape::Curve { path }, Style::filled(Rgba::BLACK));
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.name = Some(name.to_owned());
        obj.transform = Affine::translate(centre);
    }
    id
}

/// A number field in display units, named `name`.
fn length(ui: &mut egui::Ui, value: &mut f64, unit: Unit, name: &str) {
    let field = egui::DragValue::new(value)
        .range(unit.from_points(0.1)..=unit.from_points(2000.0))
        .speed(unit.step() / 2.0)
        .max_decimals(unit.decimals())
        .suffix(format!(" {}", unit.suffix()));
    crate::a11y::add(ui, field, name);
}

/// The Insert QR Code window: the text or link to encode and the code's width.
pub fn qr_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.insert_qr.take() else { return };
    let unit = state.doc.display_unit;
    let path = vs_core::codes::qr_path(&d.content, 1.0).filter(|_| !d.content.is_empty());
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Insert QR Code")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("insert_qr_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Content:");
                crate::a11y::add(ui, egui::TextEdit::singleline(&mut d.content).hint_text("Text or link").desired_width(240.0), "QR code content");
                ui.end_row();
                ui.label("Size:");
                length(ui, &mut d.size, unit, "QR code size");
                ui.end_row();
            });
            if path.is_none() && !d.content.is_empty() {
                ui.label("Too long for a QR code.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, path.is_some(), "OK", "Insert QR Code").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Insert QR Code").clicked();
            });
        });
    if ok && let Some((modules, _)) = vs_core::codes::qr_modules(&d.content) {
        let module = unit.to_points(d.size) / modules as f64;
        if let Some(path) = vs_core::codes::qr_path(&d.content, module) {
            let id = add_centred(state, path, "QR Code");
            state.selection = vec![id];
        }
        return;
    }
    if open && !cancel {
        state.insert_qr = Some(d);
    }
}

/// The Insert Barcode window: the text to encode (Code 128), bar width, height and
/// whether the text shows under the bars.
pub fn barcode_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.insert_barcode.take() else { return };
    let unit = state.doc.display_unit;
    let valid = vs_core::codes::code128_values(&d.content).is_some();
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Insert Barcode")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("insert_barcode_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Format:");
                ui.label("Code 128");
                ui.end_row();
                ui.label("Content:");
                crate::a11y::add(ui, egui::TextEdit::singleline(&mut d.content).desired_width(240.0), "Barcode content");
                ui.end_row();
                ui.label("Bar width:");
                length(ui, &mut d.bar_width, unit, "Bar width");
                ui.end_row();
                ui.label("Height:");
                length(ui, &mut d.height, unit, "Barcode height");
                ui.end_row();
            });
            crate::a11y::add(ui, egui::Checkbox::new(&mut d.show_text, "Show text"), "Show text");
            if !valid && !d.content.is_empty() {
                ui.label("Use letters, digits, spaces and ASCII punctuation only.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, valid, "OK", "Insert Barcode").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Insert Barcode").clicked();
            });
        });
    if ok && let Some(path) = vs_core::codes::barcode_path(&d.content, unit.to_points(d.bar_width), unit.to_points(d.height)) {
        insert_barcode(state, path, &d.content, d.show_text);
        return;
    }
    if open && !cancel {
        state.insert_barcode = Some(d);
    }
}

/// Add the bars, and the text centred under them when `show_text`, grouped.
fn insert_barcode(state: &mut AppState, bars: BezPath, content: &str, show_text: bool) {
    let bars_id = add_centred(state, bars, "Barcode");
    if !show_text {
        state.selection = vec![bars_id];
        return;
    }
    let below = state.doc.active_page().object(bars_id).map(|o| o.world_bounds()).unwrap_or_default();
    let mut text = Text::new(content);
    text.size = (below.height() / 4.0).clamp(4.0, 24.0);
    let text_id = state.doc.add_object(Shape::Text(text), Style::new_text());
    let page = state.doc.active_page_mut();
    if let Some(obj) = page.object_mut(text_id) {
        let r = obj.world_bounds();
        let gap = r.height() * 0.15;
        obj.transform = Affine::translate(Vec2::new(below.center().x - r.center().x, below.y1 + gap - r.y0)) * obj.transform;
    }
    let group = state.doc.group_objects(&[bars_id, text_id]);
    if let Some(id) = group
        && let Some(obj) = state.doc.active_page_mut().object_mut(id)
    {
        obj.name = Some("Barcode".to_owned());
    }
    state.selection = vec![group.unwrap_or(bars_id)];
}

//! Bitmaps > Convert to Bitmap and Resample, and the bitmap effects of the Effects menu
//! (Adjust, Blur, Distort, Noise and Sharpen).

use vs_core::raster::{Effect, EffectKind};
use vs_core::{Object, ObjectId, Rgba, Shape, Style};

use crate::app::AppState;

/// The selected unlocked bitmaps: what effects and Resample work on.
pub fn bitmap_targets(state: &AppState) -> Vec<ObjectId> {
    state.selected_objects().filter(|o| !o.locked && matches!(o.shape, Shape::Image(_))).map(|o| o.id).collect()
}

/// The selected unlocked objects, bottom of the stack first: what Convert to Bitmap draws.
fn convert_targets(state: &AppState) -> Vec<&Object> {
    state.doc.active_page().objects.iter().filter(|o| !o.locked && state.selection.contains(&o.id)).collect()
}

pub fn can_convert(state: &AppState) -> bool {
    !convert_targets(state).is_empty()
}

/// The open Convert to Bitmap window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvertDialog {
    dpi: f64,
    transparent: bool,
}

pub fn open_convert(state: &mut AppState) {
    state.convert_to_bitmap = Some(ConvertDialog { dpi: 300.0, transparent: true });
}

/// Draw the selected objects into one bitmap that takes the top one's place in the stack.
/// True once done.
fn convert(state: &mut AppState, d: ConvertDialog) -> bool {
    let targets = convert_targets(state);
    let Some(top) = targets.last().map(|o| o.id) else { return false };
    let background = (!d.transparent).then_some(Rgba::WHITE);
    let bitmap = match vs_io::objects_to_bitmap(&targets, d.dpi, background) {
        Ok(b) => b,
        Err(e) => {
            state.dialogs.error("Could not convert to bitmap", &e.to_string());
            return false;
        }
    };
    let removed: Vec<ObjectId> = targets.iter().map(|o| o.id).filter(|id| *id != top).collect();
    let page = state.doc.active_page_mut();
    page.objects.retain(|o| !removed.contains(&o.id));
    if let Some(obj) = page.object_mut(top) {
        obj.shape = Shape::Image(bitmap);
        obj.style = Style::default();
        obj.transform = vs_core::kurbo::Affine::IDENTITY;
        obj.contents.clear();
        obj.clone_of = None;
        obj.frame = false;
    }
    state.selection = vec![top];
    true
}

/// The Convert to Bitmap window: resolution and background, then OK.
pub fn convert_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.convert_to_bitmap.take() else { return };
    let size = vs_io::drawn_bounds(&convert_targets(state));
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Convert to Bitmap")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("convert_bitmap_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Resolution:");
                crate::a11y::add(ui, egui::DragValue::new(&mut d.dpi).range(18.0..=1200.0).speed(1.0).suffix(" dpi"), "Resolution (bitmap)");
                ui.end_row();
                ui.label("Background:");
                crate::a11y::add(ui, egui::Checkbox::new(&mut d.transparent, "Transparent"), "Transparent background");
                ui.end_row();
                if let Some(r) = size {
                    let k = d.dpi / 72.0;
                    ui.label("Image size:");
                    ui.label(format!("{} × {} px", (r.width() * k).ceil(), (r.height() * k).ceil()));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, size.is_some(), "OK", "Convert to Bitmap").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Convert to Bitmap").clicked();
            });
        });
    if ok && convert(state, d) {
        return;
    }
    if open && !cancel {
        state.convert_to_bitmap = Some(d);
    }
}

/// The open Resample window: the new size in pixels, and the bitmap's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResampleDialog {
    id: ObjectId,
    width: u32,
    height: u32,
    original: (u32, u32),
    keep_ratio: bool,
}

/// The one selected bitmap Resample works on.
pub fn resample_target(state: &AppState) -> Option<(ObjectId, u32, u32)> {
    let [id] = bitmap_targets(state)[..] else { return None };
    match &state.doc.active_page().object(id)?.shape {
        Shape::Image(b) => Some((id, b.width, b.height)),
        _ => None,
    }
}

pub fn open_resample(state: &mut AppState) {
    if let Some((id, width, height)) = resample_target(state) {
        state.resample = Some(ResampleDialog { id, width, height, original: (width, height), keep_ratio: true });
    }
}

/// The Resample window: width and height in pixels, keeping the proportions unless
/// unticked. The bitmap keeps its size on the page; its resolution changes.
pub fn resample_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.resample.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Resample")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let (ow, oh) = d.original;
            egui::Grid::new("resample_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Original:");
                ui.label(format!("{ow} × {oh} px"));
                ui.end_row();
                ui.label("Width:");
                let width = crate::a11y::add(ui, egui::DragValue::new(&mut d.width).range(1..=20_000).suffix(" px"), "Width (resample)");
                ui.end_row();
                ui.label("Height:");
                let height = crate::a11y::add(ui, egui::DragValue::new(&mut d.height).range(1..=20_000).suffix(" px"), "Height (resample)");
                ui.end_row();
                if d.keep_ratio && width.changed() {
                    d.height = (f64::from(d.width) * f64::from(oh) / f64::from(ow)).round().max(1.0) as u32;
                } else if d.keep_ratio && height.changed() {
                    d.width = (f64::from(d.height) * f64::from(ow) / f64::from(oh)).round().max(1.0) as u32;
                }
            });
            crate::a11y::add(ui, egui::Checkbox::new(&mut d.keep_ratio, "Maintain aspect ratio"), "Maintain aspect ratio");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", "Resample").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Resample").clicked();
            });
        });
    if ok {
        let page = state.doc.active_page_mut();
        if let Some(obj) = page.object_mut(d.id)
            && let Shape::Image(b) = &obj.shape
        {
            match vs_io::resample(b, d.width, d.height) {
                Ok(new) => obj.shape = Shape::Image(new),
                Err(e) => state.dialogs.error("Could not resample", &e.to_string()),
            }
        }
    } else if open && !cancel {
        state.resample = Some(d);
    }
}

/// Apply `effect` to every selected unlocked bitmap.
pub fn apply_effect(state: &mut AppState, effect: &Effect) {
    let ids = bitmap_targets(state);
    let mut failed = None;
    let page = state.doc.active_page_mut();
    for id in ids {
        if let Some(obj) = page.object_mut(id)
            && let Shape::Image(b) = &obj.shape
        {
            match vs_io::apply_effect(b, effect) {
                Ok(new) => obj.shape = Shape::Image(new),
                Err(e) => failed = Some(e.to_string()),
            }
        }
    }
    if let Some(e) = failed {
        state.dialogs.error(&format!("Could not apply {}", effect.kind().label()), &e);
    }
}

/// Run an Effects menu bitmap effect: at once when it has no settings, else open its
/// window.
pub fn run_effect(state: &mut AppState, kind: EffectKind) {
    if kind.has_settings() {
        state.effect = Some(kind.default_effect());
    } else {
        apply_effect(state, &kind.default_effect());
    }
}

/// A labelled number field in the effect window's grid, named "`label` (`window`)".
fn field(ui: &mut egui::Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str, label: &str, window: &str) {
    ui.label(format!("{label}:"));
    let speed = (range.end() - range.start()) / 200.0;
    crate::a11y::add(ui, egui::DragValue::new(value).range(range).speed(speed).max_decimals(1).suffix(suffix), &format!("{label} ({window})"));
    ui.end_row();
}

/// The settings of one effect.
fn settings(ui: &mut egui::Ui, effect: &mut Effect, window: &str) {
    let pct = -100.0..=100.0;
    match effect {
        Effect::AutoAdjust | Effect::Grayscale => {}
        Effect::BrightnessContrast { brightness, contrast, intensity } => {
            field(ui, brightness, pct.clone(), "", "Brightness", window);
            field(ui, contrast, pct.clone(), "", "Contrast", window);
            field(ui, intensity, pct, "", "Intensity", window);
        }
        Effect::ColorBalance { cyan_red, magenta_green, yellow_blue } => {
            field(ui, cyan_red, pct.clone(), "", "Cyan–Red", window);
            field(ui, magenta_green, pct.clone(), "", "Magenta–Green", window);
            field(ui, yellow_blue, pct, "", "Yellow–Blue", window);
        }
        Effect::HueSaturation { hue, saturation, lightness } => {
            field(ui, hue, -180.0..=180.0, "°", "Hue", window);
            field(ui, saturation, pct.clone(), "", "Saturation", window);
            field(ui, lightness, pct, "", "Lightness", window);
        }
        Effect::GaussianBlur { radius } => field(ui, radius, 0.0..=100.0, " px", "Radius", window),
        Effect::MotionBlur { distance, angle } => {
            field(ui, distance, 0.0..=200.0, " px", "Distance", window);
            field(ui, angle, -180.0..=180.0, "°", "Direction", window);
        }
        Effect::RadialBlur { angle } => field(ui, angle, 0.0..=90.0, "°", "Amount", window),
        Effect::SmartBlur { amount } => field(ui, amount, 0.0..=100.0, "", "Amount", window),
        Effect::Pixelate { size } => {
            ui.label("Cell size:");
            crate::a11y::add(ui, egui::DragValue::new(size).range(1..=100).speed(0.2).suffix(" px"), &format!("Cell size ({window})"));
            ui.end_row();
        }
        Effect::Ripple { amplitude, period } => {
            field(ui, amplitude, 0.0..=100.0, " px", "Amplitude", window);
            field(ui, period, 2.0..=500.0, " px", "Period", window);
        }
        Effect::Swirl { angle } => field(ui, angle, -720.0..=720.0, "°", "Angle", window),
        Effect::Wind { strength, angle } => {
            field(ui, strength, 0.0..=100.0, " px", "Strength", window);
            field(ui, angle, -180.0..=180.0, "°", "Direction", window);
        }
        Effect::AddNoise { level } => field(ui, level, 0.0..=100.0, "", "Level", window),
        Effect::RemoveNoise { radius } => {
            ui.label("Radius:");
            crate::a11y::add(ui, egui::DragValue::new(radius).range(1..=5).speed(0.05).suffix(" px"), &format!("Radius ({window})"));
            ui.end_row();
        }
        Effect::Sharpen { amount } => field(ui, amount, 0.0..=100.0, "", "Amount", window),
        Effect::UnsharpMask { amount, radius, threshold } => {
            field(ui, amount, 0.0..=500.0, "%", "Amount", window);
            field(ui, radius, 0.5..=50.0, " px", "Radius", window);
            field(ui, threshold, 0.0..=255.0, "", "Threshold", window);
        }
    }
}

/// The window of an effect with settings: its settings, then OK.
pub fn effect_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut effect) = state.effect.take() else { return };
    let title = effect.kind().label();
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("effect_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| settings(ui, &mut effect, title));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", title).clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", title).clicked();
            });
        });
    if ok {
        apply_effect(state, &effect);
    } else if open && !cancel {
        state.effect = Some(effect);
    }
}

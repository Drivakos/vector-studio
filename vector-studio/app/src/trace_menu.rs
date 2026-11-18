//! Bitmaps > Trace Bitmap: Quick Trace, Centerline Trace… and Outline Trace…. Each selected
//! bitmap is traced; the curves go just above it, grouped, and are selected. The trace
//! windows show a preview of the first selected bitmap traced with the settings as they
//! stand.

use vs_core::raster::Pixels;
use vs_core::trace::{MAX_COLORS, TraceMethod, TraceSettings};
use vs_core::{ObjectId, Rgba, Shape};

use crate::app::AppState;
use crate::bitmap_menu::bitmap_targets;

/// Bitmaps longer than this along either side, in pixels, are traced at this size. This
/// app's choice, to keep tracing quick.
pub const MAX_SIDE: u32 = 2000;

/// Longest side of a trace window's preview, in pixels.
const PREVIEW: u32 = 240;

/// The trace window, if open, and the settings the windows remember.
pub struct TraceState {
    pub dialog: Option<TraceDialog>,
    /// Outline Trace's settings as last confirmed: Quick Trace uses them too.
    pub outline: TraceSettings,
    pub centerline: TraceSettings,
    pub delete_original: bool,
}

impl Default for TraceState {
    fn default() -> Self {
        Self {
            dialog: None,
            outline: TraceMethod::Outline.default_preset().settings(),
            centerline: TraceMethod::Centerline.default_preset().settings(),
            delete_original: false,
        }
    }
}

/// The open Outline Trace or Centerline Trace window.
pub struct TraceDialog {
    pub settings: TraceSettings,
    pub delete_original: bool,
    /// The first target bitmap, shrunk for the preview.
    source: Option<Pixels>,
    /// The settings the preview was traced with, and the preview.
    shown: Option<(TraceSettings, egui::TextureHandle)>,
}

impl TraceDialog {
    /// The settings the preview shows, once it has been drawn.
    pub fn previewed(&self) -> Option<TraceSettings> {
        self.shown.as_ref().map(|(s, _)| *s)
    }
}

pub fn can_trace(state: &AppState) -> bool {
    !bitmap_targets(state).is_empty()
}

/// Bitmap `id` decoded, shrunk to `longest` pixels along its longer side if bigger.
fn pixels_of(state: &AppState, id: ObjectId, longest: u32) -> Result<Pixels, String> {
    match state.doc.active_page().object(id).map(|o| &o.shape) {
        Some(Shape::Image(b)) => vs_io::bitmap_pixels(b).map(|p| vs_io::shrink(&p, longest).0).map_err(|e| e.to_string()),
        _ => Err("not a bitmap".into()),
    }
}

/// Trace every selected unlocked bitmap with `settings`, and select what was traced.
fn trace_selected(state: &mut AppState, settings: &TraceSettings, delete_original: bool) {
    let mut made = Vec::new();
    let mut failed = None;
    for id in bitmap_targets(state) {
        match pixels_of(state, id, MAX_SIDE) {
            Ok(pixels) => made.extend(state.doc.trace_bitmap(id, &pixels, settings, delete_original)),
            Err(e) => failed = Some(e),
        }
    }
    if let Some(e) = failed {
        state.dialogs.error("Could not trace the bitmap", &e);
    } else if made.is_empty() {
        state.dialogs.error("Nothing to trace", "With these settings nothing of the bitmap is left to trace.");
    }
    if !made.is_empty() {
        state.doc.settle_layers();
        state.selection = made;
    }
}

/// Quick Trace: trace at once, with Outline Trace's settings as last confirmed.
pub fn quick_trace(state: &mut AppState) {
    let (settings, delete) = (state.trace.outline, state.trace.delete_original);
    trace_selected(state, &settings, delete);
}

/// Open the Outline Trace or Centerline Trace window.
pub fn open(state: &mut AppState, method: TraceMethod) {
    let settings = match method {
        TraceMethod::Outline => state.trace.outline,
        TraceMethod::Centerline => state.trace.centerline,
    };
    let source = bitmap_targets(state).first().and_then(|id| pixels_of(state, *id, PREVIEW).ok());
    state.trace.dialog = Some(TraceDialog { settings, delete_original: state.trace.delete_original, source, shown: None });
}

/// A slider from 0 to 100 in the trace window's grid, named "`label` (`window`)".
fn slider(ui: &mut egui::Ui, value: &mut f64, label: &str, window: &str) {
    ui.label(format!("{label}:"));
    crate::a11y::add(ui, egui::Slider::new(value, 0.0..=100.0).max_decimals(0), &format!("{label} ({window})"));
    ui.end_row();
}

/// A tick box named "`label` (`window`)".
fn tick(ui: &mut egui::Ui, enabled: bool, flag: &mut bool, label: &str, window: &str) {
    let resp = ui.add_enabled(enabled, egui::Checkbox::new(flag, label));
    crate::a11y::name(&resp, &format!("{label} ({window})"));
}

/// The settings: the kind of image, the sliders, the colours and the tick boxes.
fn settings(ui: &mut egui::Ui, d: &mut TraceDialog, window: &str) {
    let method = d.settings.method();
    egui::Grid::new("trace_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Type of image:");
        let combo = egui::ComboBox::from_id_salt("trace_preset").selected_text(d.settings.preset.label()).show_ui(ui, |ui| {
            for &preset in method.presets() {
                if ui.selectable_label(preset == d.settings.preset, preset.label()).clicked() {
                    // A kind of image brings its own sliders and colours; the tick boxes stay.
                    let s = d.settings;
                    d.settings = TraceSettings { remove_background: s.remove_background, remove_entire: s.remove_entire, remove_overlap: s.remove_overlap, ..preset.settings() };
                }
            }
        });
        crate::a11y::name(&combo.response, &format!("Type of image ({window})"));
        ui.end_row();
        slider(ui, &mut d.settings.detail, "Detail", window);
        slider(ui, &mut d.settings.smoothing, "Smoothing", window);
        slider(ui, &mut d.settings.corner_smoothness, "Corner smoothness", window);
        ui.label("Colors:");
        crate::a11y::add(ui, egui::DragValue::new(&mut d.settings.colors).range(1..=MAX_COLORS).speed(0.2), &format!("Colors ({window})"));
        ui.end_row();
    });
    ui.add_space(4.0);
    let s = &mut d.settings;
    tick(ui, true, &mut s.remove_background, "Remove background", window);
    let background = s.remove_background;
    ui.indent("trace_background", |ui| tick(ui, background, &mut s.remove_entire, "Remove color from entire image", window));
    if method == TraceMethod::Outline {
        tick(ui, true, &mut s.remove_overlap, "Remove object overlap", window);
    }
    tick(ui, true, &mut d.delete_original, "Delete original image", window);
}

/// The preview: the first bitmap traced, on white, made again when the settings change.
fn preview(ui: &mut egui::Ui, d: &mut TraceDialog, window: &str) {
    let Some(source) = &d.source else { return };
    if d.shown.as_ref().is_none_or(|(s, _)| *s != d.settings) {
        let objects = vs_core::trace::preview_objects(source, &d.settings);
        let refs: Vec<&vs_core::Object> = objects.iter().collect();
        let Ok(out) = vs_io::rasterize(&refs, vs_core::kurbo::Affine::IDENTITY, source.width, source.height, Some(Rgba::WHITE)) else { return };
        let image = egui::ColorImage::from_rgba_unmultiplied([out.width as usize, out.height as usize], &out.data);
        let texture = ui.ctx().load_texture("trace_preview", image, egui::TextureOptions::LINEAR);
        d.shown = Some((d.settings, texture));
    }
    let Some((_, texture)) = &d.shown else { return };
    let size = egui::vec2(source.width as f32, source.height as f32) * (PREVIEW as f32 / source.width.max(source.height) as f32);
    let resp = ui.add(egui::Image::new(texture).fit_to_exact_size(size));
    crate::a11y::name(&resp, &format!("Preview ({window})"));
}

/// The Outline Trace or Centerline Trace window: its settings and a preview, then OK.
pub fn window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.trace.dialog.take() else { return };
    let method = d.settings.method();
    let title = method.label();
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| settings(ui, &mut d, title));
                preview(ui, &mut d, title);
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", title).clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", title).clicked();
            });
        });
    if ok {
        match method {
            TraceMethod::Outline => state.trace.outline = d.settings,
            TraceMethod::Centerline => state.trace.centerline = d.settings,
        }
        state.trace.delete_original = d.delete_original;
        trace_selected(state, &d.settings, d.delete_original);
    } else if open && !cancel {
        state.trace.dialog = Some(d);
    }
}

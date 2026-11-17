//! Bitmaps > Convert to Bitmap, Resample, Straighten Image and Bitmap Color Mask, and the
//! bitmap effects of the Effects menu (Adjust, Blur, Distort, Noise and Sharpen). Effect
//! windows show a preview of the first selected bitmap with the settings as they stand.

use vs_core::raster::{Effect, EffectKind, Pixels, TONE_INPUTS, ToneChannel};
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

/// Longest side of an effect window's preview, in pixels.
const PREVIEW: u32 = 240;

/// The open window of an effect with settings.
pub struct EffectDialog {
    effect: Effect,
    /// The first target bitmap, shrunk for the preview, and the scale it was shrunk by.
    source: Option<(Pixels, f64)>,
    /// The settings the preview was made with, and the preview.
    shown: Option<(Effect, egui::TextureHandle)>,
    /// Bitmap Color Mask: the colour as typed, kept while it is not a whole colour yet.
    hex: String,
}

impl EffectDialog {
    /// The settings the preview shows, once it has been drawn.
    pub fn previewed(&self) -> Option<Effect> {
        self.shown.as_ref().map(|(e, _)| *e)
    }
}

/// Run an Effects menu bitmap effect: at once when it has no settings, else open its
/// window.
pub fn run_effect(state: &mut AppState, kind: EffectKind) {
    if kind.has_settings() {
        let page = state.doc.active_page();
        let source = bitmap_targets(state).first().and_then(|id| match page.object(*id).map(|o| &o.shape) {
            Some(Shape::Image(b)) => vs_io::bitmap_pixels(b).ok().map(|p| vs_io::shrink(&p, PREVIEW)),
            _ => None,
        });
        let effect = kind.default_effect();
        let hex = match effect {
            Effect::ColorMask { color, .. } => hex(color),
            _ => String::new(),
        };
        state.effect = Some(EffectDialog { effect, source, shown: None, hex });
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
        Effect::AutoAdjust | Effect::Grayscale | Effect::ToneCurve { .. } | Effect::ColorMask { .. } => {}
        Effect::Straighten { angle } => field(ui, angle, -15.0..=15.0, "°", "Angle", window),
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

/// The names of a tone curve's nodes, black to white.
const TONE_NODES: [&str; 5] = ["Black", "Shadows", "Midtones", "Highlights", "White"];

/// The Tone Curve settings: the channels it changes, the curve (drag a node up or down),
/// and each node's output level.
fn tone_curve(ui: &mut egui::Ui, outputs: &mut [f64; 5], channel: &mut ToneChannel, window: &str) {
    ui.horizontal(|ui| {
        ui.label("Channel:");
        for c in ToneChannel::ALL {
            let resp = ui.selectable_label(*channel == c, c.label());
            crate::a11y::toggle(&resp, true, *channel == c, &format!("{} ({window})", c.label()));
            if resp.clicked() {
                *channel = c;
            }
        }
    });
    let size = egui::vec2(200.0, 200.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::drag());
    crate::a11y::name(&resp, &format!("Curve ({window})"));
    let to_screen = |input: f64, output: f64| egui::pos2(rect.left() + (input / 255.0) as f32 * rect.width(), rect.bottom() - (output / 255.0) as f32 * rect.height());
    if resp.dragged()
        && let (Some(origin), Some(pos)) = (ui.input(|i| i.pointer.press_origin()), resp.interact_pointer_pos())
    {
        // The node nearest where the drag began follows the pointer up and down.
        let input = f64::from((origin.x - rect.left()) / rect.width()) * 255.0;
        let node = (0..5).min_by(|a, b| (TONE_INPUTS[*a] - input).abs().total_cmp(&(TONE_INPUTS[*b] - input).abs())).unwrap_or(0);
        outputs[node] = (f64::from((rect.bottom() - pos.y) / rect.height()) * 255.0).clamp(0.0, 255.0).round();
    }
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let faint = egui::Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4));
    for i in 1..4 {
        let t = i as f32 / 4.0;
        painter.vline(rect.left() + t * rect.width(), rect.y_range(), faint);
        painter.hline(rect.x_range(), rect.top() + t * rect.height(), faint);
    }
    painter.line_segment([rect.left_bottom(), rect.right_top()], faint);
    let lut = vs_core::raster::tone_lut(*outputs);
    let points: Vec<egui::Pos2> = lut.iter().enumerate().map(|(i, v)| to_screen(i as f64, f64::from(*v))).collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, visuals.strong_text_color())));
    for (input, output) in TONE_INPUTS.iter().zip(outputs.iter()) {
        painter.circle(to_screen(*input, *output), 4.0, visuals.selection.bg_fill, egui::Stroke::new(1.0, visuals.strong_text_color()));
    }
    egui::Grid::new("tone_nodes").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        for (name, output) in TONE_NODES.iter().zip(outputs.iter_mut()) {
            ui.label(format!("{name}:"));
            crate::a11y::add(ui, egui::DragValue::new(output).range(0.0..=255.0).speed(1.0).max_decimals(0), &format!("{name} ({window})"));
            ui.end_row();
        }
    });
}

/// `color` as "#RRGGBB".
fn hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// "#RRGGBB" or "RRGGBB" as a colour.
fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// The Bitmap Color Mask settings: hide the colour or keep only it, the colour (picked,
/// typed, or clicked on the preview) and how near a pixel must be to count.
fn color_mask(ui: &mut egui::Ui, d: &mut EffectDialog, window: &str) {
    let Effect::ColorMask { color, tolerance, hide } = &mut d.effect else { return };
    ui.horizontal(|ui| {
        for (label, value) in [("Hide colors", true), ("Show colors", false)] {
            let resp = ui.selectable_label(*hide == value, label);
            crate::a11y::toggle(&resp, true, *hide == value, &format!("{label} ({window})"));
            if resp.clicked() {
                *hide = value;
            }
        }
    });
    egui::Grid::new("color_mask_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Color:");
        ui.horizontal(|ui| {
            if egui::color_picker::color_edit_button_srgb(ui, color).changed() {
                d.hex = hex(*color);
            }
            let resp = crate::a11y::add(ui, egui::TextEdit::singleline(&mut d.hex).desired_width(80.0), &format!("Color ({window})"));
            if resp.changed()
                && let Some(c) = parse_hex(&d.hex)
            {
                *color = c;
            }
            if resp.lost_focus() {
                d.hex = hex(*color);
            }
        });
        ui.end_row();
        field(ui, tolerance, 0.0..=100.0, "%", "Tolerance", window);
    });
    ui.label(egui::RichText::new("Click the preview to pick a color").weak());
}

/// The preview of `d`'s effect on its source, made again when the settings changed.
/// Clicking it picks the colour under the pointer for Bitmap Color Mask; for Straighten
/// Image a grid over it helps line things up.
fn preview(ui: &mut egui::Ui, d: &mut EffectDialog, window: &str) {
    let Some((source, factor)) = &d.source else { return };
    if d.shown.as_ref().is_none_or(|(e, _)| *e != d.effect) {
        let out = d.effect.scaled(*factor).apply(source);
        let image = egui::ColorImage::from_rgba_unmultiplied([out.width as usize, out.height as usize], &out.data);
        let texture = ui.ctx().load_texture("effect_preview", image, egui::TextureOptions::LINEAR);
        d.shown = Some((d.effect, texture));
    }
    let Some((_, texture)) = &d.shown else { return };
    let resp = ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(PREVIEW as f32, PREVIEW as f32)).sense(egui::Sense::click()));
    crate::a11y::name(&resp, &format!("Preview ({window})"));
    let rect = resp.rect;
    if let Effect::ColorMask { color, .. } = &mut d.effect
        && let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked())
    {
        let at = (pos - rect.min) / rect.size();
        let x = ((at.x * source.width as f32) as u32).min(source.width - 1);
        let y = ((at.y * source.height as f32) as u32).min(source.height - 1);
        let [r, g, b, _] = source.pixel(x, y);
        *color = [r, g, b];
        d.hex = hex(*color);
    }
    if matches!(d.effect, Effect::Straighten { .. }) {
        let line = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(140));
        for i in 1..6 {
            let t = i as f32 / 6.0;
            ui.painter().vline(rect.left() + t * rect.width(), rect.y_range(), line);
            ui.painter().hline(rect.x_range(), rect.top() + t * rect.height(), line);
        }
    }
}

/// The window of an effect with settings: its settings and a preview, then OK.
pub fn effect_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.effect.take() else { return };
    let title = d.effect.kind().label();
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    if let Effect::ToneCurve { outputs, channel } = &mut d.effect {
                        tone_curve(ui, outputs, channel, title);
                    } else if matches!(d.effect, Effect::ColorMask { .. }) {
                        color_mask(ui, &mut d, title);
                    } else {
                        egui::Grid::new("effect_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| settings(ui, &mut d.effect, title));
                    }
                });
                preview(ui, &mut d, title);
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, true, "OK", title).clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", title).clicked();
            });
        });
    if ok {
        apply_effect(state, &d.effect);
    } else if open && !cancel {
        state.effect = Some(d);
    }
}

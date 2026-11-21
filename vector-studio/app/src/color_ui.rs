//! CMYK colour in the UI: the Color docker (colours typed as CMYK inks or RGB), the Color
//! Proof Settings docker, and the CMYK profile pickers.

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use vs_core::{ColorSettings, ObjectId, RenderingIntent, Rgba};
use vs_render::{Cmyk, ProfileInfo};

use crate::app::AppState;
use crate::files::Dialogs;

thread_local! {
    /// The CMYK profiles installed on this computer, read once.
    static PROFILES: OnceCell<Vec<ProfileInfo>> = const { OnceCell::new() };
    /// The document's sRGB ⇄ CMYK conversion, made again when its settings change.
    static CONVERTER: RefCell<Option<(ColorSettings, Option<Rc<Cmyk>>)>> = const { RefCell::new(None) };
}

fn installed() -> Vec<ProfileInfo> {
    PROFILES.with(|p| p.get_or_init(vs_render::cmyk_profiles).clone())
}

/// The conversion through the document's CMYK profile and intent.
pub fn converter(settings: &ColorSettings) -> Option<Rc<Cmyk>> {
    CONVERTER.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some((s, c)) = cell.as_ref()
            && s == settings
        {
            return c.clone();
        }
        let made = Cmyk::for_document(settings).map(Rc::new);
        *cell = Some((settings.clone(), made.clone()));
        made
    })
}

/// The name to show for a chosen CMYK profile; `None` is the generic one.
pub fn profile_label(path: Option<&Path>) -> String {
    let generic = || vs_render::profile_name(Path::new(vs_render::GENERIC_CMYK)).unwrap_or_else(|| "Generic CMYK Profile".into());
    match path {
        None => generic(),
        Some(p) => vs_render::profile_name(p).unwrap_or_else(|| {
            let file = p.file_name().map_or_else(|| p.display().to_string(), |f| f.to_string_lossy().into_owned());
            format!("{file} (not found: {} used)", generic())
        }),
    }
}

/// A list of the installed CMYK profiles, plus Other… to pick a profile file. `none` is
/// what `None` is called, or `None` when it stands for the generic profile. True when
/// `current` changed.
pub fn profile_combo(ui: &mut egui::Ui, dialogs: &mut dyn Dialogs, name: &str, current: &mut Option<PathBuf>, none: Option<&str>, width: f32) -> bool {
    let generic = Path::new(vs_render::GENERIC_CMYK);
    let shown = match (current.as_deref(), none) {
        (None, Some(label)) => label.to_owned(),
        (p, _) => profile_label(p),
    };
    let mut picked: Option<Option<PathBuf>> = None;
    let mut browse = false;
    let combo = egui::ComboBox::from_id_salt(name).selected_text(shown).width(width).truncate().height(400.0).show_ui(ui, |ui| {
        if let Some(label) = none
            && ui.selectable_label(current.is_none(), label).clicked()
        {
            picked = Some(None);
        }
        for p in installed() {
            // Without a separate "none" entry, the generic profile is `None`.
            let value = if none.is_none() && p.path == generic { None } else { Some(p.path.clone()) };
            if ui.selectable_label(*current == value, &p.name).clicked() {
                picked = Some(value);
            }
        }
        if ui.selectable_label(false, "Other…").clicked() {
            browse = true;
        }
    });
    crate::a11y::name(&combo.response, name);
    if browse && let Some(path) = dialogs.open_file("Choose a CMYK Profile", "ICC profiles", &["icc", "icm"]) {
        if vs_render::profile_name(&path).is_some() {
            picked = Some(Some(path));
        } else {
            dialogs.error("Not a CMYK profile", &format!("{} is not a CMYK ICC profile.", path.display()));
        }
    }
    match picked {
        Some(p) if p != *current => {
            *current = p;
            true
        }
        _ => false,
    }
}

pub fn intent_combo(ui: &mut egui::Ui, name: &str, intent: &mut RenderingIntent, width: f32) -> bool {
    let was = *intent;
    let combo = egui::ComboBox::from_id_salt(name).selected_text(intent.label()).width(width).show_ui(ui, |ui| {
        for i in RenderingIntent::ALL {
            ui.selectable_value(intent, i, i.label());
        }
    });
    crate::a11y::name(&combo.response, name);
    *intent != was
}

// ---------------------------------------------------------------------------------------
// Color docker

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorModel {
    #[default]
    Cmyk,
    Rgb,
}

/// The Color docker: a colour to apply, typed as CMYK inks or RGB, which follows the
/// selected object's fill.
#[derive(Clone, Debug)]
pub struct ColorDocker {
    pub model: ColorModel,
    pub color: Rgba,
    hex: String,
    /// The selection the colour was last taken from.
    seen: Vec<ObjectId>,
}

impl Default for ColorDocker {
    fn default() -> Self {
        Self { model: ColorModel::Cmyk, color: Rgba::BLACK, hex: Rgba::BLACK.to_hex(), seen: Vec::new() }
    }
}

impl ColorDocker {
    fn set(&mut self, color: Rgba) {
        self.color = color;
        self.hex = color.to_hex();
    }
}

/// A row of a value field with its label.
fn value_row(ui: &mut egui::Ui, label: &str, value: &mut f64, max: f64, suffix: &str) -> bool {
    ui.label(label);
    let changed = crate::a11y::add(ui, egui::DragValue::new(value).range(0.0..=max).speed(0.5).max_decimals(0).suffix(suffix), &format!("{label} (Color docker)")).changed();
    ui.end_row();
    changed
}

pub fn color_docker_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let mut d = std::mem::take(&mut state.color_docker);
    // Take up the selected object's fill colour when the selection changes.
    if d.seen != state.selection {
        d.seen = state.selection.clone();
        if let Some(c) = state.selected_objects().find_map(|o| o.shown_style().fill.as_ref().map(vs_core::Fill::color)) {
            d.set(c);
        }
    }
    let converter = converter(&state.doc.color);

    ui.horizontal(|ui| {
        for (model, label) in [(ColorModel::Cmyk, "CMYK"), (ColorModel::Rgb, "RGB")] {
            let resp = ui.selectable_label(d.model == model, label);
            crate::a11y::toggle(&resp, true, d.model == model, &format!("{label} model (Color docker)"));
            if resp.clicked() {
                d.model = model;
            }
        }
    });
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(220.0), 36.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(d.color.r, d.color.g, d.color.b));
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, crate::theme::TEXT_DIM), egui::StrokeKind::Inside);
    ui.add_space(4.0);

    egui::Grid::new("color_docker_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| match d.model {
        ColorModel::Cmyk => match &converter {
            Some(k) => {
                let inks = d.color.cmyk.unwrap_or_else(|| k.cmyk(d.color));
                let mut percent = inks.map(vs_core::color_settings::byte_to_ink);
                let mut changed = false;
                for (label, v) in ["Cyan", "Magenta", "Yellow", "Black"].into_iter().zip(&mut percent) {
                    changed |= value_row(ui, label, v, 100.0, " %");
                }
                if changed {
                    let inks = percent.map(vs_core::color_settings::ink_to_byte);
                    let shown = k.rgb(inks);
                    d.set(Rgba { a: d.color.a, ..shown }.with_cmyk(inks));
                }
            }
            None => {
                ui.label(egui::RichText::new("The CMYK profile can't be read").weak());
                ui.end_row();
            }
        },
        ColorModel::Rgb => {
            let mut rgb = [d.color.r, d.color.g, d.color.b].map(f64::from);
            let mut changed = false;
            for (label, v) in ["Red", "Green", "Blue"].into_iter().zip(&mut rgb) {
                changed |= value_row(ui, label, v, 255.0, "");
            }
            if changed {
                let [r, g, b] = rgb.map(|v| v.round().clamp(0.0, 255.0) as u8);
                d.set(Rgba::new(r, g, b, d.color.a));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Hex");
        let resp = crate::a11y::add(ui, egui::TextEdit::singleline(&mut d.hex).desired_width(90.0), "Hex (Color docker)");
        if resp.lost_focus() {
            match Rgba::from_hex(&d.hex) {
                Some(c) if c.to_hex() != d.color.to_hex() => d.set(Rgba { a: d.color.a, ..c }),
                _ => d.hex = d.color.to_hex(),
            }
        }
    });
    ui.add_space(6.0);
    let enabled = state.has_selection();
    ui.horizontal(|ui| {
        let fill = ui.add_enabled(enabled, egui::Button::new("Fill"));
        crate::a11y::button(&fill, enabled, "Fill (Color docker)");
        let outline = ui.add_enabled(enabled, egui::Button::new("Outline"));
        crate::a11y::button(&outline, enabled, "Outline (Color docker)");
        if fill.clicked() {
            crate::chrome::apply_fill_color(state, d.color);
        }
        if outline.clicked() {
            crate::chrome::apply_outline_color(state, d.color);
        }
    });
    if d.color.cmyk.is_some() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("CMYK output keeps these inks exactly.").weak());
    }
    state.color_docker = d;
}

// ---------------------------------------------------------------------------------------
// Color Proof Settings docker

pub fn proof_settings_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let can = crate::canvas::can_proof();
    let mut on = state.proof_colors;
    let resp = ui.add_enabled(can, egui::Checkbox::new(&mut on, "Proof colors"));
    crate::a11y::toggle(&resp, can, on, "Proof colors (Color Proof Settings)");
    if resp.changed() {
        state.proof_colors = on && can;
    }
    ui.add_space(6.0);
    let doc_profile = profile_label(state.doc.color.cmyk_profile.as_deref());
    let AppState { prefs, dialogs, .. } = state;
    let p = &mut prefs.proof;
    // Room for the combo's arrow inside the docker.
    let width = ui.available_width() - 24.0;
    ui.label("Simulate environment");
    profile_combo(ui, dialogs.as_mut(), "Simulate environment (Color Proof Settings)", &mut p.profile, Some(&format!("Document: {doc_profile}")), width);
    ui.add_space(4.0);
    ui.label("Rendering intent");
    intent_combo(ui, "Rendering intent (Color Proof Settings)", &mut p.intent, width);
    ui.add_space(4.0);
    crate::panels::section(ui, "Gamut warning", |ui| {
        crate::a11y::add(ui, egui::Checkbox::new(&mut p.gamut_warning, "Out-of-gamut colors"), "Out-of-gamut colors");
        ui.add_enabled_ui(p.gamut_warning, |ui| {
            egui::Grid::new("gamut_warning_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Color");
                crate::fill_ui::color_button(ui, &mut p.warning_color, "Warning color");
                ui.end_row();
                ui.label("Transparency");
                let mut t = f64::from(p.warning_transparency);
                if crate::a11y::add(ui, egui::DragValue::new(&mut t).range(0.0..=100.0).max_decimals(0).suffix(" %"), "Warning transparency").changed() {
                    p.warning_transparency = t.round().clamp(0.0, 100.0) as u8;
                }
                ui.end_row();
            });
        });
        ui.label(egui::RichText::new("Shows the colours the simulated printing can't reach.").weak());
    });
    if !state.proof_colors {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Turn on Proof colors to see these on the canvas.").weak());
    }
}

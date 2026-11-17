//! Text > Glyphs: every character a font has, as a grid. Click one to pick it, double-click
//! it or press Insert to type it into the text being edited, or else to add it as new text
//! in the middle of the page.

use std::collections::HashMap;

use vs_core::kurbo::{Affine, Vec2};
use vs_core::{Shape, Style, Text};

use crate::app::AppState;

/// Size of a glyph's cell in the grid, in pixels.
const CELL: f32 = 34.0;
const COLUMNS: usize = 10;

/// The open Glyphs window.
pub struct GlyphsDialog {
    family: String,
    style: String,
    /// The font's characters, by code point.
    chars: Vec<char>,
    selected: Option<char>,
    /// Pictures of the glyphs drawn so far.
    images: HashMap<char, egui::TextureHandle>,
}

impl GlyphsDialog {
    fn new(family: String, style: String) -> Self {
        let chars = vs_core::text::characters(&family, &style);
        Self { family, style, chars, selected: None, images: HashMap::new() }
    }

    fn set_font(&mut self, family: String, style: String) {
        if (family.as_str(), style.as_str()) != (self.family.as_str(), self.style.as_str()) {
            *self = Self { selected: self.selected, ..Self::new(family, style) };
        }
    }
}

/// The font the window starts with: the text being edited, else the selected text, else
/// the font for new text.
fn current_font(state: &AppState) -> (String, String) {
    let id = state.tools.text.map(|e| e.id).or_else(|| state.selection.first().copied());
    let text = id.and_then(|id| match state.doc.active_page().object(id).map(|o| &o.shape) {
        Some(Shape::Text(t)) => Some((t.family.clone(), t.style.clone())),
        _ => None,
    });
    let t = &state.tools.settings.text;
    text.unwrap_or_else(|| (t.family.clone(), t.style.clone()))
}

pub fn open(state: &mut AppState) {
    let (family, style) = current_font(state);
    state.glyphs = Some(GlyphsDialog::new(family, style));
}

/// The accessible name of `ch`'s cell, e.g. "U+0041 A".
pub fn cell_name(ch: char) -> String {
    format!("U+{:04X} {ch}", u32::from(ch))
}

/// Type `ch` into the text being edited, or add it as new text in the window's font in
/// the middle of the page, selected.
fn insert(state: &mut AppState, ch: char, family: &str, style: &str) {
    if state.tools.text.is_some() {
        crate::text_tool::insert_text(state, &ch.to_string());
        return;
    }
    let mut text = Text { family: family.to_owned(), style: style.to_owned(), ..state.tools.settings.text.clone() };
    text.content = ch.to_string();
    text.frame = None;
    let id = state.doc.add_object(Shape::Text(text), Style::new_text());
    let centre = state.doc.active_page().bounds().center();
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        let r = obj.world_bounds();
        obj.transform = Affine::translate(Vec2::new(centre.x - r.center().x, centre.y - r.center().y)) * obj.transform;
    }
    state.selection = vec![id];
    state.marquee_selection = None;
}

/// The font picker: family, then style.
fn font_picker(ui: &mut egui::Ui, d: &mut GlyphsDialog) {
    let (mut family, mut style) = (d.family.clone(), d.style.clone());
    ui.horizontal(|ui| {
        let resp = egui::ComboBox::from_id_salt("glyphs_family").selected_text(&family).width(180.0).show_ui(ui, |ui| {
            for f in vs_core::text::families() {
                ui.selectable_value(&mut family, f.clone(), f);
            }
        });
        crate::a11y::name(&resp.response, "Font (Glyphs)");
        let styles = vs_core::text::styles(&family);
        if !styles.contains(&style) {
            style = styles.first().cloned().unwrap_or_default();
        }
        let resp = egui::ComboBox::from_id_salt("glyphs_style").selected_text(&style).width(110.0).show_ui(ui, |ui| {
            for s in styles {
                ui.selectable_value(&mut style, s.clone(), s);
            }
        });
        crate::a11y::name(&resp.response, "Style (Glyphs)");
    });
    d.set_font(family, style);
}

/// The grid of glyphs; returns the character double-clicked, if any.
fn grid(ui: &mut egui::Ui, d: &mut GlyphsDialog) -> Option<char> {
    let mut chosen = None;
    let rows = d.chars.len().div_ceil(COLUMNS);
    let visuals = ui.visuals().clone();
    egui::ScrollArea::vertical().max_height(CELL * 8.0).auto_shrink([false, true]).show_rows(ui, CELL, rows, |ui, range| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        for row in range {
            ui.horizontal(|ui| {
                for &ch in d.chars.iter().skip(row * COLUMNS).take(COLUMNS) {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(CELL, CELL), egui::Sense::click());
                    let selected = d.selected == Some(ch);
                    crate::a11y::toggle(&resp, true, selected, &cell_name(ch));
                    if resp.clicked() || resp.double_clicked() {
                        d.selected = Some(ch);
                    }
                    if resp.double_clicked() {
                        chosen = Some(ch);
                    }
                    let fill = if selected { visuals.selection.bg_fill } else if resp.hovered() { visuals.widgets.hovered.bg_fill } else { visuals.extreme_bg_color };
                    ui.painter().rect(rect.shrink(1.0), 2.0, fill, egui::Stroke::NONE, egui::StrokeKind::Inside);
                    let (family, style) = (&d.family, &d.style);
                    let texture = d.images.entry(ch).or_insert_with(|| {
                        let px = vs_io::glyph_image(family, style, ch, (CELL * 2.0) as u32);
                        let image = egui::ColorImage::from_rgba_unmultiplied([px.width as usize, px.height as usize], &px.data);
                        ui.ctx().load_texture(format!("glyph_{}", u32::from(ch)), image, egui::TextureOptions::LINEAR)
                    });
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    ui.painter().image(texture.id(), rect, uv, visuals.strong_text_color());
                }
            });
        }
    });
    chosen
}

pub fn window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut d) = state.glyphs.take() else { return };
    let mut open = true;
    let mut chosen = None;
    // Below the bars, at the right of the drawing window next to the dockers.
    let at = egui::pos2(ctx.content_rect().right() - 300.0, 170.0);
    let window = egui::Window::new("Glyphs").collapsible(false).resizable(false).pivot(egui::Align2::RIGHT_TOP).default_pos(at);
    window.open(&mut open).show(ctx, |ui| {
        font_picker(ui, &mut d);
        ui.add_space(4.0);
        chosen = grid(ui, &mut d);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let label = d.selected.map_or_else(|| "No character picked".to_owned(), |ch| format!("Character {ch}, code U+{:04X}", u32::from(ch)));
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::a11y::window_button(ui, d.selected.is_some(), "Insert", "Glyphs").clicked() {
                    chosen = d.selected;
                }
            });
        });
    });
    if let Some(ch) = chosen {
        insert(state, ch, &d.family.clone(), &d.style.clone());
    }
    if open {
        state.glyphs = Some(d);
    }
}

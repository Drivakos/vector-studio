//! Fill and transparency controls (Properties docker, Interactive Fill and Transparency
//! tool property bars), and the fill edits they make.

use egui_material_icons::icons::*;
use vs_core::kurbo::{Rect, Size};
use vs_core::{Fill, Gradient, GradientKind, ObjectId, Pattern, PatternKind, Rgba, Style, TWO_COLOR_PATTERNS};

use crate::app::AppState;

/// A change to the fill of several objects. Fountain and pattern fills are placed on each
/// object.
#[derive(Clone, Debug, PartialEq)]
pub enum FillEdit {
    None,
    Solid(Rgba),
    Fountain { kind: GradientKind, from: Rgba, to: Rgba },
    /// A two-color or bitmap pattern with tiles `tile` big (local space).
    Pattern { kind: PatternKind, tile: Size },
}

/// Apply `edit` to one style whose object's local frame is `frame`.
pub fn apply_to(style: &mut Style, frame: Rect, edit: &FillEdit) {
    style.fill = match edit {
        FillEdit::None => None,
        FillEdit::Solid(c) => Some(Fill::Solid(*c)),
        FillEdit::Fountain { kind, from, to } => Some(Fill::Gradient(match style.fill.take() {
            // The same kind keeps where the fill sits; only the colours change.
            Some(Fill::Gradient(mut g)) if g.kind == *kind => {
                if let Some(s) = g.stops.first_mut() {
                    s.color = *from;
                }
                if let Some(s) = g.stops.last_mut() {
                    s.color = *to;
                }
                g
            }
            _ => Gradient::across(*kind, frame, *from, *to),
        })),
        FillEdit::Pattern { kind, tile } => Some(Fill::Pattern(match style.fill.take() {
            // A pattern keeps where its tiles start and how they are turned.
            Some(Fill::Pattern(mut p)) => {
                p.kind = kind.clone();
                p.set_tile_size(*tile);
                p
            }
            _ => Pattern::new(kind.clone(), frame.origin(), *tile),
        })),
    };
}

pub fn apply(state: &mut AppState, ids: &[ObjectId], edit: &FillEdit) {
    state.doc.restyle_objects_framed(ids, |s, frame| apply_to(s, frame, edit));
}

/// The fill edit that describes `fill` as it is.
pub fn current(fill: Option<&Fill>) -> FillEdit {
    match fill {
        None => FillEdit::None,
        Some(Fill::Solid(c)) => FillEdit::Solid(*c),
        Some(Fill::Gradient(g)) => FillEdit::Fountain {
            kind: g.kind,
            from: g.stops.first().map_or(Rgba::BLACK, |s| s.color),
            to: g.stops.last().map_or(Rgba::WHITE, |s| s.color),
        },
        Some(Fill::Pattern(p)) => FillEdit::Pattern { kind: p.kind.clone(), tile: p.tile_size() },
    }
}

pub(crate) fn color_button(ui: &mut egui::Ui, color: &mut Rgba, name: &str) -> bool {
    let mut c = [color.r, color.g, color.b, color.a];
    let resp = ui.color_edit_button_srgba_unmultiplied(&mut c);
    crate::a11y::name(&resp, name);
    let changed = resp.on_hover_text(name).changed();
    if changed {
        *color = Rgba::new(c[0], c[1], c[2], c[3]);
    }
    changed
}

/// The name of a two-color pattern's cells, or "Custom".
fn pattern_name(cells: [u8; 8]) -> &'static str {
    TWO_COLOR_PATTERNS.iter().find(|(_, c)| *c == cells).map_or("Custom", |(n, _)| n)
}

/// A small picture of a two-color pattern's cells.
fn pattern_swatch(ui: &mut egui::Ui, cells: [u8; 8], fore: Rgba, back: Rgba) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let color = |c: Rgba| egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a);
    ui.painter().rect_filled(rect, 0.0, color(back));
    for (y, row) in cells.iter().enumerate() {
        for x in (0..8).filter(|x| row & (0x80 >> x) != 0) {
            let cell = egui::Rect::from_min_size(rect.min + egui::vec2(x as f32 * 2.0, y as f32 * 2.0), egui::vec2(2.0, 2.0));
            ui.painter().rect_filled(cell, 0.0, color(fore));
        }
    }
}

/// A bitmap pattern from a PNG or JPEG file the user picks, or `None` if they cancel or
/// the file can't be used (they are told why).
fn choose_bitmap_pattern(state: &mut AppState) -> Option<PatternKind> {
    let path = state.dialogs.open_file("Choose a Bitmap Pattern", "Images", &["png", "jpg", "jpeg"])?;
    match std::fs::read(&path).map_err(vs_io::IoError::from).and_then(|data| vs_io::bitmap_pattern(&data)) {
        Ok(kind) => Some(kind),
        Err(e) => {
            state.dialogs.error("Can't use this bitmap", &format!("{}: {e}", path.display()));
            None
        }
    }
}

/// No fill / Uniform / Fountain / Two-color pattern / Bitmap pattern, then the colours
/// (and pattern and tile size). Edits the unlocked selected objects. `place` ends every
/// accessible name, e.g. "(docker)".
pub fn fill_controls(ui: &mut egui::Ui, state: &mut AppState, place: &str) {
    let ids = state.selection.clone();
    let Some(first) = state.selected_objects().find(|o| !o.locked) else { return };
    let was = current(first.shown_style().fill.as_ref());
    let mut edit = was.clone();
    let solid_color = match &was {
        FillEdit::Solid(c) => *c,
        FillEdit::Fountain { from, .. } => *from,
        FillEdit::Pattern { kind: PatternKind::TwoColor { fore, .. }, .. } => *fore,
        FillEdit::Pattern { kind: PatternKind::Bitmap { average, .. }, .. } => *average,
        FillEdit::None => Rgba::BLACK,
    };
    let tile = match &was {
        FillEdit::Pattern { tile, .. } => *tile,
        _ => Size::new(Pattern::DEFAULT_TILE, Pattern::DEFAULT_TILE),
    };
    let two_color = matches!(was, FillEdit::Pattern { kind: PatternKind::TwoColor { .. }, .. });
    let bitmap = matches!(was, FillEdit::Pattern { kind: PatternKind::Bitmap { .. }, .. });
    let segments = [
        (ICON_FORMAT_COLOR_RESET, "No fill", matches!(was, FillEdit::None)),
        (ICON_FORMAT_COLOR_FILL, "Uniform fill", matches!(was, FillEdit::Solid(_))),
        (ICON_GRADIENT, "Fountain fill", matches!(was, FillEdit::Fountain { .. })),
        (ICON_PATTERN, "Two-color pattern fill", two_color),
        (ICON_WALLPAPER, "Bitmap pattern fill", bitmap),
    ];
    let names: Vec<String> = segments.iter().map(|(_, n, _)| format!("{n} {place}")).collect();
    let segs: Vec<_> = segments.iter().zip(&names).map(|((i, _, on), n)| (*i, n.as_str(), *on)).collect();
    if let Some(i) = crate::chrome::segmented(ui, &format!("fill_type{place}"), &segs) {
        edit = match i {
            0 => FillEdit::None,
            1 => FillEdit::Solid(solid_color),
            2 => match &was {
                f @ FillEdit::Fountain { .. } => f.clone(),
                _ => FillEdit::Fountain { kind: GradientKind::Linear, from: solid_color, to: Rgba::WHITE },
            },
            3 if two_color => was.clone(),
            3 => FillEdit::Pattern { kind: PatternKind::TwoColor { cells: TWO_COLOR_PATTERNS[0].1, fore: solid_color, back: Rgba::WHITE }, tile },
            _ if bitmap => was.clone(),
            _ => choose_bitmap_pattern(state).map_or_else(|| was.clone(), |kind| FillEdit::Pattern { kind, tile }),
        };
    }
    let unit = state.doc.display_unit;
    match &mut edit {
        FillEdit::Solid(c) => {
            color_button(ui, c, &format!("Fill color {place}"));
        }
        FillEdit::Fountain { kind, from, to } => {
            let kinds = [
                (ICON_LINEAR_SCALE, format!("Linear fountain fill {place}"), *kind == GradientKind::Linear),
                (ICON_RADIO_BUTTON_CHECKED, format!("Elliptical fountain fill {place}"), *kind == GradientKind::Radial),
            ];
            let segs: Vec<_> = kinds.iter().map(|(i, n, on)| (*i, n.as_str(), *on)).collect();
            if let Some(i) = crate::chrome::segmented(ui, &format!("fountain_kind{place}"), &segs) {
                *kind = if i == 0 { GradientKind::Linear } else { GradientKind::Radial };
            }
            color_button(ui, from, &format!("Fountain start color {place}"));
            color_button(ui, to, &format!("Fountain end color {place}"));
        }
        FillEdit::Pattern { kind, tile } => {
            match kind {
                PatternKind::TwoColor { cells, fore, back } => {
                    let name = format!("Pattern {place}");
                    let combo = egui::ComboBox::from_id_salt(&name).selected_text(pattern_name(*cells)).width(150.0).height(400.0).show_ui(ui, |ui| {
                        for (label, c) in TWO_COLOR_PATTERNS {
                            ui.horizontal(|ui| {
                                pattern_swatch(ui, *c, *fore, *back);
                                ui.selectable_value(cells, *c, *label);
                            });
                        }
                    });
                    crate::a11y::name(&combo.response, &name);
                    color_button(ui, fore, &format!("Pattern foreground color {place}"));
                    color_button(ui, back, &format!("Pattern background color {place}"));
                }
                PatternKind::Bitmap { .. } => {
                    let name = format!("Choose bitmap… {place}");
                    let resp = ui.button(format!("{} Bitmap…", <&str>::from(ICON_FOLDER_OPEN)));
                    crate::a11y::button(&resp, true, &name);
                    if resp.on_hover_text("Choose the bitmap to repeat").clicked()
                        && let Some(k) = choose_bitmap_pattern(state)
                    {
                        *kind = k;
                    }
                }
            }
            let mut size = [tile.width, tile.height].map(|v| unit.from_points(v));
            let mut changed = false;
            // In the docker, width and height go together on a row of their own.
            if ui.layout().main_wrap {
                ui.end_row();
            }
            ui.horizontal(|ui| {
                for ((label, prefix), v) in [("Tile width", "W "), ("Tile height", "H ")].into_iter().zip(&mut size) {
                    let field = egui::DragValue::new(v)
                        .range(unit.from_points(0.1)..=unit.from_points(5000.0))
                        .speed(unit.step() / 2.0)
                        .max_decimals(unit.decimals())
                        .prefix(prefix)
                        .suffix(unit.suffix());
                    let resp = ui.add_sized([84.0, 20.0], field);
                    crate::a11y::name(&resp, &format!("{label} {place}"));
                    changed |= resp.on_hover_text(label).changed();
                }
            });
            if changed {
                *tile = Size::new(unit.to_points(size[0]).max(0.1), unit.to_points(size[1]).max(0.1));
            }
        }
        FillEdit::None => {}
    }
    if edit != was {
        apply(state, &ids, &edit);
    }
}

/// Uniform transparency of the unlocked selected objects, 0–100 %.
pub fn transparency_controls(ui: &mut egui::Ui, state: &mut AppState, name: &str) {
    let ids = state.selection.clone();
    let Some(first) = state.selected_objects().find(|o| !o.locked) else { return };
    let mut percent = first.shown_style().transparency * 100.0;
    let field = egui::DragValue::new(&mut percent).range(0.0..=100.0).speed(1.0).suffix(" %").max_decimals(0);
    let resp = ui.add_sized([70.0, 20.0], field);
    crate::a11y::name(&resp, name);
    if resp.on_hover_text("Transparency").changed() {
        let t = (percent / 100.0).clamp(0.0, 1.0);
        state.doc.restyle_objects(&ids, |s| s.transparency = t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fountain_edits_keep_placement_and_change_colours() {
        let frame = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut s = Style::filled(Rgba::BLACK);
        apply_to(&mut s, frame, &FillEdit::Fountain { kind: GradientKind::Linear, from: Rgba::BLACK, to: Rgba::WHITE });
        let Some(Fill::Gradient(g)) = &s.fill else { panic!("fountain") };
        assert_eq!((g.start.x, g.end.x), (0.0, 10.0));

        // Move the end, then change a colour: the end stays.
        if let Some(Fill::Gradient(g)) = &mut s.fill {
            g.end.x = 5.0;
        }
        let red = Rgba::rgb(255, 0, 0);
        apply_to(&mut s, frame, &FillEdit::Fountain { kind: GradientKind::Linear, from: red, to: Rgba::WHITE });
        let Some(Fill::Gradient(g)) = &s.fill else { panic!("fountain") };
        assert_eq!((g.end.x, g.stops[0].color), (5.0, red));
        assert_eq!(current(s.fill.as_ref()), FillEdit::Fountain { kind: GradientKind::Linear, from: red, to: Rgba::WHITE });

        apply_to(&mut s, frame, &FillEdit::None);
        assert_eq!(s.fill, None);
    }
}

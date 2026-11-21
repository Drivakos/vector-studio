//! Fill and transparency controls (Properties docker, Interactive Fill and Transparency
//! tool property bars), and the fill edits they make.

use egui_material_icons::icons::*;
use vs_core::kurbo::Rect;
use vs_core::{Fill, Gradient, GradientKind, ObjectId, Rgba, Style};

use crate::app::AppState;

/// A change to the fill of several objects. Fountain fills are placed on each object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillEdit {
    None,
    Solid(Rgba),
    Fountain { kind: GradientKind, from: Rgba, to: Rgba },
}

/// Apply `edit` to one style whose object's local frame is `frame`.
pub fn apply_to(style: &mut Style, frame: Rect, edit: FillEdit) {
    style.fill = match edit {
        FillEdit::None => None,
        FillEdit::Solid(c) => Some(Fill::Solid(c)),
        FillEdit::Fountain { kind, from, to } => Some(Fill::Gradient(match style.fill.take() {
            // The same kind keeps where the fill sits; only the colours change.
            Some(Fill::Gradient(mut g)) if g.kind == kind => {
                if let Some(s) = g.stops.first_mut() {
                    s.color = from;
                }
                if let Some(s) = g.stops.last_mut() {
                    s.color = to;
                }
                g
            }
            _ => Gradient::across(kind, frame, from, to),
        })),
    };
}

pub fn apply(state: &mut AppState, ids: &[ObjectId], edit: FillEdit) {
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

/// No fill / Uniform / Fountain, then the colours. Edits the unlocked selected objects.
/// `place` ends every accessible name, e.g. "(docker)".
pub fn fill_controls(ui: &mut egui::Ui, state: &mut AppState, place: &str) {
    let ids = state.selection.clone();
    let Some(first) = state.selected_objects().find(|o| !o.locked) else { return };
    let was = current(first.shown_style().fill.as_ref());
    let mut edit = was;
    let solid_color = match was {
        FillEdit::Solid(c) => c,
        FillEdit::Fountain { from, .. } => from,
        FillEdit::None => Rgba::BLACK,
    };
    let segments = [
        (ICON_FORMAT_COLOR_RESET, "No fill", matches!(was, FillEdit::None)),
        (ICON_FORMAT_COLOR_FILL, "Uniform fill", matches!(was, FillEdit::Solid(_))),
        (ICON_GRADIENT, "Fountain fill", matches!(was, FillEdit::Fountain { .. })),
    ];
    let names: Vec<String> = segments.iter().map(|(_, n, _)| format!("{n} {place}")).collect();
    let segs: Vec<_> = segments.iter().zip(&names).map(|((i, _, on), n)| (*i, n.as_str(), *on)).collect();
    if let Some(i) = crate::chrome::segmented(ui, &format!("fill_type{place}"), &segs) {
        edit = match i {
            0 => FillEdit::None,
            1 => FillEdit::Solid(solid_color),
            _ => match was {
                f @ FillEdit::Fountain { .. } => f,
                _ => FillEdit::Fountain { kind: GradientKind::Linear, from: solid_color, to: Rgba::WHITE },
            },
        };
    }
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
        FillEdit::None => {}
    }
    if edit != was {
        apply(state, &ids, edit);
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
        apply_to(&mut s, frame, FillEdit::Fountain { kind: GradientKind::Linear, from: Rgba::BLACK, to: Rgba::WHITE });
        let Some(Fill::Gradient(g)) = &s.fill else { panic!("fountain") };
        assert_eq!((g.start.x, g.end.x), (0.0, 10.0));

        // Move the end, then change a colour: the end stays.
        if let Some(Fill::Gradient(g)) = &mut s.fill {
            g.end.x = 5.0;
        }
        let red = Rgba::rgb(255, 0, 0);
        apply_to(&mut s, frame, FillEdit::Fountain { kind: GradientKind::Linear, from: red, to: Rgba::WHITE });
        let Some(Fill::Gradient(g)) = &s.fill else { panic!("fountain") };
        assert_eq!((g.end.x, g.stops[0].color), (5.0, red));
        assert_eq!(current(s.fill.as_ref()), FillEdit::Fountain { kind: GradientKind::Linear, from: red, to: Rgba::WHITE });

        apply_to(&mut s, frame, FillEdit::None);
        assert_eq!(s.fill, None);
    }
}

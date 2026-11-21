//! Outline controls shared by the property bar and the Properties docker: width with
//! presets, line style with previews, corners, line caps and the two pen options.

use egui::{Color32, Sense, Stroke, pos2, vec2};
use egui_material_icons::icons::*;
use vs_core::{LineCap, LineJoin, LineStyle, Outline, Rgba, Style};

use crate::theme;

/// Widths offered in the preset list, in points. 0 is a hairline.
pub const WIDTH_PRESETS: &[f64] = &[0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0];

/// Width given to an outline that is switched on without one.
pub const DEFAULT_WIDTH: f64 = 0.5;

pub fn width_label(width: Option<f64>) -> String {
    match width {
        None => "None".into(),
        Some(w) if w <= 0.0 => "Hairline".into(),
        Some(w) => format!("{} pt", trim(w)),
    }
}

fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Number field for the outline width (drag below 0 for no outline, type "none" or
/// "hairline") with a ▾ button listing the presets. Returns the new width when it changed;
/// `Some(None)` removes the outline.
pub fn width_field(ui: &mut egui::Ui, id: &str, current: Option<f64>, field_width: f32) -> Option<Option<f64>> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // -1 stands for "no outline" inside the field.
        let mut v = current.unwrap_or(-1.0);
        let field = egui::DragValue::new(&mut v)
            .range(-1.0..=500.0)
            .speed(0.05)
            .custom_formatter(|v, _| width_label((v >= 0.0).then_some(v)))
            .custom_parser(|text| {
                let t = text.trim().trim_end_matches("pt").trim().to_lowercase();
                match t.as_str() {
                    "none" | "" => Some(-1.0),
                    "hairline" => Some(0.0),
                    _ => t.replace(',', ".").parse().ok(),
                }
            });
        let resp = ui.add_sized([field_width, 20.0], field);
        crate::a11y::name(&resp, &format!("Outline width ({id})"));
        if resp.on_hover_text("Outline width").changed() {
            // Dragging from a hairline downwards passes through small negatives first.
            picked = Some((v >= 0.0).then_some(v));
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(15.0, 20.0), Sense::click());
        crate::a11y::button(&resp, true, &format!("Outline width presets ({id})"));
        ui.painter().rect_filled(rect, 4.0, theme::FIELD);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            <&str>::from(ICON_EXPAND_MORE),
            egui::FontId::new(13.0, ICON_EXPAND_MORE.font_family()),
            theme::TEXT_DIM,
        );
        let resp = resp.on_hover_text("Outline width presets");
        egui::Popup::menu(&resp).id(ui.make_persistent_id((id, "presets"))).show(|ui| {
            ui.set_min_width(110.0);
            if ui.selectable_label(current.is_none(), "None").clicked() {
                picked = Some(None);
            }
            for &w in WIDTH_PRESETS {
                let on = current.is_some_and(|c| (c - w).abs() < 1e-9);
                if ui.selectable_label(on, width_label(Some(w))).clicked() {
                    picked = Some(Some(w));
                }
            }
        });
    });
    picked
}

/// Paints `style` as a horizontal line across `rect`.
fn paint_line_style(painter: &egui::Painter, rect: egui::Rect, style: LineStyle, color: Color32) {
    let width = 2.0;
    let y = rect.center().y;
    let pattern = style.pattern();
    if pattern.is_empty() {
        painter.hline(rect.x_range(), y, Stroke::new(width, color));
        return;
    }
    let mut x = rect.min.x;
    let mut i = 0;
    while x < rect.max.x {
        let len = pattern[i % pattern.len()] as f32 * width;
        if i % 2 == 0 {
            let end = (x + len).min(rect.max.x);
            painter.rect_filled(egui::Rect::from_min_max(pos2(x, y - width / 2.0), pos2(end, y + width / 2.0)), 0.0, color);
        }
        x += len;
        i += 1;
    }
}

/// A button showing the current line style; it opens a list of previews.
pub fn line_style_picker(ui: &mut egui::Ui, id: &str, current: LineStyle, width: f32) -> Option<LineStyle> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    crate::a11y::button(&resp, true, &format!("Line style ({id})"));
    ui.painter().rect_filled(rect, 5.0, theme::FIELD);
    let chevron = 14.0;
    let line = egui::Rect::from_min_max(rect.min + vec2(8.0, 0.0), pos2(rect.max.x - chevron - 6.0, rect.max.y));
    paint_line_style(ui.painter(), line, current, theme::TEXT);
    ui.painter().text(
        pos2(rect.max.x - chevron / 2.0 - 3.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        <&str>::from(ICON_EXPAND_MORE),
        egui::FontId::new(14.0, ICON_EXPAND_MORE.font_family()),
        theme::TEXT_DIM,
    );
    let resp = resp.on_hover_text(format!("Line style: {}", current.label()));
    let mut picked = None;
    egui::Popup::menu(&resp).id(ui.make_persistent_id((id, "styles"))).show(|ui| {
        for style in LineStyle::ALL {
            let (r, row) = ui.allocate_exact_size(vec2(150.0, 22.0), Sense::click());
            crate::a11y::toggle(&row, true, style == current, style.label());
            if style == current || row.hovered() {
                ui.painter().rect_filled(r, 4.0, if row.hovered() { theme::ACCENT } else { theme::SUNKEN });
            }
            paint_line_style(ui.painter(), r.shrink2(vec2(10.0, 0.0)), style, theme::TEXT);
            if row.on_hover_text(style.label()).clicked() {
                picked = Some(style);
                ui.close();
            }
        }
    });
    picked
}

/// A row of choices; each one's accessible name is "`group` `label`", e.g. "Corners Round".
fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, group: &str, current: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (value, label) in options {
            let resp = ui.selectable_label(*current == *value, *label);
            crate::a11y::name(&resp, &format!("{group} {label}"));
            if resp.clicked() && *current != *value {
                *current = *value;
                changed = true;
            }
        }
    });
    changed
}

fn to_color32(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// The Outline section of the Properties docker. Edits `style` in place.
pub fn outline_editor(ui: &mut egui::Ui, style: &mut Style) {
    egui::Grid::new("outline_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Width");
        if let Some(w) = width_field(ui, "Properties docker", style.outline.as_ref().map(|o| o.width), 110.0) {
            set_width(style, w);
        }
        ui.end_row();

        let Some(o) = &mut style.outline else { return };
        ui.label("Color");
        let mut c = to_color32(o.color);
        if ui.color_edit_button_srgba_unmultiplied(&mut c).changed() {
            o.color = Rgba::new(c[0], c[1], c[2], c[3]);
        }
        ui.end_row();

        ui.label("Style");
        if let Some(s) = line_style_picker(ui, "Properties docker", o.style, 127.0) {
            o.style = s;
        }
        ui.end_row();

        ui.label("Corners");
        choice(ui, "Corners", &mut o.join, &[(LineJoin::Miter, "Miter"), (LineJoin::Round, "Round"), (LineJoin::Bevel, "Bevel")]);
        ui.end_row();

        ui.label("Caps").on_hover_text("Line caps: how open ends are drawn");
        choice(ui, "Caps", &mut o.cap, &[(LineCap::Butt, "Butt"), (LineCap::Round, "Round"), (LineCap::Square, "Square")]);
        ui.end_row();

        ui.label("");
        ui.checkbox(&mut o.behind_fill, "Behind fill").on_hover_text("Draw the outline under the fill");
        ui.end_row();

        ui.label("");
        ui.checkbox(&mut o.scale_with_object, "Scale with object").on_hover_text("Make the outline thicker or thinner when the object is resized");
        ui.end_row();
    });
}

/// Set the outline width; `None` removes the outline, a width on an object without one
/// adds a black outline.
pub fn set_width(style: &mut Style, width: Option<f64>) {
    match (width, &mut style.outline) {
        (None, _) => style.outline = None,
        (Some(w), Some(o)) => o.width = w,
        (Some(w), None) => style.outline = Some(Outline::new(Rgba::BLACK, w)),
    }
}

/// An edit that turns `before` into `after`, touching only the fields that differ, so it
/// can be applied to several objects without copying the first one's other settings.
pub fn style_change(before: &Style, after: &Style) -> Option<impl Fn(&mut Style) + use<>> {
    if before == after {
        return None;
    }
    let (before, after) = (before.clone(), after.clone());
    Some(move |s: &mut Style| {
        if before.fill != after.fill {
            s.fill.clone_from(&after.fill);
        }
        if before.transparency != after.transparency {
            s.transparency = after.transparency;
        }
        match (&before.outline, &after.outline, &mut s.outline) {
            (Some(b), Some(a), Some(o)) => {
                if b.color != a.color {
                    o.color = a.color;
                }
                if b.width != a.width {
                    o.width = a.width;
                }
                if b.style != a.style {
                    o.style = a.style;
                }
                if b.join != a.join {
                    o.join = a.join;
                }
                if b.cap != a.cap {
                    o.cap = a.cap;
                }
                if b.behind_fill != a.behind_fill {
                    o.behind_fill = a.behind_fill;
                }
                if b.scale_with_object != a.scale_with_object {
                    o.scale_with_object = a.scale_with_object;
                }
            }
            (b, a, _) if b != a => s.outline = a.clone(),
            _ => {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_change_only_touches_what_changed() {
        let before = Style { outline: Some(Outline::new(Rgba::BLACK, 1.0)), ..Style::filled(Rgba::WHITE) };
        let mut after = before.clone();
        after.outline.as_mut().unwrap().style = LineStyle::Dash;
        let change = style_change(&before, &after).expect("differs");

        let mut other = Style { outline: Some(Outline::new(Rgba::rgb(9, 9, 9), 4.0)), ..Style::filled(Rgba::rgb(1, 2, 3)) };
        change(&mut other);
        assert_eq!(other.fill, Some(Rgba::rgb(1, 2, 3).into()));
        let o = other.outline.unwrap();
        assert_eq!((o.width, o.color, o.style), (4.0, Rgba::rgb(9, 9, 9), LineStyle::Dash));

        assert!(style_change(&before, &before).is_none());
    }

    #[test]
    fn width_none_removes_and_width_on_none_adds_black() {
        let mut s = Style::default();
        set_width(&mut s, Some(2.0));
        assert_eq!(s.outline, Some(Outline::new(Rgba::BLACK, 2.0)));
        set_width(&mut s, None);
        assert_eq!(s.outline, None);
        assert_eq!(width_label(Some(0.0)), "Hairline");
        assert_eq!(width_label(Some(0.75)), "0.75 pt");
    }
}

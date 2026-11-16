//! The Text tool (F8): click the page to start artistic text, drag a frame for paragraph
//! text, click text to edit it, type. Font, style, size and alignment come from the
//! property bar.

use egui::{Color32, Event, Key, Pos2, Stroke};
use egui_material_icons::icons::*;
use vs_core::kurbo::{Affine, Point, Rect};
use vs_core::{ObjectId, Shape, Style, Text, TextAlign};

use crate::app::AppState;
use crate::chrome::{number_row, segmented};
use crate::theme;

/// The text being typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub id: ObjectId,
    /// Caret position in characters, counting line breaks.
    pub caret: usize,
}

fn text_of(state: &AppState, id: ObjectId) -> Option<(&Text, Affine)> {
    match state.doc.active_page().object(id) {
        Some(o) => match &o.shape {
            Shape::Text(t) => Some((t, o.transform)),
            _ => None,
        },
        None => None,
    }
}

fn text_mut(state: &mut AppState, id: ObjectId) -> Option<&mut Text> {
    match state.doc.active_page_mut().object_mut(id).map(|o| &mut o.shape) {
        Some(Shape::Text(t)) => Some(t),
        _ => None,
    }
}

/// The unlocked, selectable text under `p` (page space), topmost first.
pub fn text_at(state: &AppState, p: Point, tolerance: f64) -> Option<(ObjectId, Affine)> {
    let page = state.doc.active_page();
    page.objects
        .iter()
        .rev()
        .find(|o| !o.locked && page.is_selectable(o) && matches!(o.shape, Shape::Text(_)) && o.hit(p, tolerance))
        .map(|o| (o.id, o.transform))
}

/// Click with the Text tool at `p` (page space): edit the text there, or start new text
/// whose first baseline begins at `p`.
pub fn press(state: &mut AppState, p: Point, tolerance: f64) {
    finish(state);
    let hit = text_at(state, p, tolerance);
    let edit = match hit {
        Some((id, transform)) => {
            let caret = text_of(state, id).map_or(0, |(t, _)| t.layout().index_at(transform.inverse() * p));
            TextEdit { id, caret }
        }
        None => {
            let mut text = state.tools.settings.text.clone();
            text.content.clear();
            let id = state.doc.add_object(Shape::Text(text), Style::new_text());
            if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
                obj.transform = Affine::translate(p.to_vec2());
            }
            TextEdit { id, caret: 0 }
        }
    };
    state.selection = vec![edit.id];
    state.marquee_selection = None;
    state.tools.text = Some(edit);
}

/// Start paragraph text in `frame` (page space) and type into it.
pub fn start_paragraph(state: &mut AppState, frame: Rect) {
    finish(state);
    let mut text = state.tools.settings.text.clone();
    text.content.clear();
    text.frame = Some(frame.size());
    let id = state.doc.add_object(Shape::Text(text), Style::new_text());
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.transform = Affine::translate(frame.origin().to_vec2());
    }
    state.selection = vec![id];
    state.marquee_selection = None;
    state.tools.text = Some(TextEdit { id, caret: 0 });
}

/// Stop typing. Artistic text left empty is removed; an empty paragraph frame stays.
pub fn finish(state: &mut AppState) {
    let Some(edit) = state.tools.text.take() else { return };
    if text_of(state, edit.id).is_some_and(|(t, _)| t.content.is_empty() && !t.is_paragraph()) {
        state.doc.delete_objects(&[edit.id]);
        state.selection.retain(|id| *id != edit.id);
    }
}

/// Typing while text is being edited. Takes the text and plain keys out of this frame's
/// input so no shortcut sees them; Cmd shortcuts still work. Returns whether text is
/// being edited.
pub fn keyboard(state: &mut AppState, ctx: &egui::Context) -> bool {
    let Some(edit) = state.tools.text else { return false };
    if text_of(state, edit.id).is_none() || !state.selection.contains(&edit.id) {
        finish(state);
        return false;
    }
    let events = ctx.input_mut(|i| {
        let (mine, rest): (Vec<Event>, Vec<Event>) = std::mem::take(&mut i.events).into_iter().partition(|e| match e {
            Event::Text(_) => true,
            Event::Paste(_) => true,
            Event::Key { modifiers, .. } => !modifiers.command && !modifiers.ctrl,
            _ => false,
        });
        i.events = rest;
        mine
    });
    for event in events {
        match event {
            Event::Text(s) | Event::Paste(s) => insert(state, &s),
            Event::Key { key, pressed: true, .. } => match key {
                Key::Enter => insert(state, "\n"),
                Key::Backspace => delete(state, true),
                Key::Delete => delete(state, false),
                Key::ArrowLeft => move_caret(state, |c, _, _| c.saturating_sub(1)),
                Key::ArrowRight => move_caret(state, |c, len, _| (c + 1).min(len)),
                Key::Home => move_caret(state, |c, _, s| line_start(s, c)),
                Key::End => move_caret(state, |c, _, s| line_end(s, c)),
                Key::Escape => {
                    finish(state);
                    return false;
                }
                _ => {}
            },
            _ => {}
        }
    }
    true
}

fn line_start(content: &str, caret: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    (0..caret.min(chars.len())).rev().find(|i| chars[*i] == '\n').map_or(0, |i| i + 1)
}

fn line_end(content: &str, caret: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    (caret..chars.len()).find(|i| chars[*i] == '\n').unwrap_or(chars.len())
}

fn byte_at(content: &str, caret: usize) -> usize {
    content.char_indices().nth(caret).map_or(content.len(), |(b, _)| b)
}

fn insert(state: &mut AppState, s: &str) {
    let Some(edit) = state.tools.text else { return };
    let s: String = s.chars().filter(|c| *c == '\n' || !c.is_control()).collect();
    let Some(text) = text_mut(state, edit.id) else { return };
    let at = byte_at(&text.content, edit.caret);
    text.content.insert_str(at, &s);
    state.tools.text = Some(TextEdit { caret: edit.caret + s.chars().count(), ..edit });
}

fn delete(state: &mut AppState, backwards: bool) {
    let Some(edit) = state.tools.text else { return };
    let Some(text) = text_mut(state, edit.id) else { return };
    let len = text.content.chars().count();
    let at = if backwards {
        if edit.caret == 0 {
            return;
        }
        edit.caret - 1
    } else {
        if edit.caret >= len {
            return;
        }
        edit.caret
    };
    let b = byte_at(&text.content, at);
    text.content.remove(b);
    state.tools.text = Some(TextEdit { caret: at, ..edit });
}

fn move_caret(state: &mut AppState, f: impl FnOnce(usize, usize, &str) -> usize) {
    let Some(edit) = state.tools.text else { return };
    let Some((text, _)) = text_of(state, edit.id) else { return };
    let content = text.content.clone();
    let caret = f(edit.caret, content.chars().count(), &content);
    state.tools.text = Some(TextEdit { caret, ..edit });
}

/// The caret, drawn over the canvas while typing. `to_screen` maps page to window points.
pub fn draw_caret(painter: &egui::Painter, state: &AppState, to_screen: impl Fn(Point) -> Pos2) {
    let Some(edit) = state.tools.text else { return };
    let Some((text, transform)) = text_of(state, edit.id) else { return };
    let layout = text.layout();
    let (top, bottom) = layout.caret(edit.caret);
    let stroke = Stroke::new(1.5, Color32::BLACK);
    painter.line_segment([to_screen(transform * top), to_screen(transform * bottom)], stroke);
    // A faint frame shows where the text sits; red when paragraph text overflows it.
    let b = layout.bounds();
    let corners = [b.origin(), Point::new(b.x1, b.y0), Point::new(b.x1, b.y1), Point::new(b.x0, b.y1)].map(|c| to_screen(transform * c));
    let color = if layout.overflow { OVERFLOW } else { Color32::from_rgba_unmultiplied(0x1E, 0x6F, 0xD9, 0x80) };
    painter.add(egui::Shape::closed_line(corners.to_vec(), Stroke::new(1.0, color)));
}

/// Frame colour of paragraph text that does not fit.
pub const OVERFLOW: Color32 = Color32::from_rgb(0xD0, 0x20, 0x20);

/// Paragraph text frames of the selection (not the one being typed into, which
/// [`draw_caret`] frames), so a frame shows even while the text is short.
pub fn draw_frames(painter: &egui::Painter, state: &AppState, to_screen: impl Fn(Point) -> Pos2) {
    let typing = state.tools.text.map(|e| e.id);
    for obj in state.selected_objects().filter(|o| Some(o.id) != typing) {
        let Shape::Text(text) = &obj.shape else { continue };
        if !text.is_paragraph() {
            continue;
        }
        let layout = text.layout();
        let b = layout.bounds();
        let corners = [b.origin(), Point::new(b.x1, b.y0), Point::new(b.x1, b.y1), Point::new(b.x0, b.y1)].map(|c| to_screen(obj.transform * c));
        let color = if layout.overflow { OVERFLOW } else { Color32::from_gray(140) };
        painter.add(egui::Shape::dashed_line(&[corners.to_vec(), vec![corners[0]]].concat(), Stroke::new(1.0, color), 4.0, 3.0));
    }
}

/// The texts the font controls change: the one being typed into, else the selected
/// unlocked texts.
fn targets(state: &AppState) -> Vec<ObjectId> {
    if let Some(edit) = state.tools.text {
        return vec![edit.id];
    }
    state.selected_objects().filter(|o| !o.locked && matches!(o.shape, Shape::Text(_))).map(|o| o.id).collect()
}

/// Whether the selection is all unlocked text, so the Pick tool shows the font controls.
pub fn selection_is_text(state: &AppState) -> bool {
    state.has_selection() && state.selected_objects().all(|o| !o.locked && matches!(o.shape, Shape::Text(_)))
}

/// Font list, style, size and alignment. They change the texts being edited or selected,
/// or else the settings for new text.
pub fn controls(ui: &mut egui::Ui, state: &mut AppState) {
    let ids = targets(state);
    let mut spec = ids
        .first()
        .and_then(|id| text_of(state, *id))
        .map_or_else(|| state.tools.settings.text.clone(), |(t, _)| t.clone());
    let before = spec.clone();

    let family = egui::ComboBox::from_id_salt("font_family")
        .selected_text(&spec.family)
        .width(170.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            for f in vs_core::text::families() {
                ui.selectable_value(&mut spec.family, f.clone(), f);
            }
        });
    crate::a11y::name(&family.response, "Font list");
    family.response.on_hover_text("Font list");
    let styles = vs_core::text::styles(&spec.family);
    if spec.family != before.family && !styles.contains(&spec.style) {
        spec.style = styles.first().cloned().unwrap_or_else(|| "Regular".into());
    }
    let style = egui::ComboBox::from_id_salt("font_style")
        .selected_text(&spec.style)
        .width(100.0)
        .show_ui(ui, |ui| {
            for s in styles {
                ui.selectable_value(&mut spec.style, s.clone(), s);
            }
        });
    crate::a11y::name(&style.response, "Font style");
    style.response.on_hover_text("Font style");
    number_row(ui, Some(ICON_FORMAT_SIZE), "Font size", &mut spec.size, " pt", 1.0..=3000.0, 1.0, 1);
    theme::vsep(ui);
    let icons = [ICON_FORMAT_ALIGN_LEFT, ICON_FORMAT_ALIGN_CENTER, ICON_FORMAT_ALIGN_RIGHT];
    let segments: Vec<_> = TextAlign::ALL
        .iter()
        .zip(icons)
        .map(|(a, icon)| (icon, a.label(), spec.align == *a))
        .collect();
    let names: Vec<String> = segments.iter().map(|(_, l, _)| format!("Align text {}", l.to_lowercase())).collect();
    let segments: Vec<_> = segments.iter().zip(&names).map(|((i, _, on), n)| (*i, n.as_str(), *on)).collect();
    if let Some(i) = segmented(ui, "text_align", &segments) {
        spec.align = TextAlign::ALL[i];
    }

    if spec == before {
        return;
    }
    if ids.is_empty() {
        state.tools.settings.text = spec;
        return;
    }
    for id in ids {
        if let Some(t) = text_mut(state, id) {
            t.family.clone_from(&spec.family);
            t.style.clone_from(&spec.style);
            t.size = spec.size;
            t.align = spec.align;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_and_end_stop_at_breaks() {
        let s = "ab\ncde\nf";
        assert_eq!(line_start(s, 5), 3);
        assert_eq!(line_end(s, 5), 6);
        assert_eq!(line_start(s, 1), 0);
        assert_eq!(line_end(s, 7), 8);
    }

    #[test]
    fn caret_positions_are_characters_not_bytes() {
        assert_eq!(byte_at("é1", 1), 2);
        assert_eq!(byte_at("é1", 5), 3);
    }
}

//! Edit > Paste Special: choose how the clipboard comes in, as this app's objects, as an
//! SVG drawing turned into curves, or as plain text. The system clipboard is read
//! through [`ClipboardText`] so the end-to-end tests can stand in for it.

use vs_core::kurbo::Affine;
use vs_core::{Shape, Style, TextAlign};

use crate::app::AppState;

/// Text on the system clipboard.
pub trait ClipboardText {
    fn text(&mut self) -> Option<String>;
}

/// The Mac's clipboard.
pub struct SystemClipboard;

impl ClipboardText for SystemClipboard {
    fn text(&mut self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }
}

/// A way to paste what is on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteAs {
    /// The objects of this app's last copy or cut, as they were.
    Objects,
    /// SVG markup, imported as curves.
    Svg,
    /// Any text, as artistic text.
    Text,
}

impl PasteAs {
    pub fn label(self) -> &'static str {
        match self {
            PasteAs::Objects => "Vector Studio objects",
            PasteAs::Svg => "SVG drawing (as curves)",
            PasteAs::Text => "Plain text (as artistic text)",
        }
    }
}

/// The open Paste Special window: the clipboard text when it opened, the ways it can be
/// pasted, and the one chosen.
#[derive(Clone, Debug)]
pub struct PasteSpecialDialog {
    text: Option<String>,
    choices: Vec<PasteAs>,
    chosen: PasteAs,
}

/// Whether `text` looks like SVG markup.
fn is_svg(text: &str) -> bool {
    let t = text.trim_start();
    (t.starts_with("<svg") || t.starts_with("<?xml")) && t.contains("<svg")
}

/// Read the clipboard and open the window with the ways it can be pasted.
pub fn open(state: &mut AppState) {
    let text = state.clipboard_source.text().filter(|t| !t.is_empty());
    let mut choices = Vec::new();
    let ours = !state.internal_clipboard().is_empty() && text.as_deref().is_none_or(|t| t == state.system_clipboard_text());
    if ours {
        choices.push(PasteAs::Objects);
    }
    if let Some(t) = &text {
        if is_svg(t) {
            choices.push(PasteAs::Svg);
        }
        choices.push(PasteAs::Text);
    }
    let chosen = choices.first().copied().unwrap_or(PasteAs::Text);
    state.paste_special = Some(PasteSpecialDialog { text, choices, chosen });
}

fn paste(state: &mut AppState, how: PasteAs, text: Option<String>) {
    match how {
        PasteAs::Objects => {
            let objects = state.internal_clipboard().to_vec();
            state.selection = state.doc.paste_objects(&objects, vs_core::kurbo::Vec2::ZERO);
            state.marquee_selection = None;
        }
        PasteAs::Svg => {
            let objects = text.and_then(|t| vs_io::import_svg(t.as_bytes()).ok()).unwrap_or_default();
            if !crate::files::place_on_page(state, objects) {
                state.dialogs.error("Could not paste", "The clipboard's SVG draws nothing that can be pasted.");
            }
        }
        PasteAs::Text => {
            let Some(words) = text else { return };
            let mut t = state.tools.settings.text.clone();
            t.content = words.replace("\r\n", "\n").replace('\r', "\n");
            t.align = TextAlign::Center;
            t.frame = None;
            t.on_path = None;
            t.field = None;
            let id = state.doc.add_object(Shape::Text(t), Style::new_text());
            let page = state.doc.active_page().bounds();
            if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
                obj.transform = Affine::translate(page.center().to_vec2());
            }
            state.selection = vec![id];
            state.marquee_selection = None;
        }
    }
}

/// The Paste Special window: the ways the clipboard can be pasted, then OK.
pub fn paste_special_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.paste_special.take() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Paste Special")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            if dialog.choices.is_empty() {
                ui.label("The clipboard is empty.");
            } else {
                ui.label("Paste the clipboard as:");
                for how in &dialog.choices {
                    ui.radio_value(&mut dialog.chosen, *how, how.label());
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, !dialog.choices.is_empty(), "OK", "Paste Special").clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", "Paste Special").clicked();
            });
        });
    if ok {
        crate::text_tool::finish(state);
        paste(state, dialog.chosen, dialog.text);
    } else if open && !cancel {
        state.paste_special = Some(dialog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_is_told_from_other_text() {
        assert!(is_svg(r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>"#));
        assert!(is_svg("  <svg></svg>"));
        assert!(!is_svg("<?xml version=\"1.0\"?><html/>"));
        assert!(!is_svg("svg is a format"));
    }
}

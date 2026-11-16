//! The Find and Replace docker (Edit > Find and Replace, Cmd+F): find text objects one
//! after another across pages, and replace words in one or all of them.

use vs_core::TextMatch;

use crate::app::AppState;

/// What the Find and Replace docker holds between frames.
#[derive(Clone, Debug, Default)]
pub struct FindState {
    pub query: String,
    pub replace: String,
    pub match_case: bool,
    /// The outcome of the last button, shown under them.
    pub message: Option<String>,
}

/// The selected object, if it is a single one: where finding goes on from.
fn current(state: &AppState) -> Option<TextMatch> {
    match state.selection.as_slice() {
        [id] => Some(TextMatch { page: state.doc.active_page_index(), object: *id }),
        _ => None,
    }
}

/// Select the next text containing the query, after the selected one, going on to the
/// following pages and round to the first.
pub fn find_next(state: &mut AppState) {
    crate::text_tool::finish(state);
    let f = &state.find;
    let matches = state.doc.find_text(&f.query, f.match_case);
    if matches.is_empty() {
        state.find.message = Some("Not found".into());
        return;
    }
    let page = state.doc.active_page_index();
    let next = match current(state).and_then(|c| matches.iter().position(|m| *m == c)) {
        Some(i) => (i + 1) % matches.len(),
        // Nothing found selected: the first match on this page or after it.
        None => matches.iter().position(|m| m.page >= page).unwrap_or(0),
    };
    let m = matches[next];
    let _ = state.doc.set_active_page(m.page);
    state.selection = vec![m.object];
    state.marquee_selection = None;
    state.find.message = Some(format!("{} of {}", next + 1, matches.len()));
}

/// Replace in the selected match, then find the next one. With no match selected, find
/// one first.
pub fn replace(state: &mut AppState) {
    crate::text_tool::finish(state);
    let f = state.find.clone();
    let selected = current(state).filter(|c| state.doc.find_text(&f.query, f.match_case).contains(c));
    match selected {
        Some(m) => {
            state.doc.replace_text(&f.query, &f.replace, f.match_case, Some(m));
            find_next(state);
        }
        None => find_next(state),
    }
}

pub fn replace_all(state: &mut AppState) {
    crate::text_tool::finish(state);
    let f = state.find.clone();
    let n = state.doc.replace_text(&f.query, &f.replace, f.match_case, None);
    state.find.message = Some(match n {
        0 => "Not found".into(),
        1 => "Replaced 1 time".into(),
        n => format!("Replaced {n} times"),
    });
}

pub fn find_replace_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let f = &mut state.find;
    ui.label("Find:");
    let find = crate::a11y::add(ui, egui::TextEdit::singleline(&mut f.query).desired_width(f32::INFINITY), "Find (find and replace)");
    ui.label("Replace with:");
    let with = crate::a11y::add(ui, egui::TextEdit::singleline(&mut f.replace).desired_width(f32::INFINITY), "Replace with (find and replace)");
    if find.changed() || with.changed() {
        f.message = None;
    }
    ui.checkbox(&mut f.match_case, "Match case");
    let enter = find.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let ready = !f.query.is_empty();
    ui.add_space(6.0);
    let (mut next, mut one, mut all) = (enter && ready, false, false);
    ui.horizontal_wrapped(|ui| {
        next |= ui.add_enabled(ready, egui::Button::new("Find next")).clicked();
        one = ui.add_enabled(ready, egui::Button::new("Replace")).clicked();
        all = ui.add_enabled(ready, egui::Button::new("Replace all")).clicked();
    });
    if let Some(message) = &f.message {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(message).weak());
    }
    if next {
        find_next(state);
    } else if one {
        replace(state);
    } else if all {
        replace_all(state);
    }
}

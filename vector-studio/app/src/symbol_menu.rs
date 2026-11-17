//! Object > Symmetry and Object > Symbol: editing a symmetry group or a symbol instance
//! (its members out on the page until Finish), and the Symbol Manager window.

use std::borrow::Cow;

use vs_core::{ObjectId, Page, Shape, SymbolId};

use crate::app::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupEditKind {
    Symmetry,
    Symbol,
}

/// Edit Symmetry or Edit Symbol in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupEdit {
    pub kind: GroupEditKind,
    /// The symmetry group or instance, emptied while its members are out.
    pub group: ObjectId,
    /// The members taken out onto the page.
    pub members: Vec<ObjectId>,
    /// Objects made while editing (this id or higher) go in too.
    pub first_new: ObjectId,
    /// The symbol being edited. The instance is unlinked meanwhile, so the others keep
    /// their look until Finish.
    pub symbol: Option<SymbolId>,
}

impl GroupEdit {
    /// What goes back in on Finish: the members and anything new, still on the page.
    fn returning(&self, page: &Page) -> Vec<ObjectId> {
        page.objects
            .iter()
            .map(|o| o.id)
            .filter(|&id| id != self.group && (self.members.contains(&id) || id.0 >= self.first_new.0))
            .collect()
    }

    pub fn finish_label(&self) -> &'static str {
        match self.kind {
            GroupEditKind::Symmetry => "Finish editing symmetry",
            GroupEditKind::Symbol => "Finish editing symbol",
        }
    }
}

/// Whether an edit mode (PowerClip, symmetry or symbol) is on.
fn editing(state: &AppState) -> bool {
    state.group_edit.is_some() || state.powerclip.editing.is_some()
}

pub fn can_create_symmetry(state: &AppState) -> bool {
    !editing(state) && state.doc.can_create_symmetry(&state.selection)
}

/// Create New Symmetry: the selection becomes a symmetry group, opened for editing.
pub fn create_symmetry(state: &mut AppState) {
    if !can_create_symmetry(state) {
        return;
    }
    if let Some(group) = state.doc.create_symmetry(&state.selection) {
        state.selection = vec![group];
        start(state, GroupEditKind::Symmetry);
    }
}

pub fn can_edit_symmetry(state: &AppState) -> bool {
    !editing(state) && state.selection.len() == 1 && !state.doc.symmetry_groups(&state.selection).is_empty()
}

pub fn edit_symmetry(state: &mut AppState) {
    if can_edit_symmetry(state) {
        start(state, GroupEditKind::Symmetry);
    }
}

pub fn can_make_symbol(state: &AppState) -> bool {
    !editing(state) && state.doc.can_make_symbol(&state.selection)
}

pub fn can_edit_symbol(state: &AppState) -> bool {
    !editing(state) && state.selection.len() == 1 && !state.doc.instances(&state.selection).is_empty()
}

pub fn edit_symbol(state: &mut AppState) {
    if can_edit_symbol(state) {
        start(state, GroupEditKind::Symbol);
    }
}

/// Take the selected group's members out onto the page to edit them.
fn start(state: &mut AppState, kind: GroupEditKind) {
    let group = state.selection[0];
    let first_new = state.doc.next_object_id();
    let symbol = state.doc.active_page_mut().object_mut(group).and_then(|o| o.symbol.take());
    let members = state.doc.lift_members(group);
    state.selection.clone_from(&members);
    state.marquee_selection = None;
    state.group_edit = Some(GroupEdit { kind, group, members, first_new, symbol });
}

/// Put the members (and anything drawn meanwhile) back in the group being edited.
pub fn finish(state: &mut AppState) {
    let Some(edit) = state.group_edit.take() else { return };
    if state.doc.active_page().object(edit.group).is_none() {
        return;
    }
    let back = edit.returning(state.doc.active_page());
    state.doc.sink_members(edit.group, &back);
    if let Some(obj) = state.doc.active_page_mut().object_mut(edit.group) {
        obj.symbol = edit.symbol;
    }
    // The edited members become the symbol's, for every instance.
    state.doc.sync_symbols();
    state.selection = vec![edit.group];
    state.marquee_selection = None;
}

/// The symmetry group being edited, for the property bar.
pub fn edited_symmetry(state: &mut AppState) -> Option<&mut vs_core::Symmetry> {
    let edit = state.group_edit.as_ref().filter(|e| e.kind == GroupEditKind::Symmetry)?;
    let group = edit.group;
    state.doc.active_page_mut().object_mut(group)?.symmetry.as_mut()
}

/// The active page as the canvas shows it: while a symmetry is edited, its members (out
/// on the page) are drawn inside it, so their mirror follows every change.
pub fn shown_page(state: &AppState) -> Cow<'_, Page> {
    let page = state.doc.active_page();
    match &state.group_edit {
        Some(edit) if edit.kind == GroupEditKind::Symmetry && page.object(edit.group).is_some() => {
            let mut shown = page.clone();
            let back = edit.returning(page);
            shown.sink_members(edit.group, &back);
            Cow::Owned(shown)
        }
        _ => Cow::Borrowed(page),
    }
}

/// Symbol Manager: every symbol with how many instances it has; rename, insert one in
/// the middle of the page, or delete those not in use.
pub fn symbol_manager_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.symbol_manager {
        return;
    }
    let mut open = true;
    let mut close = false;
    egui::Window::new("Symbol Manager")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .default_pos(egui::pos2(240.0, 160.0))
        .show(ctx, |ui| {
            if state.doc.symbols.is_empty() {
                ui.label(egui::RichText::new("No symbols yet. Object > Symbol > New Symbol makes one from the selection.").weak());
            }
            let rows: Vec<(SymbolId, usize)> = state.doc.symbols.iter().map(|s| (s.id, state.doc.instance_count(s.id))).collect();
            let (mut insert, mut delete) = (None, None);
            egui::Grid::new("symbols_grid").num_columns(4).spacing([12.0, 8.0]).show(ui, |ui| {
                for (i, (id, used)) in rows.iter().enumerate() {
                    let n = i + 1;
                    let mut name = state.doc.symbol(*id).map(|s| s.name.clone()).unwrap_or_default();
                    let field = egui::TextEdit::singleline(&mut name).desired_width(140.0).min_size(egui::vec2(140.0, 0.0));
                    if crate::a11y::add(ui, field, &format!("Name of symbol {n} (Symbol Manager)")).changed() {
                        state.doc.rename_symbol(*id, &name);
                    }
                    ui.label(if *used == 1 { "1 in use".to_owned() } else { format!("{used} in use") });
                    if crate::a11y::add(ui, egui::Button::new("Insert"), &format!("Insert symbol {n} (Symbol Manager)")).clicked() {
                        insert = Some(*id);
                    }
                    let resp = ui.add_enabled_ui(*used == 0, |ui| crate::a11y::add(ui, egui::Button::new("Delete"), &format!("Delete symbol {n} (Symbol Manager)"))).inner;
                    if resp.clicked() {
                        delete = Some(*id);
                    }
                    ui.end_row();
                }
            });
            if let Some(id) = insert {
                let at = state.doc.active_page().bounds().center();
                if let Some(obj) = state.doc.insert_symbol(id, at) {
                    state.selection = vec![obj];
                    state.marquee_selection = None;
                }
            }
            if let Some(id) = delete {
                state.doc.delete_symbol(id);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let unused = rows.iter().any(|(_, used)| *used == 0);
                if crate::a11y::window_button(ui, unused, "Delete Unused", "Symbol Manager").clicked() {
                    state.doc.delete_unused_symbols();
                }
                close = crate::a11y::window_button(ui, true, "Close", "Symbol Manager").clicked();
            });
        });
    if !open || close {
        state.symbol_manager = false;
    }
}

/// The label the Objects docker shows: an instance is named by its symbol.
pub fn object_label(state: &AppState, obj: &vs_core::Object) -> String {
    match state.doc.symbol_name(obj) {
        Some(name) if obj.name.is_none() && matches!(obj.shape, Shape::Group { .. }) => format!("{name} (symbol)"),
        _ => obj.label(),
    }
}

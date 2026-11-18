//! Tables: picking cells with the Table tool, typing into a cell, and the Table menu's
//! commands and windows.

use egui::{Color32, Stroke};
use vs_core::kurbo::{Point, Rect};
use vs_core::{Cells, ObjectId, Separator, Shape};

use crate::app::AppState;

/// Rows and columns a new table starts with, unless the property bar says otherwise.
pub const DEFAULT_ROWS: u32 = 3;
pub const DEFAULT_COLUMNS: u32 = 3;

#[derive(Default)]
pub struct TableState {
    /// The cells picked with the Table tool: the table, the block, and the place the
    /// pick started from (Shift+click and drags stretch the block from it).
    pub cells: Option<Picked>,
    /// A cell being typed into: its text is out on the page while it is edited.
    pub editing: Option<CellEdit>,
    pub dialog: Option<TableDialog>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picked {
    pub table: ObjectId,
    pub block: Cells,
    pub anchor: (usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellEdit {
    pub table: ObjectId,
    pub cell: usize,
    /// The text on the page standing in for the cell's while it is typed into.
    pub text: ObjectId,
}

/// The separator choices of the conversion windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeparatorChoice {
    #[default]
    Commas,
    Tabs,
    Paragraphs,
    UserDefined,
}

impl SeparatorChoice {
    const ALL: [SeparatorChoice; 4] = [Self::Commas, Self::Tabs, Self::Paragraphs, Self::UserDefined];

    fn label(self) -> &'static str {
        match self {
            Self::Commas => "Commas",
            Self::Tabs => "Tabs",
            Self::Paragraphs => "Paragraphs",
            Self::UserDefined => "User defined",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TableDialog {
    /// Create New Table: rows, columns and size in the document's unit.
    Create { rows: u32, columns: u32, width: f64, height: f64 },
    /// Split into Rows or Columns: how many parts.
    Split { columns: bool, parts: u32 },
    /// Convert Text to Table or Table to Text.
    Convert { to_table: bool, choice: SeparatorChoice, other: String },
}

/// The selected table, when it is the only selected object.
pub fn selected_table(state: &AppState) -> Option<ObjectId> {
    match state.selection[..] {
        [id] if state.doc.is_table(id) => Some(id),
        _ => None,
    }
}

/// The picked cells, while the Table tool is on, their table is the selection and they
/// are still on its grid.
pub fn picked(state: &AppState) -> Option<Picked> {
    if state.tool != crate::toolbox::Tool::Table {
        return None;
    }
    let p = state.table.cells?;
    let obj = state.doc.active_page().object(p.table)?;
    let t = obj.table()?;
    let fits = p.block.row + p.block.rows <= t.rows.len() && p.block.col + p.block.cols <= t.columns.len();
    (selected_table(state) == Some(p.table) && fits).then_some(p)
}

/// The topmost unlocked table at `p` (page space), and the grid place there.
pub fn place_at(state: &AppState, p: Point) -> Option<(ObjectId, (usize, usize))> {
    let page = state.doc.active_page();
    page.objects.iter().rev().filter(|o| !o.locked && page.is_selectable(o)).find_map(|o| {
        let place = o.table()?.place_at(o.transform.inverse() * p)?;
        Some((o.id, place))
    })
}

fn pick(state: &mut AppState, table: ObjectId, block: Cells, anchor: (usize, usize)) {
    let block = state.doc.active_page().object(table).and_then(|o| o.table()).map_or(block, |t| t.expand(block));
    state.selection = vec![table];
    state.marquee_selection = None;
    state.table.cells = Some(Picked { table, block, anchor });
}

/// A press with the Table tool at `p`: pick the cell there (Shift stretches the pick to
/// it). Returns whether it was on a table, and whether it was on the one cell already
/// picked (a click there types into it).
pub fn press(state: &mut AppState, p: Point, shift: bool) -> Option<bool> {
    crate::text_tool::finish(state);
    let (table, place) = place_at(state, p)?;
    let before = picked(state);
    match before {
        Some(b) if shift && b.table == table => pick(state, table, Cells::spanning(b.anchor, place), b.anchor),
        _ => pick(state, table, Cells::one(place.0, place.1), place),
    }
    let again = before.is_some_and(|b| b.table == table && single_cell(state, b.block) && b.block.contains(place.0, place.1));
    Some(again)
}

fn single_cell(state: &AppState, block: Cells) -> bool {
    let p = state.table.cells;
    p.and_then(|p| state.doc.active_page().object(p.table)).and_then(|o| o.table()).is_some_and(|t| t.cells_in(block).len() == 1)
}

/// Dragging across cells: the pick runs from where the press was to the pointer.
/// Returns whether the pick changed.
pub fn drag_to(state: &mut AppState, p: Point) -> bool {
    let Some(b) = picked(state) else { return false };
    let Some(obj) = state.doc.active_page().object(b.table) else { return false };
    let Some(t) = obj.table() else { return false };
    let local = obj.transform.inverse() * p;
    let size = t.size();
    // Past the edge, the outermost cells.
    let inside = Point::new(local.x.clamp(0.0, size.width - 1e-6), local.y.clamp(0.0, size.height - 1e-6));
    let Some(place) = t.place_at(inside) else { return false };
    pick(state, b.table, Cells::spanning(b.anchor, place), b.anchor);
    picked(state).is_some_and(|now| now.block != b.block)
}

/// A click with the Text tool in a table cell: type into it.
pub fn type_at(state: &mut AppState, p: Point) {
    let Some((table, (row, col))) = place_at(state, p) else { return };
    let Some(k) = state.doc.active_page().object(table).and_then(|o| o.table()).and_then(|t| t.cell_at(row, col)) else { return };
    begin_typing(state, table, k);
}

/// Release after a press on the one picked cell: start typing into it if the pick did
/// not move on.
pub fn release(state: &mut AppState, again: bool) {
    if let Some(b) = picked(state)
        && again
        && single_cell(state, b.block)
        && let Some(k) = state.doc.active_page().object(b.table).and_then(|o| o.table()).and_then(|t| t.cell_at(b.block.row, b.block.col))
    {
        begin_typing(state, b.table, k);
    }
}

/// Type into cell `k` of `table`: its text comes out onto the page, where it is, for the
/// Text tool to edit; [`end_typing`] puts it back.
pub fn begin_typing(state: &mut AppState, table: ObjectId, k: usize) {
    crate::text_tool::finish(state);
    let Some(obj) = state.doc.active_page().object(table) else { return };
    let Some(member) = obj.cell_text(k) else { return };
    let transform = obj.transform * member.transform;
    let (shape, style) = (member.shape.clone(), member.style.clone());
    let Shape::Text(t) = &shape else { return };
    let caret = t.content.chars().count();
    let place = obj.table().map(|t| t.cells[k]);
    // The cell shows no text while its copy is out.
    if let Some(Shape::Group { children }) = state.doc.active_page_mut().object_mut(table).map(|o| &mut o.shape)
        && let Some(Shape::Text(t)) = children.get_mut(2 * k + 1).map(|m| &mut m.shape)
    {
        t.content.clear();
    }
    let id = state.doc.add_object(shape, style);
    if let Some(o) = state.doc.active_page_mut().object_mut(id) {
        o.transform = transform;
    }
    state.selection = vec![id];
    state.marquee_selection = None;
    state.table.editing = Some(CellEdit { table, cell: k, text: id });
    if let Some(c) = place {
        state.table.cells = Some(Picked { table, block: c, anchor: (c.row, c.col) });
    }
    state.tools.text = Some(crate::text_tool::TextEdit { id, caret });
}

/// Typing into a cell ended: its text goes back into the cell, and the table and the
/// cell are picked again.
pub fn end_typing(state: &mut AppState) {
    let Some(edit) = state.table.editing.take() else { return };
    let Some(typed) = state.doc.active_page().object(edit.text).cloned() else { return };
    state.doc.delete_objects(&[edit.text]);
    if let Shape::Text(text) = typed.shape {
        state.doc.set_cell_text(edit.table, edit.cell, text, typed.style);
    }
    if state.doc.is_table(edit.table) {
        state.selection = vec![edit.table];
        state.marquee_selection = None;
    } else {
        state.selection.retain(|id| *id != edit.text);
    }
}

/// The picked cells, highlighted on the selected table.
pub fn draw_picked(painter: &egui::Painter, state: &AppState, to_screen: impl Fn(Point) -> egui::Pos2) {
    let Some(b) = picked(state) else { return };
    if state.table.editing.is_some() {
        return;
    }
    let Some(obj) = state.doc.active_page().object(b.table) else { return };
    let Some(t) = obj.table() else { return };
    let r: Rect = t.rect(b.block);
    let corners = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)];
    let points: Vec<egui::Pos2> = corners.iter().map(|c| to_screen(obj.transform * *c)).collect();
    let blue = Color32::from_rgb(0x1E, 0x6F, 0xD9);
    painter.add(egui::Shape::convex_polygon(points, blue.gamma_multiply(0.25), Stroke::new(1.5, blue)));
}

/// Whether each Table menu command can run now.
pub fn can_insert(state: &AppState) -> bool {
    picked(state).is_some()
}

pub fn can_merge(state: &AppState) -> bool {
    picked(state).is_some_and(|b| !single_cell(state, b.block))
}

pub fn can_split(state: &AppState) -> bool {
    picked(state).is_some_and(|b| single_cell(state, b.block))
}

pub fn can_text_to_table(state: &AppState) -> bool {
    match state.selection[..] {
        [id] => state.doc.can_convert_text_to_table(id),
        _ => false,
    }
}

/// Insert > Row Above / Row Below / Column Left / Column Right: as many as are picked;
/// the new ones are picked.
pub fn insert(state: &mut AppState, columns: bool, after: bool) {
    crate::text_tool::finish(state);
    let Some(b) = picked(state) else { return };
    let new = if columns { state.doc.insert_table_columns(b.table, b.block, after) } else { state.doc.insert_table_rows(b.table, b.block, after) };
    if let Some(block) = new {
        pick(state, b.table, block, (block.row, block.col));
    }
}

/// Merge Cells: the picked cells become one, which stays picked.
pub fn merge(state: &mut AppState) {
    crate::text_tool::finish(state);
    let Some(b) = picked(state) else { return };
    if let Some(block) = state.doc.merge_table_cells(b.table, b.block) {
        pick(state, b.table, block, (block.row, block.col));
    }
}

pub fn open_create(state: &mut AppState) {
    let s = &state.tools.settings;
    let unit = state.doc.display_unit;
    let page = state.doc.active_page().size;
    // Half the page wide, a row per 0.4 inch, to start.
    let (width, height) = (page.width / 2.0, 28.8 * f64::from(s.table_rows));
    state.table.dialog = Some(TableDialog::Create { rows: s.table_rows, columns: s.table_columns, width: unit.from_points(width), height: unit.from_points(height) });
}

pub fn open_split(state: &mut AppState, columns: bool) {
    state.table.dialog = Some(TableDialog::Split { columns, parts: 2 });
}

pub fn open_convert(state: &mut AppState, to_table: bool) {
    crate::text_tool::finish(state);
    state.table.dialog = Some(TableDialog::Convert { to_table, choice: SeparatorChoice::Commas, other: String::new() });
}

fn separator(choice: SeparatorChoice, other: &str) -> Option<Separator> {
    Some(match choice {
        SeparatorChoice::Commas => Separator::Commas,
        SeparatorChoice::Tabs => Separator::Tabs,
        SeparatorChoice::Paragraphs => Separator::Paragraphs,
        SeparatorChoice::UserDefined => Separator::Other(other.chars().next()?),
    })
}

fn apply(state: &mut AppState, dialog: &TableDialog) {
    crate::text_tool::finish(state);
    match dialog {
        TableDialog::Create { rows, columns, width, height } => {
            let unit = state.doc.display_unit;
            let size = vs_core::kurbo::Size::new(unit.to_points(*width), unit.to_points(*height));
            let centre = state.doc.active_page().bounds().center();
            let rect = Rect::from_center_size(centre, size);
            let font = state.tools.settings.text.clone();
            let id = state.doc.create_table(rect, *rows as usize, *columns as usize, &font);
            state.selection = vec![id];
            state.marquee_selection = None;
            state.table.cells = None;
        }
        TableDialog::Split { columns, parts } => {
            let Some(b) = picked(state) else { return };
            if let Some(block) = state.doc.split_table_cell(b.table, b.block, *parts as usize, *columns) {
                pick(state, b.table, block, (block.row, block.col));
            }
        }
        TableDialog::Convert { to_table, choice, other } => {
            let Some(sep) = separator(*choice, other) else { return };
            let [id] = state.selection[..] else { return };
            let new = if *to_table { state.doc.text_to_table(id, sep) } else { state.doc.table_to_text(id, sep) };
            if let Some(new) = new {
                state.selection = vec![new];
                state.marquee_selection = None;
                state.table.cells = None;
            }
        }
    }
}

/// The Table menu's windows: Create New Table, Split into Rows or Columns, and the two
/// conversions.
pub fn table_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dialog) = state.table.dialog.clone() else { return };
    let title = match &dialog {
        TableDialog::Create { .. } => "Create New Table",
        TableDialog::Split { columns: false, .. } => "Split Cells into Rows",
        TableDialog::Split { columns: true, .. } => "Split Cells into Columns",
        TableDialog::Convert { to_table: true, .. } => "Convert Text to Table",
        TableDialog::Convert { to_table: false, .. } => "Convert Table to Text",
    };
    let unit = state.doc.display_unit;
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let mut valid = true;
            match &mut dialog {
                TableDialog::Create { rows, columns, width, height } => {
                    egui::Grid::new("create_table_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Rows:");
                        crate::a11y::add(ui, egui::DragValue::new(rows).range(1..=99).speed(0.1), &format!("Rows ({title})"));
                        ui.end_row();
                        ui.label("Columns:");
                        crate::a11y::add(ui, egui::DragValue::new(columns).range(1..=99).speed(0.1), &format!("Columns ({title})"));
                        ui.end_row();
                        let min = unit.from_points(vs_core::table::MIN_TRACK);
                        for (label, v) in [("Width", width), ("Height", height)] {
                            ui.label(format!("{label}:"));
                            let drag = egui::DragValue::new(v).speed(unit.step() / 2.0).range(min..=f64::MAX).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix()));
                            crate::a11y::add(ui, drag, &format!("{label} ({title})"));
                            ui.end_row();
                        }
                    });
                }
                TableDialog::Split { columns, parts } => {
                    ui.horizontal(|ui| {
                        ui.label(if *columns { "Number of columns:" } else { "Number of rows:" });
                        crate::a11y::add(ui, egui::DragValue::new(parts).range(2..=99).speed(0.1), &format!("Number of parts ({title})"));
                    });
                }
                TableDialog::Convert { to_table, choice, other } => {
                    ui.label(if *to_table { "Create columns based on the following separator:" } else { "Separate text with the following:" });
                    for c in SeparatorChoice::ALL {
                        let r = ui.radio_value(choice, c, c.label());
                        crate::a11y::name(&r, &format!("{} ({title})", c.label()));
                    }
                    ui.add_enabled_ui(*choice == SeparatorChoice::UserDefined, |ui| {
                        crate::a11y::add(ui, egui::TextEdit::singleline(other).char_limit(1).desired_width(30.0), &format!("Separator character ({title})"));
                    });
                    valid = separator(*choice, other).is_some();
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok = crate::a11y::window_button(ui, valid, "OK", title).clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", title).clicked();
            });
        });
    state.table.dialog = (open && !ok && !cancel).then(|| dialog.clone());
    if ok {
        apply(state, &dialog);
    }
}

/// The Table tool's property bar: rows and columns of new tables.
pub fn controls(ui: &mut egui::Ui, state: &mut AppState) {
    use egui_material_icons::icons::{ICON_TABLE_ROWS, ICON_VIEW_COLUMN};
    let s = &mut state.tools.settings;
    let mut rows = f64::from(s.table_rows);
    if crate::chrome::number_row(ui, Some(ICON_TABLE_ROWS), "Table rows", &mut rows, "", 1.0..=99.0, 1.0, 0) {
        s.table_rows = rows as u32;
    }
    let mut columns = f64::from(s.table_columns);
    if crate::chrome::number_row(ui, Some(ICON_VIEW_COLUMN), "Table columns", &mut columns, "", 1.0..=99.0, 1.0, 0) {
        s.table_columns = columns as u32;
    }
}

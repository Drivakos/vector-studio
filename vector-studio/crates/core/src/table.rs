//! Tables: a group of cells laid out on a grid of rows and columns. Each cell is a
//! rectangle (its fill and borders) with paragraph text inside; a merged cell spans
//! several places of the grid. Rows grow to fit the text in them.

use kurbo::{Affine, Point, Rect, Size, Vec2};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, ObjectId, Shape, Style};
use crate::text::Text;

/// Space between a cell's edges and its text, in points.
pub const CELL_MARGIN: f64 = 2.0;
/// The narrowest column and the lowest row, in points.
pub const MIN_TRACK: f64 = 2.0 * CELL_MARGIN + 1.0;
const EPS: f64 = 1e-6;

/// A block of grid places: `rows` × `cols` from the top-left place (`row`, `col`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cells {
    pub row: usize,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
}

impl Cells {
    pub fn one(row: usize, col: usize) -> Self {
        Self { row, col, rows: 1, cols: 1 }
    }

    /// The block with places `a` and `b` (row, column) at opposite corners.
    pub fn spanning(a: (usize, usize), b: (usize, usize)) -> Self {
        Self { row: a.0.min(b.0), col: a.1.min(b.1), rows: a.0.abs_diff(b.0) + 1, cols: a.1.abs_diff(b.1) + 1 }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        (self.row..self.row + self.rows).contains(&row) && (self.col..self.col + self.cols).contains(&col)
    }

    fn overlaps(&self, o: &Cells) -> bool {
        self.row < o.row + o.rows && o.row < self.row + self.rows && self.col < o.col + o.cols && o.col < self.col + self.cols
    }

    fn union(&self, o: &Cells) -> Cells {
        let (row, col) = (self.row.min(o.row), self.col.min(o.col));
        let rows = (self.row + self.rows).max(o.row + o.rows) - row;
        let cols = (self.col + self.cols).max(o.col + o.cols) - col;
        Cells { row, col, rows, cols }
    }

    fn transposed(self) -> Cells {
        Cells { row: self.col, col: self.row, rows: self.cols, cols: self.rows }
    }
}

/// The grid of a table group. Cell `k` is drawn by the group's members `2k` (its
/// rectangle) and `2k + 1` (its text); the table's top-left corner is at its origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    /// Column widths, left to right, in points.
    pub columns: Vec<f64>,
    /// Row heights, top to bottom, in points.
    pub rows: Vec<f64>,
    /// Every cell, in reading order of their top-left places.
    pub cells: Vec<Cells>,
}

/// Where a cell's members come from when the grid changes.
#[derive(Clone, Debug, PartialEq)]
enum From {
    /// Cell `i` as it was.
    Keep(usize),
    /// A new, empty cell styled like cell `i`.
    Copy(usize),
    /// Cells merged into one, in reading order: the first one's style, every text.
    Merge(Vec<usize>),
}

impl From {
    fn first(&self) -> usize {
        match self {
            From::Keep(i) | From::Copy(i) => *i,
            From::Merge(v) => v[0],
        }
    }
}

/// A grid being changed: rows and columns, and each cell with where it comes from.
struct Work {
    rows: Vec<f64>,
    columns: Vec<f64>,
    cells: Vec<(Cells, From)>,
}

impl Work {
    fn new(t: &Table) -> Self {
        Self { rows: t.rows.clone(), columns: t.columns.clone(), cells: t.cells.iter().enumerate().map(|(i, c)| (*c, From::Keep(i))).collect() }
    }

    fn transpose(&mut self) {
        std::mem::swap(&mut self.rows, &mut self.columns);
        self.cells.iter_mut().for_each(|(c, _)| *c = c.transposed());
    }

    fn at(&self, row: usize, col: usize) -> Option<usize> {
        self.cells.iter().position(|(c, _)| c.contains(row, col))
    }

    fn row_y(&self, row: usize) -> f64 {
        self.rows[..row].iter().sum()
    }

    /// A row `height` high before row `at`, its cells styled like those in row `like`
    /// (counted before the insertion). Cells spanning across `at` grow into it.
    fn insert_row(&mut self, at: usize, height: f64, like: usize) {
        let mut spanned = vec![false; self.columns.len()];
        for (c, _) in &mut self.cells {
            if c.row >= at {
                c.row += 1;
            } else if c.row + c.rows > at {
                c.rows += 1;
                spanned[c.col..c.col + c.cols].iter_mut().for_each(|s| *s = true);
            }
        }
        self.rows.insert(at, height);
        let like = if like >= at { like + 1 } else { like };
        for (col, _) in spanned.iter().enumerate().filter(|(_, s)| !**s) {
            let source = self.at(like, col).map_or(0, |k| self.cells[k].1.first());
            self.cells.push((Cells::one(at, col), From::Copy(source)));
        }
    }

    /// Cut row `row` in two, the first part `top` high; cells across it span both parts.
    fn split_track(&mut self, row: usize, top: f64) {
        for (c, _) in &mut self.cells {
            if c.row > row {
                c.row += 1;
            } else if c.row + c.rows > row {
                c.rows += 1;
            }
        }
        let h = self.rows[row];
        self.rows.splice(row..=row, [top, h - top]);
    }

    /// Cut cell `k` into `n` cells of equal height, adding grid lines where needed.
    /// Returns the block they fill.
    fn split_rows(&mut self, k: usize, n: usize) -> Cells {
        let c = self.cells[k].0;
        let y0 = self.row_y(c.row);
        let h: f64 = self.rows[c.row..c.row + c.rows].iter().sum();
        let ys: Vec<f64> = (0..=n).map(|i| y0 + h * i as f64 / n as f64).collect();
        for &y in &ys[1..n] {
            let mut top = 0.0;
            for row in 0..self.rows.len() {
                let bottom = top + self.rows[row];
                if y > top + EPS && y < bottom - EPS {
                    self.split_track(row, y - top);
                    break;
                }
                top = bottom;
            }
        }
        let line = |w: &Work, y: f64| (0..=w.rows.len()).min_by(|a, b| (w.row_y(*a) - y).abs().total_cmp(&(w.row_y(*b) - y).abs())).unwrap_or(0);
        let lines: Vec<usize> = ys.iter().map(|y| line(self, *y)).collect();
        let (cell, from) = self.cells.remove(k);
        let first = from.first();
        for i in 0..n {
            let part = Cells { row: lines[i], rows: lines[i + 1] - lines[i], ..cell };
            self.cells.push((part, if i == 0 { from.clone() } else { From::Copy(first) }));
        }
        Cells { row: lines[0], rows: lines[n] - lines[0], ..cell }
    }

    fn finish(mut self) -> (Table, Vec<From>) {
        self.cells.sort_by_key(|(c, _)| (c.row, c.col));
        let (cells, from) = self.cells.into_iter().unzip();
        (Table { columns: self.columns, rows: self.rows, cells }, from)
    }
}

impl Table {
    /// `rows` × `cols` cells of equal size filling `size`.
    pub fn new(rows: usize, cols: usize, size: Size) -> Self {
        let (rows, cols) = (rows.max(1), cols.max(1));
        let w = (size.width / cols as f64).max(MIN_TRACK);
        let h = (size.height / rows as f64).max(MIN_TRACK);
        let cells = (0..rows).flat_map(|r| (0..cols).map(move |c| Cells::one(r, c))).collect();
        Self { columns: vec![w; cols], rows: vec![h; rows], cells }
    }

    pub fn size(&self) -> Size {
        Size::new(self.columns.iter().sum(), self.rows.iter().sum())
    }

    /// The whole grid as one block.
    pub fn all(&self) -> Cells {
        Cells { row: 0, col: 0, rows: self.rows.len(), cols: self.columns.len() }
    }

    /// The area of a block, in the table's space.
    pub fn rect(&self, c: Cells) -> Rect {
        let x0: f64 = self.columns[..c.col].iter().sum();
        let y0: f64 = self.rows[..c.row].iter().sum();
        let w: f64 = self.columns[c.col..c.col + c.cols].iter().sum();
        let h: f64 = self.rows[c.row..c.row + c.rows].iter().sum();
        Rect::new(x0, y0, x0 + w, y0 + h)
    }

    /// The grid place (row, column) at `p`, in the table's space.
    pub fn place_at(&self, p: Point) -> Option<(usize, usize)> {
        let find = |tracks: &[f64], v: f64| {
            let mut edge = 0.0;
            tracks.iter().position(|t| {
                edge += t;
                v < edge
            })
        };
        if p.x < 0.0 || p.y < 0.0 {
            return None;
        }
        Some((find(&self.rows, p.y)?, find(&self.columns, p.x)?))
    }

    /// The cell covering a grid place.
    pub fn cell_at(&self, row: usize, col: usize) -> Option<usize> {
        self.cells.iter().position(|c| c.contains(row, col))
    }

    /// The smallest block holding `range` that cuts through no cell.
    pub fn expand(&self, range: Cells) -> Cells {
        let mut out = range;
        loop {
            let grown = self.cells.iter().filter(|c| c.overlaps(&out)).fold(out, |r, c| r.union(c));
            if grown == out {
                return out;
            }
            out = grown;
        }
    }

    /// The cells in `range` (expanded so it cuts through none), in reading order.
    pub fn cells_in(&self, range: Cells) -> Vec<usize> {
        let range = self.expand(range);
        (0..self.cells.len()).filter(|k| self.cells[*k].overlaps(&range)).collect()
    }

    /// Whether this grid describes `children`: a rectangle and a text per cell.
    fn fits(&self, children: &[Object]) -> bool {
        children.len() == 2 * self.cells.len()
            && children.chunks(2).all(|pair| matches!(pair[0].shape, Shape::Rectangle { .. }) && matches!(pair[1].shape, Shape::Text(_)))
    }

    /// Put each cell's rectangle and text frame where the grid has it.
    fn place(&self, children: &mut [Object]) {
        for (k, c) in self.cells.iter().enumerate() {
            let r = self.rect(*c);
            children[2 * k].shape = Shape::Rectangle { rect: r, corner_radius: 0.0 };
            children[2 * k].transform = Affine::IDENTITY;
            let text = &mut children[2 * k + 1];
            text.transform = Affine::translate((r.x0 + CELL_MARGIN, r.y0 + CELL_MARGIN));
            if let Shape::Text(t) = &mut text.shape {
                t.frame = Some(Size::new((r.width() - 2.0 * CELL_MARGIN).max(1.0), (r.height() - 2.0 * CELL_MARGIN).max(1.0)));
            }
        }
    }

    /// Grow rows until the text of every cell fits: one-row cells first, then each
    /// taller cell through its last row.
    fn fit(&mut self, children: &[Object]) {
        let needed: Vec<f64> = (0..self.cells.len())
            .map(|k| match &children[2 * k + 1].shape {
                Shape::Text(t) if !t.content.is_empty() => {
                    let free = Text { frame: t.frame.map(|f| Size::new(f.width, 1e9)), ..t.clone() };
                    let l = free.layout();
                    l.lines.last().map_or(0.0, |line| line.baseline + l.descent) + 2.0 * CELL_MARGIN
                }
                _ => 0.0,
            })
            .collect();
        let mut order: Vec<usize> = (0..self.cells.len()).collect();
        order.sort_by_key(|k| self.cells[*k].rows);
        for k in order {
            let c = self.cells[k];
            let have: f64 = self.rows[c.row..c.row + c.rows].iter().sum();
            if needed[k] > have + EPS {
                self.rows[c.row + c.rows - 1] += needed[k] - have;
            }
        }
    }
}

/// A cell's members: its rectangle and its text, placed later by [`Table::place`].
fn cell_members(doc: &mut Document, rect_style: Style, text: Text, text_style: Style) -> [Object; 2] {
    let member = |doc: &mut Document, shape: Shape, style: Style| {
        let mut o = Object::new(ObjectId(0), shape, style);
        o.id = doc.alloc_object_id();
        o
    };
    let rect = member(doc, Shape::Rectangle { rect: Rect::ZERO, corner_radius: 0.0 }, rect_style);
    let text = member(doc, Shape::Text(Text { frame: Some(Size::new(1.0, 1.0)), on_path: None, field: None, ..text }), text_style);
    [rect, text]
}

/// How cells are told apart in text: Convert Text to Table and Convert Table to Text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    #[default]
    Commas,
    Tabs,
    /// Every cell on a line of its own.
    Paragraphs,
    /// Another character.
    Other(char),
}

impl Separator {
    fn char(self) -> Option<char> {
        match self {
            Separator::Commas => Some(','),
            Separator::Tabs => Some('\t'),
            Separator::Paragraphs => None,
            Separator::Other(c) => Some(c),
        }
    }
}

impl Object {
    /// The grid of a table group, while its members still are a rectangle and a text per
    /// cell.
    pub fn table(&self) -> Option<&Table> {
        match (&self.table, &self.shape) {
            (Some(t), Shape::Group { children }) if t.fits(children) => Some(t),
            _ => None,
        }
    }

    /// Cell `k`'s text member.
    pub fn cell_text(&self, k: usize) -> Option<&Object> {
        match &self.shape {
            Shape::Group { children } if self.table().is_some() => children.get(2 * k + 1),
            _ => None,
        }
    }
}

impl Document {
    fn table_object(&self, id: ObjectId) -> Option<(&Object, &Table)> {
        let obj = self.active_page().object(id).filter(|o| !o.locked)?;
        Some((obj, obj.table()?))
    }

    /// Whether `id` is an unlocked table on the active page.
    pub fn is_table(&self, id: ObjectId) -> bool {
        self.table_object(id).is_some()
    }

    /// A new table filling `rect`: `rows` × `cols` cells with black borders, their text
    /// in `font`'s font. Returns its id.
    pub fn create_table(&mut self, rect: Rect, rows: usize, cols: usize, font: &Text) -> ObjectId {
        let table = Table::new(rows, cols, rect.size());
        let text = Text { content: String::new(), ..font.clone() };
        let mut children = Vec::new();
        for _ in 0..table.cells.len() {
            children.extend(cell_members(self, Style::new_object(), text.clone(), Style::new_text()));
        }
        table.place(&mut children);
        let id = self.add_object(Shape::Group { children }, Style::default());
        if let Some(obj) = self.active_page_mut().object_mut(id) {
            obj.transform = Affine::translate(rect.origin().to_vec2());
            obj.table = Some(table);
        }
        id
    }

    /// Give table `id` the grid `table`, its cells' members coming from `from`, and grow
    /// rows to fit.
    fn rebuild_table(&mut self, id: ObjectId, mut table: Table, from: Vec<From>) -> Option<()> {
        let old = match &self.active_page().object(id)?.shape {
            Shape::Group { children } => children.clone(),
            _ => return None,
        };
        let mut children = Vec::with_capacity(2 * from.len());
        for f in &from {
            match f {
                From::Keep(i) => children.extend_from_slice(&old[2 * i..2 * i + 2]),
                From::Copy(i) => {
                    let Shape::Text(t) = &old[2 * i + 1].shape else { return None };
                    let text = Text { content: String::new(), ..t.clone() };
                    children.extend(cell_members(self, old[2 * i].style.clone(), text, old[2 * i + 1].style.clone()));
                }
                From::Merge(list) => {
                    let mut pair = old[2 * list[0]..2 * list[0] + 2].to_vec();
                    let texts: Vec<String> = list
                        .iter()
                        .filter_map(|i| match &old[2 * i + 1].shape {
                            Shape::Text(t) if !t.content.is_empty() => Some(t.content.clone()),
                            _ => None,
                        })
                        .collect();
                    if let Shape::Text(t) = &mut pair[1].shape {
                        t.content = texts.join("\n");
                    }
                    children.extend(pair);
                }
            }
        }
        table.place(&mut children);
        table.fit(&children);
        table.place(&mut children);
        let obj = self.active_page_mut().object_mut(id)?;
        obj.shape = Shape::Group { children };
        obj.table = Some(table);
        Some(())
    }

    fn change_table<R>(&mut self, id: ObjectId, f: impl FnOnce(&Table, &mut Work) -> R) -> Option<R> {
        let (_, table) = self.table_object(id)?;
        let table = table.clone();
        let mut work = Work::new(&table);
        let out = f(&table, &mut work);
        let (table, from) = work.finish();
        self.rebuild_table(id, table, from)?;
        Some(out)
    }

    /// Insert as many rows as `range` covers, above it or below, as high as its rows and
    /// styled like its top or bottom row. Returns the new rows.
    pub fn insert_table_rows(&mut self, id: ObjectId, range: Cells, below: bool) -> Option<Cells> {
        self.change_table(id, |table, w| {
            let range = table.expand(range);
            let n = range.rows;
            let (at, like) = if below { (range.row + n, range.row + n - 1) } else { (range.row, range.row) };
            for k in (0..n).rev() {
                w.insert_row(at, table.rows[range.row + k], like);
            }
            Cells { row: at, col: 0, rows: n, cols: w.columns.len() }
        })
    }

    /// Insert as many columns as `range` covers, to its left or right. Returns them.
    pub fn insert_table_columns(&mut self, id: ObjectId, range: Cells, right: bool) -> Option<Cells> {
        self.change_table(id, |table, w| {
            let range = table.expand(range);
            let n = range.cols;
            let (at, like) = if right { (range.col + n, range.col + n - 1) } else { (range.col, range.col) };
            w.transpose();
            for k in (0..n).rev() {
                w.insert_row(at, table.columns[range.col + k], like);
            }
            w.transpose();
            Cells { row: 0, col: at, rows: w.rows.len(), cols: n }
        })
    }

    /// Merge the cells in `range` into one, keeping every text, one per line. Returns
    /// the merged cell's block.
    pub fn merge_table_cells(&mut self, id: ObjectId, range: Cells) -> Option<Cells> {
        let (_, table) = self.table_object(id)?;
        if table.cells_in(range).len() < 2 {
            return None;
        }
        self.change_table(id, |table, w| {
            let range = table.expand(range);
            let inside = table.cells_in(range);
            w.cells.retain(|(_, f)| !inside.contains(&f.first()));
            w.cells.push((range, From::Merge(inside)));
            range
        })
    }

    /// Split the one cell in `range` into `n` rows (or columns) of equal size. The first
    /// keeps the text. Returns the block the parts fill.
    pub fn split_table_cell(&mut self, id: ObjectId, range: Cells, n: usize, columns: bool) -> Option<Cells> {
        let (_, table) = self.table_object(id)?;
        let [k] = table.cells_in(range)[..] else { return None };
        if n < 2 {
            return None;
        }
        self.change_table(id, |_, w| {
            if columns {
                w.transpose();
            }
            let at = w.cells.iter().position(|(_, f)| *f == From::Keep(k)).expect("the cell is in the grid");
            let block = w.split_rows(at, n);
            if columns {
                w.transpose();
                block.transposed()
            } else {
                block
            }
        })
    }

    /// Restyle the rectangles of the cells in `range` of table `id`.
    pub fn restyle_table_cells(&mut self, id: ObjectId, range: Cells, mut f: impl FnMut(&mut Style)) {
        let Some((_, table)) = self.table_object(id) else { return };
        let cells = table.cells_in(range);
        if let Some(Shape::Group { children }) = self.active_page_mut().object_mut(id).map(|o| &mut o.shape) {
            for k in cells {
                f(&mut children[2 * k].style);
            }
        }
    }

    /// Set cell `k`'s text member (keeping its frame) and grow rows to fit.
    pub fn set_cell_text(&mut self, id: ObjectId, k: usize, text: Text, style: Style) -> Option<()> {
        let (_, table) = self.table_object(id)?;
        let table = table.clone();
        let obj = self.active_page_mut().object_mut(id)?;
        let Shape::Group { children } = &mut obj.shape else { return None };
        let member = children.get_mut(2 * k + 1)?;
        let Shape::Text(t) = &mut member.shape else { return None };
        *t = Text { frame: t.frame, on_path: None, field: None, ..text };
        member.style = style;
        let from = (0..table.cells.len()).map(From::Keep).collect();
        self.rebuild_table(id, table, from)
    }

    /// Convert Table to Text: the table becomes paragraph text in its place, one line per
    /// row with the cells told apart by `sep` (or one line per cell). Places covered by
    /// a merged cell add empty cells. Returns the text.
    pub fn table_to_text(&mut self, id: ObjectId, sep: Separator) -> Option<ObjectId> {
        let (obj, table) = self.table_object(id)?;
        let cell_text = |k: usize| match obj.cell_text(k).map(|o| &o.shape) {
            Some(Shape::Text(t)) => t.content.clone(),
            _ => String::new(),
        };
        let content = match sep.char() {
            None => (0..table.cells.len()).map(cell_text).collect::<Vec<_>>().join("\n"),
            Some(ch) => (0..table.rows.len())
                .map(|r| {
                    (0..table.columns.len())
                        .map(|c| match table.cell_at(r, c) {
                            Some(k) if table.cells[k].row == r && table.cells[k].col == c => cell_text(k).replace('\n', " "),
                            _ => String::new(),
                        })
                        .collect::<Vec<_>>()
                        .join(&ch.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let (font, style) = match obj.cell_text(0) {
            Some(Object { shape: Shape::Text(t), style, .. }) => (t.clone(), style.clone()),
            _ => (Text::new(""), Style::new_text()),
        };
        let text = Text { content, frame: Some(table.size()), ..font };
        let (transform, layer) = (obj.transform, obj.layer);
        let mut new = Object::new(ObjectId(0), Shape::Text(text), style);
        new.id = self.alloc_object_id();
        new.transform = transform;
        new.layer = layer;
        let new_id = new.id;
        let objects = &mut self.active_page_mut().objects;
        let at = objects.iter().position(|o| o.id == id)?;
        objects[at] = new;
        Some(new_id)
    }

    /// Whether `id` is unlocked text Convert Text to Table can take.
    pub fn can_convert_text_to_table(&self, id: ObjectId) -> bool {
        self.active_page().object(id).is_some_and(|o| !o.locked && matches!(&o.shape, Shape::Text(t) if t.on_path.is_none() && !t.content.is_empty()))
    }

    /// Convert Text to Table: each line becomes a row, cut into cells at `sep` (or one
    /// cell per line). Columns are as wide, and rows as high, as their widest and
    /// tallest text. The table takes the text's place. Returns it.
    pub fn text_to_table(&mut self, id: ObjectId, sep: Separator) -> Option<ObjectId> {
        if !self.can_convert_text_to_table(id) {
            return None;
        }
        let src = self.active_page().object(id)?.clone();
        let Shape::Text(source) = &src.shape else { return None };
        let mut lines: Vec<&str> = source.content.split('\n').collect();
        if lines.len() > 1 && lines.last() == Some(&"") {
            lines.pop();
        }
        let grid: Vec<Vec<String>> = lines
            .iter()
            .map(|l| match sep.char() {
                Some(ch) => l.split(ch).map(str::to_owned).collect(),
                None => vec![(*l).to_owned()],
            })
            .collect();
        let cols = grid.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let font = Text { content: String::new(), frame: None, on_path: None, field: None, columns: 1, gutter: 0.0, list: None, drop_cap: 0, tabs: Vec::new(), ..source.clone() };
        let measure = |s: &str| Text { content: s.to_owned(), ..font.clone() }.layout().bounds().size();
        let mut table = Table::new(grid.len(), cols, Size::ZERO);
        for (r, row) in grid.iter().enumerate() {
            for c in 0..cols {
                let size = measure(row.get(c).map_or("", String::as_str));
                // A point of slack, so the text does not wrap on rounding.
                table.columns[c] = table.columns[c].max(size.width + 2.0 * CELL_MARGIN + 1.0);
                table.rows[r] = table.rows[r].max(size.height + 2.0 * CELL_MARGIN);
            }
        }
        let mut children = Vec::new();
        for row in &grid {
            for c in 0..cols {
                let text = Text { content: row.get(c).cloned().unwrap_or_default(), ..font.clone() };
                children.extend(cell_members(self, Style::new_object(), text, src.style.clone()));
            }
        }
        table.place(&mut children);
        table.fit(&children);
        table.place(&mut children);
        let origin: Vec2 = source.layout().bounds().origin().to_vec2();
        let mut new = Object::new(ObjectId(0), Shape::Group { children }, Style::default());
        new.id = self.alloc_object_id();
        new.transform = src.transform * Affine::translate(origin);
        new.layer = src.layer;
        new.table = Some(table);
        let new_id = new.id;
        let objects = &mut self.active_page_mut().objects;
        let at = objects.iter().position(|o| o.id == id)?;
        objects[at] = new;
        Some(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(doc: &Document, id: ObjectId) -> Vec<String> {
        let obj = doc.active_page().object(id).unwrap();
        (0..obj.table().unwrap().cells.len())
            .map(|k| match &obj.cell_text(k).unwrap().shape {
                Shape::Text(t) => t.content.clone(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn table(doc: &Document, id: ObjectId) -> Table {
        doc.active_page().object(id).unwrap().table().unwrap().clone()
    }

    fn small(s: &str) -> Text {
        Text { size: 8.0, ..Text::new(s) }
    }

    fn sample() -> (Document, ObjectId) {
        let mut doc = Document::new("t");
        let id = doc.create_table(Rect::new(10.0, 20.0, 310.0, 140.0), 3, 3, &small(""));
        for k in 0..9 {
            doc.set_cell_text(id, k, small(&k.to_string()), Style::new_text()).unwrap();
        }
        (doc, id)
    }

    #[test]
    fn a_new_table_lays_out_equal_cells_with_text_inside() {
        let (doc, id) = sample();
        let obj = doc.active_page().object(id).unwrap();
        let t = obj.table().unwrap();
        assert_eq!((t.rows.len(), t.columns.len(), t.cells.len()), (3, 3, 9));
        assert_eq!(t.rect(Cells::one(1, 2)), Rect::new(200.0, 40.0, 300.0, 80.0));
        assert!((obj.world_bounds().x0 - 10.0).abs() < 1.0 && (obj.world_bounds().y0 - 20.0).abs() < 1.0);
        let text = obj.cell_text(5).unwrap();
        assert_eq!(text.transform.translation(), Vec2::new(200.0 + CELL_MARGIN, 40.0 + CELL_MARGIN));
        assert_eq!(t.place_at(Point::new(250.0, 50.0)), Some((1, 2)));
        assert_eq!(t.place_at(Point::new(301.0, 50.0)), None);
        assert_eq!(obj.label(), "Table");
    }

    #[test]
    fn rows_and_columns_go_in_beside_the_selection_and_merged_cells_grow_across() {
        let (mut doc, id) = sample();
        let merged = doc.merge_table_cells(id, Cells::spanning((0, 0), (1, 0))).unwrap();
        assert_eq!(merged, Cells { row: 0, col: 0, rows: 2, cols: 1 });
        assert_eq!(texts(&doc, id)[0], "0\n3");
        // A row between rows 0 and 1 runs into the merged cell, which grows.
        let new = doc.insert_table_rows(id, Cells::one(0, 1), true).unwrap();
        assert_eq!(new, Cells { row: 1, col: 0, rows: 1, cols: 3 });
        let t = table(&doc, id);
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.cells[0], Cells { row: 0, col: 0, rows: 3, cols: 1 });
        assert_eq!(t.cell_at(1, 1).map(|k| t.cells[k]), Some(Cells::one(1, 1)));
        assert_eq!(texts(&doc, id), ["0\n3", "1", "2", "", "", "4", "5", "6", "7", "8"]);
        // Two columns to the left of a two-column selection.
        let new = doc.insert_table_columns(id, Cells::spanning((3, 1), (3, 2)), false).unwrap();
        assert_eq!(new, Cells { row: 0, col: 1, rows: 4, cols: 2 });
        let t = table(&doc, id);
        assert_eq!(t.columns, [100.0; 5]);
        assert_eq!(t.cell_at(3, 3).map(|k| t.cells[k]), Some(Cells::one(3, 3)));
    }

    #[test]
    fn splitting_a_cell_adds_grid_lines_only_where_needed() {
        let (mut doc, id) = sample();
        // One cell into two rows: its row is cut, its neighbours span both halves.
        let parts = doc.split_table_cell(id, Cells::one(1, 1), 2, false).unwrap();
        assert_eq!(parts, Cells { row: 1, col: 1, rows: 2, cols: 1 });
        let t = table(&doc, id);
        assert_eq!(t.rows, [40.0, 20.0, 20.0, 40.0]);
        assert_eq!(t.cells[t.cell_at(1, 0).unwrap()], Cells { row: 1, col: 0, rows: 2, cols: 1 });
        assert_eq!(texts(&doc, id)[4], "4", "the first part keeps the text");
        assert_eq!(texts(&doc, id)[6], "", "the second part is new");
        // A cell split where a grid line already is adds none: 0–40 and 40–80.
        doc.merge_table_cells(id, Cells::spanning((0, 2), (1, 2))).unwrap();
        let rows = table(&doc, id).rows.len();
        let parts = doc.split_table_cell(id, Cells::one(0, 2), 2, false).unwrap();
        let t = table(&doc, id);
        assert_eq!(t.rows.len(), rows);
        assert_eq!(t.cells[t.cell_at(0, 2).unwrap()], Cells::one(0, 2));
        assert_eq!(t.cells[t.cell_at(1, 2).unwrap()], Cells { row: 1, col: 2, rows: 2, cols: 1 });
        assert_eq!(parts, Cells { row: 0, col: 2, rows: 3, cols: 1 });
        // Columns work the same way.
        let parts = doc.split_table_cell(id, Cells::one(3, 0), 3, true).unwrap();
        assert_eq!(parts.cols, 3);
        // (0, 0) now spans the three new columns; (0, 3) is the next cell.
        assert!(doc.split_table_cell(id, Cells::spanning((0, 0), (0, 3)), 2, false).is_none(), "one cell at a time");
    }

    #[test]
    fn rows_grow_to_fit_their_text() {
        let (mut doc, id) = sample();
        let long = Text::new("a\nb\nc\nd");
        doc.set_cell_text(id, 4, long, Style::new_text()).unwrap();
        let t = table(&doc, id);
        assert!(t.rows[1] > 40.0, "{:?}", t.rows);
        assert_eq!(t.rows[0], 40.0);
        let obj = doc.active_page().object(id).unwrap();
        let Shape::Text(text) = &obj.cell_text(4).unwrap().shape else { unreachable!() };
        assert!(!text.layout().overflow);
    }

    #[test]
    fn cell_fills_change_only_the_chosen_cells() {
        let (mut doc, id) = sample();
        doc.restyle_table_cells(id, Cells::one(0, 0), |s| s.fill = Some(crate::Fill::Solid(crate::Rgba::rgb(255, 0, 0))));
        let obj = doc.active_page().object(id).unwrap();
        let Shape::Group { children } = &obj.shape else { unreachable!() };
        assert!(children[0].style.fill.is_some());
        assert!(children[2].style.fill.is_none());
        assert_eq!(children[1].style, Style::new_text(), "the text keeps its colour");
    }

    #[test]
    fn text_and_tables_convert_both_ways() {
        let mut doc = Document::new("t");
        let src = doc.add_object(Shape::Text(Text::new("a,b\nc,d,e\n")), Style::new_text());
        assert!(doc.can_convert_text_to_table(src));
        let id = doc.text_to_table(src, Separator::Commas).unwrap();
        assert!(doc.active_page().object(src).is_none());
        let t = table(&doc, id);
        assert_eq!((t.rows.len(), t.columns.len()), (2, 3));
        assert_eq!(texts(&doc, id), ["a", "b", "", "c", "d", "e"]);
        let obj = doc.active_page().object(id).unwrap();
        for k in 0..6 {
            let Shape::Text(text) = &obj.cell_text(k).unwrap().shape else { unreachable!() };
            assert!(!text.layout().overflow && text.layout().lines.len() == 1, "cell {k} fits on one line");
        }
        doc.merge_table_cells(id, Cells::spanning((0, 1), (0, 2))).unwrap();
        let back = doc.table_to_text(id, Separator::Tabs).unwrap();
        let obj = doc.active_page().object(back).unwrap();
        let Shape::Text(text) = &obj.shape else { unreachable!() };
        assert_eq!(text.content, "a\tb\t\nc\td\te");
        assert!(text.is_paragraph());
        // Paragraphs: one cell per line.
        let id = doc.text_to_table(back, Separator::Paragraphs).unwrap();
        assert_eq!(table(&doc, id).columns.len(), 1);
        let back = doc.table_to_text(id, Separator::Paragraphs).unwrap();
        let Shape::Text(text) = &doc.active_page().object(back).unwrap().shape else { unreachable!() };
        assert_eq!(text.content, "a\tb\t\nc\td\te");
    }

    #[test]
    fn a_table_whose_members_were_changed_is_a_plain_group() {
        let (mut doc, id) = sample();
        doc.active_page_mut().object_mut(id).unwrap().convert_to_curves();
        let obj = doc.active_page().object(id).unwrap();
        assert!(obj.table().is_none());
        assert!(obj.table.is_none());
        assert!(doc.insert_table_rows(id, Cells::one(0, 0), true).is_none());
    }
}

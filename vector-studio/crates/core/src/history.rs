//! Snapshot undo/redo.
//!
//! The UI edits the document freely and calls [`History::commit`] whenever it is at
//! rest (no drag or text field in progress). Any change to page content since the last
//! commit becomes one undo step, so every edit path is undoable without bookkeeping.

use crate::document::Document;

#[derive(Clone, Debug)]
struct Snapshot {
    doc: Document,
    revision: u64,
}

#[derive(Clone, Debug)]
pub struct History {
    base: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    next_revision: u64,
    saved_revision: Option<u64>,
    limit: usize,
}

impl History {
    pub const DEFAULT_LIMIT: usize = 200;

    /// Start a history at `doc`, which counts as saved when `saved` is true.
    pub fn new(doc: &Document, saved: bool) -> Self {
        Self {
            base: Snapshot { doc: doc.clone(), revision: 0 },
            undo: Vec::new(),
            redo: Vec::new(),
            next_revision: 1,
            saved_revision: saved.then_some(0),
            limit: Self::DEFAULT_LIMIT,
        }
    }

    /// Record `doc` as a new step if its pages changed. Returns true if a step was added.
    pub fn commit(&mut self, doc: &Document) -> bool {
        if doc.pages() == self.base.doc.pages() {
            // Keep view-only state (active page, units) in the snapshot without a step.
            self.base.doc = doc.clone();
            return false;
        }
        let snapshot = Snapshot { doc: doc.clone(), revision: self.next_revision };
        self.next_revision += 1;
        self.undo.push(std::mem::replace(&mut self.base, snapshot));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Commit pending edits, then step back. Returns false if there is nothing to undo.
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        self.commit(doc);
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.base, prev));
        *doc = self.base.doc.clone();
        true
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        self.commit(doc);
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.base, next));
        *doc = self.base.doc.clone();
        true
    }

    /// Mark the current state as saved (call after committing).
    pub fn mark_saved(&mut self) {
        self.saved_revision = Some(self.base.revision);
    }

    /// Whether the last committed state differs from the saved one.
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != Some(self.base.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, Style};
    use kurbo::Rect;

    fn add(doc: &mut Document) {
        doc.add_object(Shape::Ellipse { rect: Rect::new(0.0, 0.0, 1.0, 1.0), arc: None }, Style::default());
    }

    fn count(doc: &Document) -> usize {
        doc.active_page().objects.len()
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut doc = Document::default();
        let mut h = History::new(&doc, true);
        add(&mut doc);
        h.commit(&doc);
        add(&mut doc); // uncommitted edit is committed by undo
        assert!(h.undo(&mut doc));
        assert_eq!(count(&doc), 1);
        assert!(h.undo(&mut doc));
        assert_eq!(count(&doc), 0);
        assert!(!h.undo(&mut doc));
        assert!(h.redo(&mut doc));
        assert!(h.redo(&mut doc));
        assert_eq!(count(&doc), 2);
        assert!(!h.redo(&mut doc));
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut doc = Document::default();
        let mut h = History::new(&doc, true);
        add(&mut doc);
        h.commit(&doc);
        h.undo(&mut doc);
        add(&mut doc);
        h.commit(&doc);
        assert!(!h.can_redo());
    }

    #[test]
    fn view_changes_are_not_steps() {
        let mut doc = Document::default();
        doc.insert_page_after(0).unwrap();
        let mut h = History::new(&doc, true);
        doc.set_active_page(0).unwrap();
        assert!(!h.commit(&doc));
        assert!(!h.is_dirty());
    }

    #[test]
    fn dirty_tracks_saved_revision() {
        let mut doc = Document::default();
        let mut h = History::new(&doc, true);
        add(&mut doc);
        h.commit(&doc);
        assert!(h.is_dirty());
        h.mark_saved();
        assert!(!h.is_dirty());
        h.undo(&mut doc);
        assert!(h.is_dirty());
        h.redo(&mut doc);
        assert!(!h.is_dirty());
    }
}

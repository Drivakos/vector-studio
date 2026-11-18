//! Symbols: objects kept once in the document and placed as instances. An instance is a
//! group whose members copy its symbol's objects; changing any instance's members (with
//! Edit Symbol, or a fill or outline set on the instance) changes the symbol and every
//! instance of it. Symbols cannot hold symbols: instances inside one become plain groups.

use std::collections::HashMap;

use kurbo::{Affine, Point, Rect};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, ObjectId, Shape, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    /// In the symbol's own space; an instance's transform places them.
    pub objects: Vec<Object>,
}

/// Turn `obj` and everything in it from instances into plain groups.
pub(crate) fn unlink(obj: &mut Object) {
    obj.symbol = None;
    if let Shape::Group { children } = &mut obj.shape {
        children.iter_mut().for_each(unlink);
    }
    obj.contents.iter_mut().for_each(unlink);
}

/// Whether `a` and `b` are the same but for ids and layers, which members don't use.
fn same(a: &Object, b: &Object) -> bool {
    let shapes = match (&a.shape, &b.shape) {
        (Shape::Group { children: x }, Shape::Group { children: y }) => same_all(x, y),
        (x, y) => x == y,
    };
    shapes
        && a.name == b.name
        && a.style == b.style
        && a.transform == b.transform
        && a.locked == b.locked
        && a.frame == b.frame
        && a.clone_of == b.clone_of
        && a.symbol == b.symbol
        && a.symmetry == b.symmetry
        && same_all(&a.contents, &b.contents)
}

fn same_all(a: &[Object], b: &[Object]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
}

/// Every object on `objects` and in them, mutably, outermost first.
fn walk_mut(objects: &mut [Object], f: &mut impl FnMut(&mut Object)) {
    for obj in objects {
        f(obj);
        if let Shape::Group { children } = &mut obj.shape {
            walk_mut(children, f);
        }
        walk_mut(&mut obj.contents, f);
    }
}

fn walk<'a>(objects: &'a [Object], f: &mut impl FnMut(&'a Object)) {
    for obj in objects {
        f(obj);
        if let Shape::Group { children } = &obj.shape {
            walk(children, f);
        }
        walk(&obj.contents, f);
    }
}

impl Document {
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.id == id)
    }

    /// The symbol's name when `obj` is an instance.
    pub fn symbol_name(&self, obj: &Object) -> Option<&str> {
        obj.symbol.and_then(|id| self.symbol(id)).map(|s| s.name.as_str())
    }

    /// How many instances of `id` there are, on every page.
    pub fn instance_count(&self, id: SymbolId) -> usize {
        let mut n = 0;
        for page in &self.pages {
            walk(&page.objects, &mut |o| n += usize::from(o.symbol == Some(id)));
        }
        n
    }

    pub fn can_make_symbol(&self, ids: &[ObjectId]) -> bool {
        self.active_page().objects.iter().any(|o| ids.contains(&o.id) && !o.locked)
    }

    /// New Symbol: the unlocked objects among `ids` become a symbol, and are replaced by
    /// one instance of it where they were. Returns the instance.
    pub fn new_symbol(&mut self, ids: &[ObjectId]) -> Option<ObjectId> {
        let group = self.wrap_in_group(ids, 1)?;
        let id = self.alloc_symbol_id();
        let mut taken: Vec<u32> = self.symbols.iter().filter_map(|s| s.name.strip_prefix("Symbol ")?.parse().ok()).collect();
        taken.sort_unstable();
        let number = (1..).find(|n| taken.binary_search(n).is_err()).expect("a free number");
        let obj = self.active_page_mut().object_mut(group)?;
        let center = obj.world_bounds().center().to_vec2();
        let Shape::Group { children } = &mut obj.shape else { return None };
        let mut objects = children.clone();
        for o in &mut objects {
            unlink(o);
            o.transform = Affine::translate(-center) * o.transform;
        }
        obj.transform = Affine::translate(center);
        obj.symbol = Some(id);
        *children = objects.clone();
        self.symbols.push(Symbol { id, name: format!("Symbol {number}"), objects });
        Some(group)
    }

    /// A new instance of `id`, centred on `at`, on the active layer. Returns it.
    pub fn insert_symbol(&mut self, id: SymbolId, at: Point) -> Option<ObjectId> {
        let objects = self.symbol(id)?.objects.clone();
        let members = self.copies_of(&objects);
        let center = Self::center_of(&objects);
        let obj = self.add_object(Shape::Group { children: members }, Style::default());
        let o = self.active_page_mut().object_mut(obj)?;
        o.symbol = Some(id);
        o.transform = Affine::translate(at - center);
        Some(obj)
    }

    fn center_of(objects: &[Object]) -> Point {
        objects.iter().map(Object::world_bounds).reduce(|a, b| a.union(b)).unwrap_or(Rect::ZERO).center()
    }

    fn copies_of(&mut self, objects: &[Object]) -> Vec<Object> {
        objects
            .iter()
            .map(|o| {
                let mut copy = o.clone();
                self.renumber(&mut copy);
                copy
            })
            .collect()
    }

    /// The unlocked instances among `ids` on the active page.
    pub fn instances(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && !o.locked && o.symbol.is_some()).map(|o| o.id).collect()
    }

    /// Revert to Objects: the unlocked instances among `ids` become plain groups. Returns them.
    pub fn revert_symbols(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let out = self.instances(ids);
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| out.contains(&o.id)) {
            obj.symbol = None;
        }
        out
    }

    pub fn rename_symbol(&mut self, id: SymbolId, name: &str) {
        if let Some(s) = self.symbols.iter_mut().find(|s| s.id == id) {
            name.clone_into(&mut s.name);
        }
    }

    /// Delete a symbol that has no instances. Returns whether it was deleted.
    pub fn delete_symbol(&mut self, id: SymbolId) -> bool {
        if self.instance_count(id) > 0 {
            return false;
        }
        let before = self.symbols.len();
        self.symbols.retain(|s| s.id != id);
        self.symbols.len() < before
    }

    /// Delete every symbol that has no instances. Returns how many went.
    pub fn delete_unused_symbols(&mut self) -> usize {
        let unused: Vec<SymbolId> = self.symbols.iter().map(|s| s.id).filter(|&id| self.instance_count(id) == 0).collect();
        unused.iter().filter(|&&id| self.delete_symbol(id)).count()
    }

    /// The symbols that `objects` (and anything in them) are instances of.
    pub fn symbols_used_by(&self, objects: &[Object]) -> Vec<Symbol> {
        let mut out: Vec<Symbol> = Vec::new();
        walk(objects, &mut |o| {
            if let Some(s) = o.symbol.and_then(|id| self.symbol(id))
                && !out.iter().any(|x| x.id == s.id)
            {
                out.push(s.clone());
            }
        });
        out
    }

    /// After pasting `ids` copied with the symbols `defs`: an instance whose symbol this
    /// document has (same id and name) shows it as it is now; any other symbol is added to
    /// the document under a new id. Instances of a symbol in neither become plain groups.
    pub fn adopt_symbols(&mut self, ids: &[ObjectId], defs: &[Symbol]) {
        let mut map: HashMap<SymbolId, SymbolId> = HashMap::new();
        for def in defs {
            match self.symbol(def.id) {
                Some(s) if s.name == def.name => {
                    map.insert(def.id, def.id);
                }
                _ => {
                    let id = self.alloc_symbol_id();
                    let objects = self.copies_of(&def.objects);
                    self.symbols.push(Symbol { id, name: def.name.clone(), objects });
                    map.insert(def.id, id);
                }
            }
        }
        let symbols = self.symbols.clone();
        let mut page = std::mem::take(&mut self.active_page_mut().objects);
        for obj in page.iter_mut().filter(|o| ids.contains(&o.id)) {
            walk_mut(std::slice::from_mut(obj), &mut |o| {
                o.symbol = o.symbol.and_then(|s| map.get(&s).copied());
                if let (Some(s), Shape::Group { children }) = (o.symbol.and_then(|s| symbols.iter().find(|x| x.id == s)), &mut o.shape) {
                    children.clone_from(&s.objects);
                    children.iter_mut().for_each(|c| self.renumber(c));
                }
            });
        }
        self.active_page_mut().objects = page;
        self.sync_symbols();
    }

    /// Keep every instance like its symbol. An instance whose members were changed passes
    /// the change to its symbol (the first one found wins); one whose members are all gone
    /// takes the symbol's objects back. Runs every frame; costs nothing without symbols.
    pub fn sync_symbols(&mut self) {
        if self.symbols.is_empty() {
            return;
        }
        let mut pages = std::mem::take(&mut self.pages);
        // Edits first.
        let mut edited: Vec<SymbolId> = Vec::new();
        for page in &pages {
            walk(&page.objects, &mut |o| {
                let Some(id) = o.symbol else { return };
                let Shape::Group { children } = &o.shape else { return };
                if children.is_empty() {
                    return;
                }
                if let Some(s) = self.symbols.iter_mut().find(|s| s.id == id)
                    && !same_all(children, &s.objects)
                    && !edited.contains(&id)
                {
                    let mut objects = children.clone();
                    objects.iter_mut().for_each(unlink);
                    s.objects = objects;
                    edited.push(id);
                }
            });
        }
        // Then every instance follows its symbol; members get their own ids.
        let symbols = std::mem::take(&mut self.symbols);
        for page in &mut pages {
            walk_mut(&mut page.objects, &mut |o| {
                let Some(id) = o.symbol else { return };
                let Shape::Group { children } = &mut o.shape else { return };
                match symbols.iter().find(|s| s.id == id) {
                    Some(s) if !same_all(children, &s.objects) => {
                        *children = s.objects.clone();
                        children.iter_mut().for_each(|c| self.renumber(c));
                    }
                    Some(_) => {}
                    None => o.symbol = None,
                }
            });
        }
        self.symbols = symbols;
        self.pages = pages;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;
    use kurbo::Vec2;

    fn rect(doc: &mut Document, r: Rect, c: Rgba) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, Style::filled(c))
    }

    fn fill_of(doc: &Document, id: ObjectId) -> Option<crate::Fill> {
        let obj = doc.active_page().object(id).unwrap();
        obj.leaves()[0].style.fill.clone()
    }

    #[test]
    fn new_symbol_replaces_the_objects_with_an_instance_in_place() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), Rgba::rgb(255, 0, 0));
        let b = rect(&mut doc, Rect::new(30.0, 0.0, 40.0, 10.0), Rgba::BLACK);
        assert!(doc.can_make_symbol(&[a]));
        let inst = doc.new_symbol(&[a, b]).unwrap();
        let page = doc.active_page();
        assert_eq!(page.objects.len(), 1);
        let obj = page.object(inst).unwrap();
        assert_eq!(obj.label(), "Symbol");
        assert_eq!(doc.symbol_name(obj), Some("Symbol 1"));
        assert_eq!(obj.world_bounds(), Rect::new(0.0, 0.0, 40.0, 20.0), "where they were");
        let sym = &doc.symbols[0];
        assert_eq!(Document::center_of(&sym.objects), Point::ZERO, "centred on the symbol's origin");
        assert_eq!(doc.instance_count(sym.id), 1);

        assert!(doc.new_symbol(&[]).is_none(), "nothing selected");
        let c = rect(&mut doc, Rect::new(0.0, 0.0, 1.0, 1.0), Rgba::BLACK);
        let other = doc.new_symbol(&[c]).unwrap();
        assert_eq!(doc.symbol_name(doc.active_page().object(other).unwrap()), Some("Symbol 2"));
    }

    #[test]
    fn changing_one_instance_changes_the_symbol_and_every_instance() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), Rgba::rgb(255, 0, 0));
        let first = doc.new_symbol(&[a]).unwrap();
        let sym = doc.symbols[0].id;
        let second = doc.insert_symbol(sym, Point::new(200.0, 200.0)).unwrap();
        assert_eq!(doc.active_page().object(second).unwrap().world_bounds(), Rect::new(190.0, 190.0, 210.0, 210.0));
        let dup = doc.duplicate_objects(&[second], Vec2::new(50.0, 0.0))[0];
        doc.sync_symbols();
        assert_eq!(doc.instance_count(sym), 3);
        assert_eq!(doc.symbols[0].objects[0].style.fill, Some(crate::Fill::Solid(Rgba::rgb(255, 0, 0))), "a copy is no edit");

        let blue = Rgba::rgb(0, 0, 255);
        doc.restyle_objects(&[second], |s| s.fill = Some(crate::Fill::Solid(blue)));
        doc.sync_symbols();
        for id in [first, second, dup] {
            assert_eq!(fill_of(&doc, id), Some(crate::Fill::Solid(blue)));
        }

        // Members taken out (Edit Symbol), edited and put back.
        let members = doc.lift_members(first);
        doc.active_page_mut().object_mut(first).unwrap().symbol = None;
        doc.sync_symbols();
        assert_eq!(fill_of(&doc, second), Some(crate::Fill::Solid(blue)), "others wait while it is edited");
        doc.move_objects(&members, Vec2::new(5.0, 0.0));
        doc.sink_members(first, &members);
        doc.active_page_mut().object_mut(first).unwrap().symbol = Some(sym);
        doc.sync_symbols();
        assert_eq!(doc.active_page().object(second).unwrap().world_bounds(), Rect::new(195.0, 190.0, 215.0, 210.0), "moved in every instance");

        // Revert and delete.
        assert!(!doc.delete_symbol(sym), "still in use");
        doc.revert_symbols(&[first, second, dup]);
        assert_eq!(doc.active_page().object(first).unwrap().label(), "Group of 1 objects");
        assert!(doc.delete_symbol(sym));
        assert!(doc.symbols.is_empty());
    }

    #[test]
    fn an_instance_emptied_takes_the_symbol_back_and_nested_symbols_are_unlinked() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), Rgba::BLACK);
        let inner = doc.new_symbol(&[a]).unwrap();
        let outer = doc.new_symbol(&[inner]).unwrap();
        assert!(doc.symbols[1].objects.iter().all(|o| o.symbol.is_none()), "no symbol inside a symbol");
        assert_eq!(doc.instance_count(doc.symbols[0].id), 0);
        assert_eq!(doc.delete_unused_symbols(), 1);

        let Shape::Group { children } = &mut doc.active_page_mut().object_mut(outer).unwrap().shape else { panic!() };
        children.clear();
        doc.sync_symbols();
        assert_eq!(doc.active_page().object(outer).unwrap().world_bounds(), Rect::new(0.0, 0.0, 20.0, 20.0));
    }

    #[test]
    fn pasting_into_another_document_brings_the_symbol_along() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), Rgba::BLACK);
        let inst = doc.new_symbol(&[a]).unwrap();
        doc.rename_symbol(doc.symbols[0].id, "Logo");
        let copied = doc.copy_objects(&[inst]);
        let defs = doc.symbols_used_by(&copied);
        assert_eq!(defs.len(), 1);

        // Same document: still an instance of the same symbol.
        let pasted = doc.paste_objects(&copied, Vec2::ZERO);
        doc.adopt_symbols(&pasted, &defs);
        assert_eq!(doc.symbols.len(), 1);
        assert_eq!(doc.instance_count(defs[0].id), 2);

        // Another document with a different symbol under the same id.
        let mut other = Document::default();
        let b = rect(&mut other, Rect::new(0.0, 0.0, 5.0, 5.0), Rgba::BLACK);
        let theirs = other.new_symbol(&[b]).unwrap();
        // Make the two collide.
        other.symbols[0].id = defs[0].id;
        other.active_page_mut().object_mut(theirs).unwrap().symbol = Some(defs[0].id);
        let pasted = other.paste_objects(&copied, Vec2::ZERO);
        other.adopt_symbols(&pasted, &defs);
        assert_eq!(other.symbols.len(), 2, "added, not merged with theirs");
        let obj = other.active_page().object(pasted[0]).unwrap();
        assert_eq!(other.symbol_name(obj), Some("Logo"));
        assert_ne!(obj.symbol, Some(defs[0].id));
        assert_eq!(obj.world_bounds(), Rect::new(0.0, 0.0, 20.0, 20.0));

        let json = serde_json::to_string(&other).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back.symbols, other.symbols, "saved");
    }
}

//! Editing operations on the objects of the active page.

use kurbo::{Affine, Rect, Vec2};

use crate::document::{Document, Object, ObjectId, Shape, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Right,
    Top,
    Bottom,
    CenterHorizontal,
    CenterVertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order {
    ToFront,
    ToBack,
    Forward,
    Backward,
    /// Turn the stacking order of the objects upside down, in the places they take.
    Reverse,
}

/// Spread objects evenly between the two outermost ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Distribute {
    /// Equal distances between left edges.
    Left,
    CenterHorizontal,
    Right,
    /// Equal gaps between objects, left to right.
    SpacingHorizontal,
    Top,
    CenterVertical,
    Bottom,
    SpacingVertical,
}

impl Distribute {
    pub const ALL: [Distribute; 8] = [
        Distribute::Left,
        Distribute::CenterHorizontal,
        Distribute::Right,
        Distribute::SpacingHorizontal,
        Distribute::Top,
        Distribute::CenterVertical,
        Distribute::Bottom,
        Distribute::SpacingVertical,
    ];

    fn horizontal(self) -> bool {
        matches!(self, Distribute::Left | Distribute::CenterHorizontal | Distribute::Right | Distribute::SpacingHorizontal)
    }
}

/// The start and end of `r` along x (`horizontal`) or y.
fn lo_hi(r: &Rect, horizontal: bool) -> (f64, f64) {
    if horizontal { (r.x0, r.x1) } else { (r.y0, r.y1) }
}

impl Document {
    /// Pre-multiply `transform` (page space) onto each unlocked object.
    pub fn transform_objects(&mut self, ids: &[ObjectId], transform: Affine) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            obj.transform = transform * obj.transform;
        }
    }

    pub fn move_objects(&mut self, ids: &[ObjectId], delta: Vec2) {
        self.transform_objects(ids, Affine::translate(delta));
    }

    /// Align `ids` to `target` (page space).
    pub fn align_objects(&mut self, ids: &[ObjectId], target: Rect, align: Align) {
        let page = self.active_page_mut();
        for obj in page.objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            let b = obj.world_bounds();
            let delta = match align {
                Align::Left => Vec2::new(target.x0 - b.x0, 0.0),
                Align::Right => Vec2::new(target.x1 - b.x1, 0.0),
                Align::Top => Vec2::new(0.0, target.y0 - b.y0),
                Align::Bottom => Vec2::new(0.0, target.y1 - b.y1),
                Align::CenterHorizontal => Vec2::new(target.center().x - b.center().x, 0.0),
                Align::CenterVertical => Vec2::new(0.0, target.center().y - b.center().y),
            };
            obj.transform = Affine::translate(delta) * obj.transform;
        }
    }

    /// Restack `ids`, keeping their relative order.
    pub fn reorder_objects(&mut self, ids: &[ObjectId], order: Order) {
        let objects = &mut self.active_page_mut().objects;
        let selected = |o: &Object| ids.contains(&o.id);
        match order {
            Order::ToFront | Order::ToBack => {
                let (picked, rest): (Vec<_>, Vec<_>) = objects.drain(..).partition(|o| selected(o));
                *objects = if order == Order::ToFront {
                    rest.into_iter().chain(picked).collect()
                } else {
                    picked.into_iter().chain(rest).collect()
                };
            }
            Order::Forward => {
                for i in (0..objects.len().saturating_sub(1)).rev() {
                    if selected(&objects[i]) && !selected(&objects[i + 1]) {
                        objects.swap(i, i + 1);
                    }
                }
            }
            Order::Backward => {
                for i in 1..objects.len() {
                    if selected(&objects[i]) && !selected(&objects[i - 1]) {
                        objects.swap(i, i - 1);
                    }
                }
            }
            Order::Reverse => {
                let slots: Vec<usize> = (0..objects.len()).filter(|i| selected(&objects[*i])).collect();
                for k in 0..slots.len() / 2 {
                    objects.swap(slots[k], slots[slots.len() - 1 - k]);
                }
            }
        }
    }

    /// Spread the unlocked objects among `ids` (at least three) so their edges, centres or
    /// the gaps between them are evenly spaced. The two outermost objects stay put.
    pub fn distribute_objects(&mut self, ids: &[ObjectId], how: Distribute) {
        let page = self.active_page();
        let mut items: Vec<(ObjectId, Rect)> = page
            .objects
            .iter()
            .filter(|o| ids.contains(&o.id) && !o.locked)
            .map(|o| (o.id, o.world_bounds()))
            .collect();
        if items.len() < 3 {
            return;
        }
        let h = how.horizontal();
        let key = |r: &Rect| {
            let (a, b) = lo_hi(r, h);
            match how {
                Distribute::Left | Distribute::Top => a,
                Distribute::Right | Distribute::Bottom => b,
                _ => (a + b) / 2.0,
            }
        };
        items.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
        let n = items.len();
        let mut moves = Vec::new();
        if matches!(how, Distribute::SpacingHorizontal | Distribute::SpacingVertical) {
            let (start, _) = lo_hi(&items[0].1, h);
            let (_, end) = lo_hi(&items[n - 1].1, h);
            let total: f64 = items.iter().map(|(_, r)| lo_hi(r, h).1 - lo_hi(r, h).0).sum();
            let gap = (end - start - total) / (n - 1) as f64;
            let mut at = start;
            for (id, r) in &items {
                let (a, b) = lo_hi(r, h);
                moves.push((*id, at - a));
                at += b - a + gap;
            }
        } else {
            let (first, last) = (key(&items[0].1), key(&items[n - 1].1));
            for (i, (id, r)) in items.iter().enumerate() {
                let want = first + (last - first) * i as f64 / (n - 1) as f64;
                moves.push((*id, want - key(r)));
            }
        }
        for (id, d) in moves {
            let delta = if h { Vec2::new(d, 0.0) } else { Vec2::new(0.0, d) };
            self.move_objects(&[id], delta);
        }
    }

    /// Delete the unlocked objects among `ids`.
    pub fn delete_objects(&mut self, ids: &[ObjectId]) {
        self.active_page_mut().objects.retain(|o| o.locked || !ids.contains(&o.id));
    }

    /// Change the style of the unlocked objects among `ids` (every member of a group).
    pub fn restyle_objects(&mut self, ids: &[ObjectId], mut f: impl FnMut(&mut Style)) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            obj.restyle(&mut f);
        }
    }

    /// Like [`Document::restyle_objects`], also passing each object's local frame.
    pub fn restyle_objects_framed(&mut self, ids: &[ObjectId], mut f: impl FnMut(&mut Style, kurbo::Rect)) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            obj.restyle_framed(&mut f);
        }
    }

    pub fn set_locked(&mut self, ids: &[ObjectId], locked: bool) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            obj.locked = locked;
        }
    }

    /// Group the unlocked objects among `ids` (at least two). The group takes the place of
    /// the topmost one in the stack. Returns the group's id.
    pub fn group_objects(&mut self, ids: &[ObjectId]) -> Option<ObjectId> {
        let page = self.active_page();
        let picked = |o: &Object| ids.contains(&o.id) && !o.locked;
        if page.objects.iter().filter(|o| picked(o)).count() < 2 {
            return None;
        }
        let top = page.objects.iter().rposition(picked)?;
        let below_top = page.objects[..top].iter().filter(|o| picked(o)).count();
        let id = self.alloc_object_id();
        let objects = &mut self.active_page_mut().objects;
        let (children, rest): (Vec<_>, Vec<_>) = objects.drain(..).partition(picked);
        *objects = rest;
        let group = Object {
            id,
            name: None,
            shape: Shape::Group { children },
            style: Style::default(),
            transform: Affine::IDENTITY,
            locked: false,
        };
        objects.insert(top - below_top, group);
        Some(id)
    }

    /// Replace each unlocked group among `ids` by its members, in place. Returns the ids of
    /// the members (to select them) and of any non-group objects in `ids`.
    pub fn ungroup_objects(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let objects = &mut self.active_page_mut().objects;
        let mut out = Vec::new();
        let mut i = 0;
        while i < objects.len() {
            let obj = &objects[i];
            if !ids.contains(&obj.id) {
                i += 1;
                continue;
            }
            if obj.locked || !matches!(obj.shape, Shape::Group { .. }) {
                out.push(obj.id);
                i += 1;
                continue;
            }
            let group = objects.remove(i);
            let Shape::Group { children } = group.shape else { unreachable!() };
            let n = children.len();
            for (k, mut child) in children.into_iter().enumerate() {
                child.transform = group.transform * child.transform;
                out.push(child.id);
                objects.insert(i + k, child);
            }
            i += n;
        }
        out
    }

    /// Copies of `ids`, not yet in the document.
    pub fn copy_objects(&self, ids: &[ObjectId]) -> Vec<Object> {
        self.active_page()
            .objects
            .iter()
            .filter(|o| ids.contains(&o.id))
            .cloned()
            .collect()
    }

    /// Add copies of `objects` on top with fresh ids, moved by `offset`. Returns the new ids.
    pub fn paste_objects(&mut self, objects: &[Object], offset: Vec2) -> Vec<ObjectId> {
        let mut new_ids = Vec::with_capacity(objects.len());
        for obj in objects {
            let mut copy = obj.clone();
            self.renumber(&mut copy);
            copy.transform = Affine::translate(offset) * copy.transform;
            new_ids.push(copy.id);
            self.active_page_mut().objects.push(copy);
        }
        new_ids
    }

    /// Step and Repeat: `copies` copies of the objects among `ids`, the n-th moved by
    /// n × `step`. Returns the new objects, first copy first.
    pub fn step_and_repeat(&mut self, ids: &[ObjectId], copies: u32, step: Vec2) -> Vec<ObjectId> {
        let originals = self.copy_objects(ids);
        (1..=copies).flat_map(|n| self.paste_objects(&originals, step * f64::from(n))).collect()
    }

    pub fn duplicate_objects(&mut self, ids: &[ObjectId], offset: Vec2) -> Vec<ObjectId> {
        let copies = self.copy_objects(ids);
        self.paste_objects(&copies, offset)
    }

    /// Replace analytic shapes with editable curves (transform kept). Groups and locked
    /// objects are left alone.
    pub fn convert_to_curves(&mut self, ids: &[ObjectId]) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            obj.convert_to_curves();
        }
    }

    /// All object ids on the active page, bottom first.
    pub fn all_object_ids(&self) -> Vec<ObjectId> {
        self.active_page().objects.iter().map(|o| o.id).collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn step_and_repeat_makes_evenly_spaced_copies() {
        let mut doc = Document::default();
        let a = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 10.0), corner_radius: 0.0 }, Style::new_object());
        let made = doc.step_and_repeat(&[a], 3, Vec2::new(15.0, 5.0));
        assert_eq!(made.len(), 3);
        let page = doc.active_page();
        let x: Vec<f64> = made.iter().map(|id| page.object(*id).unwrap().world_bounds().x0).collect();
        assert_eq!(x, vec![15.0, 30.0, 45.0]);
        assert_eq!(page.object(made[2]).unwrap().world_bounds().y0, 15.0);
        assert!(doc.step_and_repeat(&[a], 0, Vec2::ZERO).is_empty());
    }

    #[test]
    fn distribute_spreads_edges_centres_and_gaps() {
        use super::Distribute;
        let mut doc = Document::default();
        let style = Style::default();
        let rect = |x0: f64, w: f64| Shape::Rectangle { rect: Rect::new(x0, 0.0, x0 + w, 10.0), corner_radius: 0.0 };
        let a = doc.add_object(rect(0.0, 10.0), style.clone());
        let b = doc.add_object(rect(15.0, 30.0), style.clone());
        let c = doc.add_object(rect(90.0, 10.0), style.clone());
        let ids = [c, a, b];
        let x0 = |doc: &Document, id| doc.active_page().object(id).unwrap().world_bounds().x0;

        doc.distribute_objects(&ids, Distribute::Left);
        assert_eq!((x0(&doc, a), x0(&doc, b), x0(&doc, c)), (0.0, 45.0, 90.0));

        doc.distribute_objects(&ids, Distribute::SpacingHorizontal);
        // Widths 10 + 30 + 10 over 100: two gaps of 25.
        assert_eq!((x0(&doc, a), x0(&doc, b), x0(&doc, c)), (0.0, 35.0, 90.0));

        doc.distribute_objects(&ids, Distribute::CenterHorizontal);
        let cx = |doc: &Document, id| doc.active_page().object(id).unwrap().world_bounds().center().x;
        assert_eq!(cx(&doc, b), 50.0);

        // Two objects: nothing to spread.
        let before = doc.clone();
        doc.distribute_objects(&[a, b], Distribute::Left);
        assert_eq!(doc, before);
    }

    #[test]
    fn reverse_order_flips_the_selected_objects_in_their_places() {
        use super::Order;
        let mut doc = Document::default();
        let ids: Vec<_> = (0..4).map(|i| doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 1.0, f64::from(i) + 1.0), corner_radius: 0.0 }, Style::default())).collect();
        doc.reorder_objects(&[ids[0], ids[1], ids[3]], Order::Reverse);
        let order: Vec<_> = doc.active_page().objects.iter().map(|o| o.id).collect();
        assert_eq!(order, vec![ids[3], ids[1], ids[2], ids[0]]);
    }

    use super::*;
    use crate::document::{Rgba, Style};

    fn doc_with(rects: &[Rect]) -> (Document, Vec<ObjectId>) {
        let mut doc = Document::default();
        let style = Style::filled(Rgba::BLACK);
        let ids = rects
            .iter()
            .map(|r| doc.add_object(Shape::Rectangle { rect: *r, corner_radius: 0.0 }, style.clone()))
            .collect();
        (doc, ids)
    }

    fn order(doc: &Document) -> Vec<ObjectId> {
        doc.all_object_ids()
    }

    #[test]
    fn align_left_and_center() {
        let (mut doc, ids) = doc_with(&[Rect::new(10.0, 0.0, 20.0, 10.0), Rect::new(50.0, 0.0, 70.0, 10.0)]);
        doc.align_objects(&ids, Rect::new(0.0, 0.0, 100.0, 100.0), Align::Left);
        let page = doc.active_page();
        assert_eq!(page.object(ids[0]).unwrap().world_bounds().x0, 0.0);
        assert_eq!(page.object(ids[1]).unwrap().world_bounds().x0, 0.0);

        doc.align_objects(&ids[..1], Rect::new(0.0, 0.0, 100.0, 100.0), Align::CenterVertical);
        assert_eq!(doc.active_page().object(ids[0]).unwrap().world_bounds().center().y, 50.0);
    }

    #[test]
    fn reorder_keeps_relative_order() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let (mut doc, ids) = doc_with(&[r, r, r, r]);
        doc.reorder_objects(&[ids[0], ids[1]], Order::ToFront);
        assert_eq!(order(&doc), vec![ids[2], ids[3], ids[0], ids[1]]);
        doc.reorder_objects(&[ids[0], ids[1]], Order::ToBack);
        assert_eq!(order(&doc), vec![ids[0], ids[1], ids[2], ids[3]]);
        doc.reorder_objects(&[ids[1]], Order::Forward);
        assert_eq!(order(&doc), vec![ids[0], ids[2], ids[1], ids[3]]);
        doc.reorder_objects(&[ids[1]], Order::Backward);
        assert_eq!(order(&doc), vec![ids[0], ids[1], ids[2], ids[3]]);
        doc.reorder_objects(&[ids[3]], Order::Forward); // already on top
        assert_eq!(order(&doc), vec![ids[0], ids[1], ids[2], ids[3]]);
    }

    #[test]
    fn duplicate_gives_fresh_ids_on_top_with_offset() {
        let (mut doc, ids) = doc_with(&[Rect::new(0.0, 0.0, 10.0, 10.0)]);
        let copies = doc.duplicate_objects(&ids, Vec2::new(5.0, 5.0));
        assert_eq!(copies.len(), 1);
        assert_ne!(copies[0], ids[0]);
        assert_eq!(order(&doc).last(), Some(&copies[0]));
        assert_eq!(doc.active_page().object(copies[0]).unwrap().world_bounds().x0, 5.0);
    }

    #[test]
    fn convert_to_curves_keeps_geometry() {
        let (mut doc, ids) = doc_with(&[Rect::new(0.0, 0.0, 10.0, 10.0)]);
        doc.transform_objects(&ids, Affine::rotate(0.3));
        let before = doc.active_page().object(ids[0]).unwrap().world_bounds();
        doc.convert_to_curves(&ids);
        let obj = doc.active_page().object(ids[0]).unwrap();
        assert!(matches!(obj.shape, Shape::Curve { .. }));
        let after = obj.world_bounds();
        assert!((before.x0 - after.x0).abs() < 1e-9 && (before.y1 - after.y1).abs() < 1e-9);
    }

    #[test]
    fn convert_to_curves_reaches_into_groups_and_skips_locked_objects() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let (mut doc, ids) = doc_with(&[r, r + Vec2::new(20.0, 0.0), r + Vec2::new(40.0, 0.0)]);
        let group = doc.group_objects(&ids[..2]).unwrap();
        doc.set_locked(&[ids[2]], true);
        assert!(doc.active_page().object(group).unwrap().can_convert_to_curves());
        doc.convert_to_curves(&[group, ids[2]]);
        let page = doc.active_page();
        let Shape::Group { children } = &page.object(group).unwrap().shape else { panic!("still a group") };
        assert!(children.iter().all(|c| matches!(c.shape, Shape::Curve { .. })));
        assert!(!page.object(group).unwrap().can_convert_to_curves());
        assert!(matches!(page.object(ids[2]).unwrap().shape, Shape::Rectangle { .. }), "locked: unchanged");
    }

    #[test]
    fn group_and_ungroup_keep_stack_position_and_geometry() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let (mut doc, ids) = doc_with(&[r, r + Vec2::new(20.0, 0.0), r, r + Vec2::new(40.0, 0.0)]);
        let group = doc.group_objects(&[ids[1], ids[3]]).expect("grouped");
        assert_eq!(order(&doc), vec![ids[0], ids[2], group]);
        assert_eq!(doc.active_page().object(group).unwrap().world_bounds(), Rect::new(20.0, 0.0, 50.0, 10.0));

        doc.transform_objects(&[group], Affine::translate((5.0, 7.0)));
        let members = doc.ungroup_objects(&[group]);
        assert_eq!(members, vec![ids[1], ids[3]]);
        assert_eq!(order(&doc), vec![ids[0], ids[2], ids[1], ids[3]]);
        let b = doc.active_page().object(ids[3]).unwrap().world_bounds();
        assert_eq!((b.x0, b.y0), (45.0, 7.0));
    }

    #[test]
    fn group_needs_two_unlocked_objects() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let (mut doc, ids) = doc_with(&[r, r]);
        assert_eq!(doc.group_objects(&ids[..1]), None);
        doc.set_locked(&ids[..1], true);
        assert_eq!(doc.group_objects(&ids), None);
    }

    #[test]
    fn locked_objects_do_not_move_restyle_or_delete() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let (mut doc, ids) = doc_with(&[r, r]);
        doc.set_locked(&ids[..1], true);
        doc.move_objects(&ids, Vec2::new(5.0, 0.0));
        doc.restyle_objects(&ids, |s| s.fill = None);
        let page = doc.active_page();
        assert_eq!(page.object(ids[0]).unwrap().world_bounds().x0, 0.0);
        assert!(page.object(ids[0]).unwrap().style.fill.is_some());
        assert_eq!(page.object(ids[1]).unwrap().world_bounds().x0, 5.0);
        assert!(page.object(ids[1]).unwrap().style.fill.is_none());
        doc.delete_objects(&ids);
        assert_eq!(order(&doc), vec![ids[0]]);
    }

    #[test]
    fn restyle_reaches_group_members_and_paste_renumbers_them() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let (mut doc, ids) = doc_with(&[r, r]);
        let group = doc.group_objects(&ids).unwrap();
        doc.restyle_objects(&[group], |s| s.fill = Some(crate::Fill::Solid(crate::Rgba::WHITE)));
        let obj = doc.active_page().object(group).unwrap().clone();
        assert!(obj.leaves().iter().all(|l| l.style.fill == Some(crate::Fill::Solid(crate::Rgba::WHITE))));
        let copy = doc.duplicate_objects(&[group], Vec2::ZERO)[0];
        let copied = doc.active_page().object(copy).unwrap().all_ids();
        assert!(copied.iter().all(|id| !obj.all_ids().contains(id)));
    }

    #[test]
    fn delete_removes_only_given() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let (mut doc, ids) = doc_with(&[r, r]);
        doc.delete_objects(&ids[..1]);
        assert_eq!(order(&doc), vec![ids[1]]);
    }
}

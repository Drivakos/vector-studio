//! Editing operations on the objects of the active page.

use kurbo::{Affine, Rect, Vec2};

use crate::document::{Document, Object, ObjectId, Shape};

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
}

impl Document {
    /// Pre-multiply `transform` (page space) onto each object.
    pub fn transform_objects(&mut self, ids: &[ObjectId], transform: Affine) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            obj.transform = transform * obj.transform;
        }
    }

    pub fn move_objects(&mut self, ids: &[ObjectId], delta: Vec2) {
        self.transform_objects(ids, Affine::translate(delta));
    }

    /// Align `ids` to `target` (page space).
    pub fn align_objects(&mut self, ids: &[ObjectId], target: Rect, align: Align) {
        let page = self.active_page_mut();
        for obj in page.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
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
        }
    }

    pub fn delete_objects(&mut self, ids: &[ObjectId]) {
        self.active_page_mut().objects.retain(|o| !ids.contains(&o.id));
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
            copy.id = self.alloc_object_id();
            copy.transform = Affine::translate(offset) * copy.transform;
            new_ids.push(copy.id);
            self.active_page_mut().objects.push(copy);
        }
        new_ids
    }

    pub fn duplicate_objects(&mut self, ids: &[ObjectId], offset: Vec2) -> Vec<ObjectId> {
        let copies = self.copy_objects(ids);
        self.paste_objects(&copies, offset)
    }

    /// Replace analytic shapes with editable curves (transform kept).
    pub fn convert_to_curves(&mut self, ids: &[ObjectId]) {
        for obj in self.active_page_mut().objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            if !matches!(obj.shape, Shape::Curve { .. }) {
                obj.shape = Shape::Curve { path: obj.shape.to_path() };
            }
        }
    }

    /// All object ids on the active page, bottom first.
    pub fn all_object_ids(&self) -> Vec<ObjectId> {
        self.active_page().objects.iter().map(|o| o.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Rgba, Style};

    fn doc_with(rects: &[Rect]) -> (Document, Vec<ObjectId>) {
        let mut doc = Document::default();
        let style = Style { fill: Some(Rgba::BLACK), outline: None };
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
    fn delete_removes_only_given() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let (mut doc, ids) = doc_with(&[r, r]);
        doc.delete_objects(&ids[..1]);
        assert_eq!(order(&doc), vec![ids[1]]);
    }
}

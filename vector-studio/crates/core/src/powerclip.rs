//! PowerClip: objects placed inside a closed shape (the container) show only within its
//! outline. The contents live in [`Object::contents`], in the container's local space.

use crate::document::{Document, Object, ObjectId, Shape};

impl Object {
    /// Whether this object can hold PowerClip contents: an unlocked closed shape or text,
    /// not a group or bitmap.
    pub fn can_be_container(&self) -> bool {
        !self.locked && !matches!(self.shape, Shape::Group { .. } | Shape::Image(_)) && self.shape.is_closed()
    }
}

impl Document {
    /// Whether Place Inside Frame applies: some unlocked object is selected.
    pub fn can_place_inside(&self, ids: &[ObjectId]) -> bool {
        self.active_page().objects.iter().any(|o| ids.contains(&o.id) && !o.locked)
    }

    /// Put the unlocked objects among `ids` (not the container itself) inside `container`,
    /// on top of any contents it has, where they are on the page. Returns whether
    /// anything was placed.
    pub fn place_inside(&mut self, ids: &[ObjectId], container: ObjectId) -> bool {
        let page = self.active_page_mut();
        let Some(frame) = page.object(container).filter(|o| o.can_be_container()) else { return false };
        let to_local = frame.transform.inverse();
        let (mut placed, rest): (Vec<Object>, Vec<Object>) =
            std::mem::take(&mut page.objects).into_iter().partition(|o| ids.contains(&o.id) && !o.locked && o.id != container);
        page.objects = rest;
        if placed.is_empty() {
            return false;
        }
        for obj in &mut placed {
            obj.transform = to_local * obj.transform;
        }
        page.object_mut(container).expect("container is still on the page").contents.append(&mut placed);
        true
    }

    /// Whether Create Empty Frame applies: a selected object can be a container and is
    /// not a frame yet.
    pub fn can_create_empty_frame(&self, ids: &[ObjectId]) -> bool {
        self.active_page().objects.iter().any(|o| ids.contains(&o.id) && o.can_be_container() && !o.frame)
    }

    /// Create Empty Frame: make the selected objects that can hold contents into PowerClip
    /// frames, which objects dropped on them go into. Returns them.
    pub fn create_empty_frame(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out = Vec::new();
        for obj in &mut self.active_page_mut().objects {
            if ids.contains(&obj.id) && obj.can_be_container() && !obj.frame {
                obj.frame = true;
                out.push(obj.id);
            }
        }
        out
    }

    /// Whether Extract Contents applies: an unlocked selected object has contents.
    pub fn can_extract_contents(&self, ids: &[ObjectId]) -> bool {
        self.active_page().objects.iter().any(|o| ids.contains(&o.id) && !o.locked && !o.contents.is_empty())
    }

    /// Take the contents out of the unlocked containers among `ids` and put them on the
    /// page just above their container, where they showed. Returns their ids.
    pub fn extract_contents(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let page = self.active_page_mut();
        let mut out = Vec::new();
        let mut i = 0;
        while i < page.objects.len() {
            let obj = &mut page.objects[i];
            if !ids.contains(&obj.id) || obj.locked || obj.contents.is_empty() {
                i += 1;
                continue;
            }
            let (transform, layer) = (obj.transform, obj.layer);
            let contents = std::mem::take(&mut obj.contents);
            let n = contents.len();
            for (k, mut c) in contents.into_iter().enumerate() {
                c.transform = transform * c.transform;
                c.layer = layer;
                out.push(c.id);
                page.objects.insert(i + 1 + k, c);
            }
            i += 1 + n;
        }
        out
    }
}

impl crate::Page {
    /// The topmost unlocked PowerClip frame (see [`Object::frame`]) whose outline holds
    /// `p`, other than those in `exclude`.
    pub fn frame_at(&self, p: kurbo::Point, exclude: &[ObjectId]) -> Option<ObjectId> {
        use kurbo::Shape as _;
        self.objects
            .iter()
            .rev()
            .find(|o| o.frame && !o.locked && !exclude.contains(&o.id) && o.world_path().winding(p) != 0)
            .map(|o| o.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;
    use crate::Rgba;
    use kurbo::{Point, Rect, Vec2};

    fn rect(doc: &mut Document, r: Rect, color: Rgba) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, Style::filled(color))
    }

    #[test]
    fn placed_objects_show_only_inside_and_move_with_the_container() {
        let mut doc = Document::default();
        let frame = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0), Rgba::WHITE);
        let wide = rect(&mut doc, Rect::new(50.0, 40.0, 200.0, 60.0), Rgba::rgb(255, 0, 0));
        assert!(doc.place_inside(&[wide], frame));
        let page = doc.active_page();
        assert_eq!(page.objects.len(), 1);
        let container = page.object(frame).unwrap();
        assert_eq!(container.label(), "PowerClip Rectangle");
        assert_eq!(container.world_bounds(), Rect::new(0.0, 0.0, 100.0, 100.0), "contents do not grow the bounds");
        assert_eq!(page.hit_test(Point::new(150.0, 50.0), 0.0), None, "clipped away");
        assert_eq!(page.hit_test(Point::new(75.0, 50.0), 0.0), Some(frame));
        let red = page.paint_at(Point::new(75.0, 50.0), 0.0).unwrap();
        assert_eq!(red.color, Rgba::rgb(255, 0, 0), "contents draw over the container's fill");

        doc.move_objects(&[frame], Vec2::new(10.0, 0.0));
        let leaves = doc.active_page().object(frame).unwrap().leaves().len();
        assert_eq!(leaves, 2, "fill, then contents (no outline)");
        let out = doc.extract_contents(&[frame]);
        assert_eq!(out, vec![wide]);
        let page = doc.active_page();
        assert_eq!(page.objects.iter().map(|o| o.id).collect::<Vec<_>>(), vec![frame, wide], "just above the container");
        assert_eq!(page.object(wide).unwrap().world_bounds(), Rect::new(60.0, 40.0, 210.0, 60.0), "moved with it");
        assert!(page.object(frame).unwrap().contents.is_empty());
    }

    #[test]
    fn only_closed_unlocked_shapes_take_contents_and_copies_get_fresh_ids() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0), Rgba::BLACK);
        let line = doc.add_object(Shape::Curve { path: kurbo::Shape::to_path(&kurbo::Line::new((0.0, 0.0), (5.0, 5.0)), 0.1) }, Style::default());
        assert!(!doc.place_inside(&[a], line), "an open curve is no container");
        let frame = rect(&mut doc, Rect::new(0.0, 0.0, 20.0, 20.0), Rgba::WHITE);
        assert!(!doc.place_inside(&[frame], frame), "not inside itself");
        assert!(doc.place_inside(&[a], frame));
        let copies = doc.duplicate_objects(&[frame], Vec2::ZERO);
        let copy = doc.active_page().object(copies[0]).unwrap();
        assert_ne!(copy.contents[0].id, a, "contents are renumbered with their container");
        doc.set_locked(&[frame], true);
        assert!(!doc.can_extract_contents(&[frame]));
        assert!(doc.extract_contents(&[frame]).is_empty());
    }

    #[test]
    fn an_empty_frame_takes_objects_dropped_on_it() {
        let mut doc = Document::default();
        let frame = rect(&mut doc, Rect::new(0.0, 0.0, 100.0, 100.0), Rgba::WHITE);
        let dot = rect(&mut doc, Rect::new(40.0, 40.0, 60.0, 60.0), Rgba::BLACK);
        assert!(doc.can_create_empty_frame(&[frame]));
        assert_eq!(doc.create_empty_frame(&[frame, dot]), vec![frame, dot]);
        assert!(!doc.can_create_empty_frame(&[frame]), "already a frame");
        assert_eq!(doc.active_page().object(frame).unwrap().label(), "Empty PowerClip Rectangle");
        assert_eq!(doc.active_page().frame_at(Point::new(50.0, 50.0), &[]), Some(dot), "the topmost frame");
        assert_eq!(doc.active_page().frame_at(Point::new(50.0, 50.0), &[dot]), Some(frame));
        assert_eq!(doc.active_page().frame_at(Point::new(150.0, 50.0), &[]), None);
        doc.active_page_mut().object_mut(dot).unwrap().frame = false;
        assert!(doc.place_inside(&[dot], frame));
        assert_eq!(doc.active_page().object(frame).unwrap().label(), "PowerClip Rectangle");
        doc.extract_contents(&[frame]);
        assert!(doc.active_page().object(frame).unwrap().frame, "a frame stays a frame when emptied");
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert!(back.active_page().object(frame).unwrap().frame, "saved");
    }
}

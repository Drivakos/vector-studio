//! Clones: copies that follow their master's shape, fill and outline. A clone keeps its
//! own position, size and rotation; editing its shape or style itself breaks the link.

use std::collections::HashMap;

use kurbo::Vec2;

use crate::document::{Document, Object, ObjectId, Shape, Style};

/// What each clone last took from its master, to tell the master's edits (passed on)
/// from the clone's own (which break the link). Not saved, and not part of a document's
/// equality: after opening a file, a clone's current shape and style are taken as synced.
#[derive(Clone, Debug, Default)]
pub struct CloneSync(HashMap<ObjectId, (Shape, Style)>);

impl PartialEq for CloneSync {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Object {
    /// Whether this object can be cloned: unlocked, not a group or a PowerClip.
    pub fn can_be_cloned(&self) -> bool {
        !self.locked && !matches!(self.shape, Shape::Group { .. }) && self.contents.is_empty()
    }
}

impl Document {
    pub fn can_clone(&self, ids: &[ObjectId]) -> bool {
        self.active_page().objects.iter().any(|o| ids.contains(&o.id) && o.can_be_cloned())
    }

    /// Clones of the objects among `ids` that can be cloned, on top, moved by `offset`.
    /// A clone of a clone follows the same master. Returns the clones' ids.
    pub fn clone_objects(&mut self, ids: &[ObjectId], offset: Vec2) -> Vec<ObjectId> {
        let masters: Vec<Object> = self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && o.can_be_cloned()).cloned().collect();
        let mut out = Vec::new();
        for master in masters {
            let mut clone = master.clone();
            clone.id = self.alloc_object_id();
            clone.name = None;
            clone.transform = kurbo::Affine::translate(offset) * master.transform;
            clone.clone_of = Some(master.clone_of.unwrap_or(master.id));
            self.clone_sync.0.insert(clone.id, (clone.shape.clone(), clone.style.clone()));
            out.push(clone.id);
            self.active_page_mut().objects.push(clone);
        }
        self.active_page_mut().settle_layers();
        out
    }

    /// Pass masters' shape and style changes on to their clones. A clone whose own shape
    /// or style was changed, or whose master is gone, stops being a clone.
    pub fn sync_clones(&mut self) {
        let mut sync = std::mem::take(&mut self.clone_sync.0);
        for page in &mut self.pages {
            // Runs every frame: pages without clones cost one pass.
            let wanted: std::collections::HashSet<ObjectId> = page.objects.iter().filter_map(|o| o.clone_of).collect();
            if wanted.is_empty() {
                continue;
            }
            let masters: HashMap<ObjectId, (Shape, Style)> = page
                .objects
                .iter()
                .filter(|o| wanted.contains(&o.id))
                .map(|o| (o.id, (o.shape.clone(), o.style.clone())))
                .collect();
            for obj in page.objects.iter_mut().filter(|o| o.clone_of.is_some()) {
                let master = obj.clone_of.and_then(|m| masters.get(&m));
                let synced = sync.entry(obj.id).or_insert_with(|| (obj.shape.clone(), obj.style.clone()));
                let edited = obj.shape != synced.0 || obj.style != synced.1;
                match master {
                    Some(m) if !edited => {
                        if *m != *synced {
                            obj.shape = m.0.clone();
                            obj.style = m.1.clone();
                            *synced = m.clone();
                        }
                    }
                    _ => {
                        obj.clone_of = None;
                        sync.remove(&obj.id);
                    }
                }
            }
        }
        self.clone_sync.0 = sync;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fill, Rgba};
    use kurbo::Rect;

    fn setup() -> (Document, ObjectId, ObjectId) {
        let mut doc = Document::default();
        let master = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 10.0, 10.0), corner_radius: 0.0 }, Style::new_object());
        let clone = doc.clone_objects(&[master], Vec2::new(20.0, 0.0))[0];
        (doc, master, clone)
    }

    fn obj(doc: &Document, id: ObjectId) -> &Object {
        doc.active_page().object(id).unwrap()
    }

    #[test]
    fn clones_follow_the_masters_shape_and_style_but_not_its_position() {
        let (mut doc, master, clone) = setup();
        assert_eq!(obj(&doc, clone).clone_of, Some(master));
        assert_eq!(obj(&doc, clone).label(), "Clone Rectangle");
        assert_eq!(obj(&doc, clone).world_bounds().x0, 20.0);
        doc.restyle_objects(&[master], |s| s.fill = Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
        if let Some(Shape::Rectangle { corner_radius, .. }) = doc.active_page_mut().object_mut(master).map(|o| &mut o.shape) {
            *corner_radius = 3.0;
        }
        doc.move_objects(&[master], Vec2::new(0.0, 50.0));
        doc.sync_clones();
        let c = obj(&doc, clone);
        assert_eq!(c.style.fill, Some(Fill::Solid(Rgba::rgb(255, 0, 0))));
        assert!(matches!(c.shape, Shape::Rectangle { corner_radius: 3.0, .. }));
        assert!(c.world_bounds().y0.abs() < 1e-9, "the clone keeps its own place");
        let second = doc.clone_objects(&[clone], Vec2::ZERO)[0];
        assert_eq!(obj(&doc, second).clone_of, Some(master), "a clone of a clone follows the master");
    }

    #[test]
    fn editing_the_clone_or_deleting_the_master_breaks_the_link() {
        let (mut doc, master, clone) = setup();
        doc.restyle_objects(&[clone], |s| s.fill = Some(Fill::Solid(Rgba::BLACK)));
        doc.sync_clones();
        assert_eq!(obj(&doc, clone).clone_of, None, "its own fill");
        doc.restyle_objects(&[master], |s| s.fill = None);
        doc.sync_clones();
        assert_eq!(obj(&doc, clone).style.fill, Some(Fill::Solid(Rgba::BLACK)), "no longer follows");

        let (mut doc, master, clone) = setup();
        doc.delete_objects(&[master]);
        doc.sync_clones();
        assert_eq!(obj(&doc, clone).clone_of, None);
    }

    #[test]
    fn links_survive_saving_and_groups_cannot_be_cloned() {
        let (doc, master, clone) = setup();
        let mut opened: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(opened, doc);
        opened.restyle_objects(&[master], |s| s.transparency = 0.5);
        opened.sync_clones();
        assert_eq!(obj(&opened, clone).style.transparency, 0.5, "still linked after opening");

        let mut doc = Document::default();
        let a = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 1.0, 1.0), corner_radius: 0.0 }, Style::default());
        let b = doc.add_object(Shape::Rectangle { rect: Rect::new(0.0, 0.0, 1.0, 1.0), corner_radius: 0.0 }, Style::default());
        let g = doc.group_objects(&[a, b]).unwrap();
        assert!(!doc.can_clone(&[g]));
        assert!(doc.clone_objects(&[g], Vec2::ZERO).is_empty());
    }
}

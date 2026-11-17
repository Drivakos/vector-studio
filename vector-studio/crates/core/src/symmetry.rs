//! Symmetry: a group whose members are drawn again, mirrored across one or more lines
//! through a centre. The mirrored copies follow the members as they change.

use std::f64::consts::TAU;

use kurbo::{Affine, Point};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, ObjectId, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Symmetry {
    /// Mirror lines, evenly spread round the centre; the first is vertical.
    pub lines: u32,
    /// Where the lines cross, in the group's space.
    pub center: Point,
}

impl Symmetry {
    pub const MAX_LINES: u32 = 12;

    /// The mirrored copies' placements in the group's space, the members' own left out:
    /// one reflection per line, and the rotations between them.
    pub fn copies(&self) -> Vec<Affine> {
        let n = self.lines.clamp(1, Self::MAX_LINES);
        let around = |a: Affine| Affine::translate(self.center.to_vec2()) * a * Affine::translate(-self.center.to_vec2());
        let flip = Affine::scale_non_uniform(-1.0, 1.0);
        let mut out = Vec::with_capacity(2 * n as usize - 1);
        for k in 0..n {
            let turn = Affine::rotate(TAU * f64::from(k) / f64::from(n));
            if k > 0 {
                out.push(around(turn));
            }
            out.push(around(turn * flip));
        }
        out
    }
}

impl Document {
    fn top_level(&self, ids: &[ObjectId], f: impl Fn(&Object) -> bool) -> Vec<ObjectId> {
        self.active_page().objects.iter().filter(|o| ids.contains(&o.id) && !o.locked && f(o)).map(|o| o.id).collect()
    }

    pub fn can_create_symmetry(&self, ids: &[ObjectId]) -> bool {
        !self.top_level(ids, |_| true).is_empty()
    }

    /// Create New Symmetry: group the unlocked objects among `ids` and mirror them across
    /// a vertical line through their middle. Returns the symmetry group.
    pub fn create_symmetry(&mut self, ids: &[ObjectId]) -> Option<ObjectId> {
        let group = self.wrap_in_group(ids, 1)?;
        let obj = self.active_page_mut().object_mut(group)?;
        obj.symmetry = Some(Symmetry { lines: 1, center: obj.world_bounds().center() });
        Some(group)
    }

    /// The unlocked symmetry groups among `ids`.
    pub fn symmetry_groups(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        self.top_level(ids, |o| o.symmetry.is_some())
    }

    /// Remove Symmetry: the mirrored copies become ordinary members of a plain group.
    /// Returns the groups changed.
    pub fn remove_symmetry(&mut self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let groups = self.symmetry_groups(ids);
        for &id in &groups {
            let Some(mut obj) = self.active_page().object(id).cloned() else { continue };
            let copies = obj.symmetry.take().map(|s| s.copies()).unwrap_or_default();
            if let Shape::Group { children } = &mut obj.shape {
                let members = children.clone();
                for m in copies {
                    for member in &members {
                        let mut copy = member.clone();
                        self.renumber(&mut copy);
                        copy.transform = m * copy.transform;
                        children.push(copy);
                    }
                }
            }
            *self.active_page_mut().object_mut(id).expect("still on the page") = obj;
        }
        groups
    }

    /// Take an unlocked group's members out onto the page, just above it and where they
    /// show, to edit them. The group stays, empty. Returns the members.
    pub fn lift_members(&mut self, group: ObjectId) -> Vec<ObjectId> {
        let page = self.active_page_mut();
        let Some(i) = page.objects.iter().position(|o| o.id == group && !o.locked) else { return Vec::new() };
        let (transform, layer) = (page.objects[i].transform, page.objects[i].layer);
        let Shape::Group { children } = &mut page.objects[i].shape else { return Vec::new() };
        let members = std::mem::take(children);
        let mut out = Vec::with_capacity(members.len());
        for (k, mut m) in members.into_iter().enumerate() {
            m.transform = transform * m.transform;
            m.layer = layer;
            out.push(m.id);
            page.objects.insert(i + 1 + k, m);
        }
        out
    }

    /// Put the objects among `ids` that are on the active page into `group`, on top of
    /// its members, where they are on the page. Returns whether any went in.
    pub fn sink_members(&mut self, group: ObjectId, ids: &[ObjectId]) -> bool {
        self.active_page_mut().sink_members(group, ids)
    }
}

impl crate::Page {
    /// See [`Document::sink_members`].
    pub fn sink_members(&mut self, group: ObjectId, ids: &[ObjectId]) -> bool {
        let page = self;
        let Some(to_local) = page.object(group).filter(|o| matches!(o.shape, Shape::Group { .. })).map(|o| o.transform.inverse()) else {
            return false;
        };
        let (mut moved, rest): (Vec<Object>, Vec<Object>) = std::mem::take(&mut page.objects).into_iter().partition(|o| ids.contains(&o.id) && o.id != group);
        page.objects = rest;
        if moved.is_empty() {
            return false;
        }
        for obj in &mut moved {
            obj.transform = to_local * obj.transform;
        }
        if let Some(Object { shape: Shape::Group { children }, .. }) = page.object_mut(group) {
            children.append(&mut moved);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rgba, Style};
    use kurbo::Rect;

    fn rect(doc: &mut Document, r: Rect) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: r, corner_radius: 0.0 }, Style::filled(Rgba::rgb(255, 0, 0)))
    }

    fn close(a: Rect, b: Rect) -> bool {
        (a.x0 - b.x0).abs() < 1e-9 && (a.y0 - b.y0).abs() < 1e-9 && (a.x1 - b.x1).abs() < 1e-9 && (a.y1 - b.y1).abs() < 1e-9
    }

    #[test]
    fn copies_reflect_across_each_line() {
        let one = Symmetry { lines: 1, center: Point::new(100.0, 0.0) };
        assert_eq!(one.copies().len(), 1);
        assert!((one.copies()[0] * Point::new(90.0, 5.0) - Point::new(110.0, 5.0)).hypot() < 1e-9, "across the vertical line");
        let two = Symmetry { lines: 2, center: Point::ZERO };
        let seen: Vec<Point> = two.copies().iter().map(|m| *m * Point::new(1.0, 2.0)).collect();
        assert_eq!(seen.len(), 3);
        for want in [Point::new(-1.0, 2.0), Point::new(-1.0, -2.0), Point::new(1.0, -2.0)] {
            assert!(seen.iter().any(|p| (*p - want).hypot() < 1e-9), "{want:?} in {seen:?}");
        }
        assert_eq!(Symmetry { lines: 99, center: Point::ZERO }.copies().len(), 2 * Symmetry::MAX_LINES as usize - 1);
    }

    #[test]
    fn a_symmetry_group_draws_its_mirror_and_can_be_edited_and_removed() {
        let mut doc = Document::default();
        let left = rect(&mut doc, Rect::new(0.0, 0.0, 40.0, 20.0));
        let right = rect(&mut doc, Rect::new(60.0, 50.0, 100.0, 60.0));
        assert!(doc.can_create_symmetry(&[left]));
        // Centre of both: x = 50.
        let group = doc.create_symmetry(&[left, right]).unwrap();
        let obj = doc.active_page().object(group).unwrap();
        assert_eq!(obj.label(), "Symmetry");
        assert_eq!(obj.leaves().len(), 4, "two members and their mirror");
        assert!(obj.hit(Point::new(80.0, 10.0), 0.0), "the mirror of the left one is hit");
        assert!(close(obj.world_bounds(), Rect::new(0.0, 0.0, 100.0, 60.0)));

        // Moving the group moves the mirror line with it.
        doc.move_objects(&[group], kurbo::Vec2::new(10.0, 0.0));
        assert!(doc.active_page().object(group).unwrap().hit(Point::new(90.0, 10.0), 0.0));

        // Edit: members out on the page, the group empty; then back in.
        let lifted = doc.lift_members(group);
        assert_eq!(lifted, vec![left, right]);
        assert_eq!(doc.active_page().objects.len(), 3);
        assert!(close(doc.active_page().object(left).unwrap().world_bounds(), Rect::new(10.0, 0.0, 50.0, 20.0)), "where it showed");
        doc.move_objects(&[left], kurbo::Vec2::new(0.0, 100.0));
        assert!(doc.sink_members(group, &lifted));
        let obj = doc.active_page().object(group).unwrap();
        assert_eq!(doc.active_page().objects.len(), 1);
        assert!(obj.hit(Point::new(90.0, 110.0), 0.0), "the mirror follows the edit");

        // Ungroup keeps what showed: the mirrored copies become objects.
        let out = doc.ungroup_objects(&[group]);
        assert_eq!(out.len(), 4);
        assert!(doc.active_page().objects.iter().all(|o| o.symmetry.is_none()));
    }

    #[test]
    fn remove_symmetry_keeps_the_copies_and_saves() {
        let mut doc = Document::default();
        let a = rect(&mut doc, Rect::new(0.0, 0.0, 10.0, 10.0));
        let group = doc.create_symmetry(&[a]).unwrap();
        doc.active_page_mut().object_mut(group).unwrap().symmetry.as_mut().unwrap().lines = 2;
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active_page().object(group).unwrap().symmetry.unwrap().lines, 2, "saved");
        assert_eq!(doc.remove_symmetry(&[group]), vec![group]);
        let obj = doc.active_page().object(group).unwrap();
        assert!(obj.symmetry.is_none());
        let Shape::Group { children } = &obj.shape else { panic!("group") };
        assert_eq!(children.len(), 4);
        let ids: std::collections::HashSet<_> = children.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), 4, "fresh ids");
    }
}

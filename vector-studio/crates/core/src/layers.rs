//! Layers of a page. Each top-level object names its layer; `Page::objects` stays one
//! list, bottom first, kept sorted so every object of a lower layer is drawn before the
//! objects of the layers above it (see [`Page::settle_layers`]).

use serde::{Deserialize, Serialize};

use crate::document::{Object, ObjectId, Page};

/// A layer's id, unique within its page. 0 means "not set": such objects go to the
/// active layer when the page is settled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerId(pub u64);

impl LayerId {
    pub const UNSET: LayerId = LayerId(0);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    /// Hidden layers are not drawn, exported, hit or snapped to.
    pub visible: bool,
    /// Objects on a locked layer are drawn but cannot be selected.
    pub locked: bool,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>) -> Self {
        Self { id, name: name.into(), visible: true, locked: false }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerError {
    LastLayer,
    NoSuchLayer,
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayerError::LastLayer => write!(f, "a page needs at least one layer"),
            LayerError::NoSuchLayer => write!(f, "there is no such layer"),
        }
    }
}

impl std::error::Error for LayerError {}

impl Page {
    /// Layers, bottom first.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    fn layer_mut(&mut self, id: LayerId) -> Result<&mut Layer, LayerError> {
        self.layers.iter_mut().find(|l| l.id == id).ok_or(LayerError::NoSuchLayer)
    }

    fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// The layer new objects go on.
    pub fn active_layer(&self) -> LayerId {
        self.active_layer
    }

    pub fn set_active_layer(&mut self, id: LayerId) -> Result<(), LayerError> {
        self.layer_index(id).ok_or(LayerError::NoSuchLayer)?;
        self.active_layer = id;
        Ok(())
    }

    fn next_layer_id(&self) -> LayerId {
        LayerId(self.layers.iter().map(|l| l.id.0).max().unwrap_or(0) + 1)
    }

    /// The first "Layer N" name not taken on this page.
    fn free_layer_name(&self) -> String {
        (1..).map(|n| format!("Layer {n}")).find(|name| self.layers.iter().all(|l| &l.name != name)).expect("unbounded")
    }

    /// Add a layer on top of the others and make it active.
    pub fn add_layer(&mut self) -> LayerId {
        let layer = Layer::new(self.next_layer_id(), self.free_layer_name());
        let id = layer.id;
        self.layers.push(layer);
        self.active_layer = id;
        id
    }

    /// Remove a layer and the objects on it. The layer below (or else above) becomes
    /// active if the removed one was.
    pub fn delete_layer(&mut self, id: LayerId) -> Result<(), LayerError> {
        let index = self.layer_index(id).ok_or(LayerError::NoSuchLayer)?;
        if self.layers.len() == 1 {
            return Err(LayerError::LastLayer);
        }
        self.layers.remove(index);
        self.objects.retain(|o| o.layer != id);
        if self.active_layer == id {
            self.active_layer = self.layers[index.saturating_sub(1)].id;
        }
        Ok(())
    }

    pub fn rename_layer(&mut self, id: LayerId, name: &str) -> Result<(), LayerError> {
        let name = name.trim();
        if !name.is_empty() {
            self.layer_mut(id)?.name = name.to_string();
        }
        Ok(())
    }

    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> Result<(), LayerError> {
        self.layer_mut(id)?.visible = visible;
        Ok(())
    }

    pub fn set_layer_locked(&mut self, id: LayerId, locked: bool) -> Result<(), LayerError> {
        self.layer_mut(id)?.locked = locked;
        Ok(())
    }

    /// Move a layer one place up (towards the front) or down, with its objects.
    pub fn move_layer(&mut self, id: LayerId, up: bool) -> Result<(), LayerError> {
        let i = self.layer_index(id).ok_or(LayerError::NoSuchLayer)?;
        let j = if up { i + 1 } else { i.wrapping_sub(1) };
        if j < self.layers.len() {
            self.layers.swap(i, j);
            self.settle_layers();
        }
        Ok(())
    }

    /// Put the objects among `ids` on top of layer `to`.
    pub fn move_objects_to_layer(&mut self, ids: &[ObjectId], to: LayerId) -> Result<(), LayerError> {
        self.layer_index(to).ok_or(LayerError::NoSuchLayer)?;
        let (mut moved, rest): (Vec<Object>, Vec<Object>) = std::mem::take(&mut self.objects).into_iter().partition(|o| ids.contains(&o.id));
        moved.iter_mut().for_each(|o| o.layer = to);
        self.objects = rest;
        self.objects.append(&mut moved);
        self.settle_layers();
        Ok(())
    }

    /// Whether `obj` is drawn: its layer is visible (objects of unknown layers are).
    pub fn is_shown(&self, obj: &Object) -> bool {
        self.layer(obj.layer).is_none_or(|l| l.visible)
    }

    /// Whether `obj` can be picked on the canvas: shown, on an unlocked layer.
    pub fn is_selectable(&self, obj: &Object) -> bool {
        self.layer(obj.layer).is_none_or(|l| l.visible && !l.locked)
    }

    /// The objects drawn, bottom first.
    pub fn shown_objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter(|o| self.is_shown(o))
    }

    /// Restore the page's layer invariants: at least one layer, a valid active layer,
    /// every object on an existing layer (else the active one), and objects ordered by
    /// layer while keeping their order within each layer.
    pub fn settle_layers(&mut self) {
        if self.layers.is_empty() {
            let id = self.next_layer_id();
            self.layers.push(Layer::new(id, "Layer 1"));
        }
        if self.layer_index(self.active_layer).is_none() {
            self.active_layer = self.layers.last().expect("one layer").id;
        }
        let active = self.active_layer;
        let index: std::collections::HashMap<LayerId, usize> = self.layers.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
        for obj in &mut self.objects {
            if !index.contains_key(&obj.layer) {
                obj.layer = active;
            }
        }
        self.objects.sort_by_key(|o| index[&o.layer]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape, Style};
    use kurbo::{Point, Rect};

    fn square(doc: &mut Document, x: f64) -> ObjectId {
        doc.add_object(Shape::Rectangle { rect: Rect::new(x, 0.0, x + 10.0, 10.0), corner_radius: 0.0 }, Style::filled(crate::Rgba::BLACK))
    }

    fn order(doc: &Document) -> Vec<ObjectId> {
        doc.active_page().objects.iter().map(|o| o.id).collect()
    }

    #[test]
    fn a_new_page_has_one_active_layer() {
        let doc = Document::default();
        let page = doc.active_page();
        assert_eq!(page.layers().len(), 1);
        assert_eq!(page.layers()[0].name, "Layer 1");
        assert_eq!(page.active_layer(), page.layers()[0].id);
    }

    #[test]
    fn new_objects_go_on_top_of_the_active_layer() {
        let mut doc = Document::default();
        let bottom = doc.active_page().active_layer();
        let a = square(&mut doc, 0.0);
        let top = doc.active_page_mut().add_layer();
        let b = square(&mut doc, 20.0);
        doc.active_page_mut().set_active_layer(bottom).unwrap();
        let c = square(&mut doc, 40.0);
        assert_eq!(order(&doc), vec![a, c, b], "c is on the lower layer, under b");
        assert_eq!(doc.active_page().object(b).unwrap().layer, top);
        assert_eq!(doc.active_page().layers()[1].name, "Layer 2");
    }

    #[test]
    fn hidden_layers_are_not_hit_and_locked_layers_are_shown_but_not_hit() {
        let mut doc = Document::default();
        let lower = doc.active_page().active_layer();
        let a = square(&mut doc, 0.0);
        let upper = doc.active_page_mut().add_layer();
        let b = square(&mut doc, 0.0);
        let page = doc.active_page_mut();
        let p = Point::new(5.0, 5.0);
        assert_eq!(page.hit_test(p, 0.0), Some(b));
        page.set_layer_visible(upper, false).unwrap();
        assert_eq!(page.hit_test(p, 0.0), Some(a));
        assert_eq!(page.shown_objects().count(), 1);
        page.set_layer_locked(lower, true).unwrap();
        assert_eq!(page.hit_test(p, 0.0), None);
        assert_eq!(page.shown_objects().count(), 1, "locked still draws");
        assert!(page.objects_inside(Rect::new(-1.0, -1.0, 20.0, 20.0)).is_empty());
    }

    #[test]
    fn moving_layers_and_objects_keeps_the_stack_in_layer_order() {
        let mut doc = Document::default();
        let lower = doc.active_page().active_layer();
        let a = square(&mut doc, 0.0);
        let b = square(&mut doc, 20.0);
        let upper = doc.active_page_mut().add_layer();
        let c = square(&mut doc, 40.0);
        let page = doc.active_page_mut();
        page.move_layer(upper, false).unwrap();
        assert_eq!(order(&doc), vec![c, a, b]);
        let page = doc.active_page_mut();
        page.move_objects_to_layer(&[a], upper).unwrap();
        assert_eq!(order(&doc), vec![c, a, b], "a lands on top of the (now lower) upper layer");
        assert_eq!(doc.active_page().object(a).unwrap().layer, upper);
        let page = doc.active_page_mut();
        assert_eq!(page.move_layer(upper, false), Ok(()), "already at the bottom: nothing moves");
        assert_eq!(page.layers()[0].id, upper);
        assert_eq!(page.layers()[1].id, lower);
    }

    #[test]
    fn deleting_a_layer_removes_its_objects_but_not_the_last_layer() {
        let mut doc = Document::default();
        let lower = doc.active_page().active_layer();
        let a = square(&mut doc, 0.0);
        let upper = doc.active_page_mut().add_layer();
        square(&mut doc, 20.0);
        let page = doc.active_page_mut();
        page.delete_layer(upper).unwrap();
        assert_eq!(order(&doc), vec![a]);
        let page = doc.active_page_mut();
        assert_eq!(page.active_layer(), lower);
        assert_eq!(page.delete_layer(lower), Err(LayerError::LastLayer));
        assert_eq!(page.delete_layer(LayerId(99)), Err(LayerError::NoSuchLayer));
        page.rename_layer(lower, "  Art ").unwrap();
        page.rename_layer(lower, "   ").unwrap();
        assert_eq!(page.layers()[0].name, "Art");
    }

    #[test]
    fn files_without_layers_get_one_holding_every_object() {
        let mut doc = Document::default();
        let a = square(&mut doc, 0.0);
        let mut json: serde_json::Value = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        let page = &mut json["pages"][0];
        page.as_object_mut().unwrap().remove("layers");
        page.as_object_mut().unwrap().remove("active_layer");
        page["objects"][0].as_object_mut().unwrap().remove("layer");
        let mut old: Document = serde_json::from_value(json).unwrap();
        old.repair();
        let page = old.active_page();
        assert_eq!(page.layers().len(), 1);
        assert_eq!(page.object(a).unwrap().layer, page.layers()[0].id);
    }
}

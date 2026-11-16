//! Page numbers: text fields that show the number of the page they are on.

use kurbo::Affine;

use crate::document::{Document, Object, ObjectId, Shape, Style};
use crate::text::{Text, TextAlign, TextField};

/// Distance from the bottom of the page to a new page number's baseline: half an inch.
pub const PAGE_NUMBER_MARGIN: f64 = 36.0;

impl Document {
    /// Add a page number to the active layer of the active page: artistic text in
    /// `font`'s family, style and size, centred near the bottom edge. Returns its id.
    pub fn insert_page_number(&mut self, font: &Text) -> ObjectId {
        let text = Text {
            content: String::new(),
            align: TextAlign::Center,
            frame: None,
            on_path: None,
            field: Some(TextField::PageNumber),
            ..font.clone()
        };
        let size = self.active_page().size;
        let id = self.add_object(Shape::Text(text), Style::new_text());
        if let Some(obj) = self.active_page_mut().object_mut(id) {
            obj.transform = Affine::translate((size.width / 2.0, size.height - PAGE_NUMBER_MARGIN));
        }
        self.sync_page_numbers();
        id
    }

    /// Show each page's number in its page number fields, inside groups and PowerClips
    /// too. Run after pages are added, removed or reordered.
    pub fn sync_page_numbers(&mut self) {
        for (i, page) in self.pages.iter_mut().enumerate() {
            let number = (i + 1).to_string();
            for obj in &mut page.objects {
                show_number(obj, &number);
            }
        }
    }
}

fn show_number(obj: &mut Object, number: &str) {
    match &mut obj.shape {
        Shape::Text(t) if t.field == Some(TextField::PageNumber) && t.content != number => t.content = number.to_owned(),
        Shape::Group { children } => children.iter_mut().for_each(|c| show_number(c, number)),
        _ => {}
    }
    obj.contents.iter_mut().for_each(|c| show_number(c, number));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(doc: &Document, page: usize, id: ObjectId) -> String {
        match &doc.pages()[page].object(id).unwrap().shape {
            Shape::Text(t) => t.content.clone(),
            _ => panic!("not text"),
        }
    }

    #[test]
    fn page_numbers_follow_their_page() {
        let mut doc = Document::default();
        doc.insert_page_after(0).unwrap();
        let id = doc.insert_page_number(&Text::new(""));
        assert_eq!(content(&doc, 1, id), "2");
        let obj = doc.pages()[1].object(id).unwrap();
        let size = doc.pages()[1].size;
        assert_eq!(obj.transform.translation(), kurbo::Vec2::new(size.width / 2.0, size.height - PAGE_NUMBER_MARGIN));

        doc.insert_page_before(0).unwrap();
        doc.sync_page_numbers();
        assert_eq!(content(&doc, 2, id), "3", "a page added before renumbers it");

        doc.set_active_page(2).unwrap();
        let other = doc.add_object(Shape::Text(Text::new("x")), Style::new_text());
        let group = doc.group_objects(&[id, other]).unwrap();
        doc.delete_page(0).unwrap();
        doc.sync_page_numbers();
        let Shape::Group { children } = &doc.pages()[1].object(group).unwrap().shape else { panic!("group") };
        assert!(matches!(&children[0].shape, Shape::Text(t) if t.content == "2"), "inside groups too");
    }

    #[test]
    fn plain_text_is_left_alone() {
        let mut doc = Document::default();
        let id = doc.add_object(Shape::Text(Text::new("7")), Style::new_text());
        doc.insert_page_after(0).unwrap();
        doc.sync_page_numbers();
        assert_eq!(content(&doc, 0, id), "7");
    }
}

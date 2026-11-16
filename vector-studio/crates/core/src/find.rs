//! Find and Replace for text: search every text object on every page, and replace words
//! in them. Page number fields are not searched; their text is the document's to set.

use std::ops::Range;

use crate::document::{Document, Object, ObjectId, Shape};
use crate::text::Text;

/// Where `query` occurs in `text`, as byte ranges, left to right and not overlapping.
/// Without `match_case`, letters match whatever their case.
pub fn find_all(text: &str, query: &str, match_case: bool) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let same = |a: char, b: char| a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()));
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        if needle.iter().enumerate().all(|(k, c)| same(chars[i + k].1, *c)) {
            let end = chars.get(i + needle.len()).map_or(text.len(), |c| c.0);
            out.push(chars[i].0..end);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// `text` with every match of `query` replaced by `with`, and how many there were.
pub fn replace_all(text: &str, query: &str, with: &str, match_case: bool) -> (String, usize) {
    let found = find_all(text, query, match_case);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for r in &found {
        out.push_str(&text[last..r.start]);
        out.push_str(with);
        last = r.end;
    }
    out.push_str(&text[last..]);
    (out, found.len())
}

/// A text object found: its page and the top-level object holding it (the text itself,
/// or the group or PowerClip it is in).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextMatch {
    pub page: usize,
    pub object: ObjectId,
}

fn searchable(t: &Text) -> bool {
    t.field.is_none()
}

/// Whether `obj` or anything in it is text containing `query`.
fn contains(obj: &Object, query: &str, match_case: bool) -> bool {
    let own = matches!(&obj.shape, Shape::Text(t) if searchable(t) && !find_all(&t.content, query, match_case).is_empty());
    let nested = match &obj.shape {
        Shape::Group { children } => children.iter().any(|c| contains(c, query, match_case)),
        _ => false,
    };
    own || nested || obj.contents.iter().any(|c| contains(c, query, match_case))
}

/// Replace `query` in `obj` and everything in it, unless locked. Returns the count.
fn replace_in(obj: &mut Object, query: &str, with: &str, match_case: bool) -> usize {
    if obj.locked {
        return 0;
    }
    let mut n = 0;
    match &mut obj.shape {
        Shape::Text(t) if searchable(t) => {
            let (content, count) = replace_all(&t.content, query, with, match_case);
            if count > 0 {
                t.content = content;
                n += count;
            }
        }
        Shape::Group { children } => n += children.iter_mut().map(|c| replace_in(c, query, with, match_case)).sum::<usize>(),
        _ => {}
    }
    n + obj.contents.iter_mut().map(|c| replace_in(c, query, with, match_case)).sum::<usize>()
}

impl Document {
    /// Objects with text containing `query`, page by page, bottom of the stack first.
    /// Only objects that can be picked (on visible, unlocked layers) are found.
    pub fn find_text(&self, query: &str, match_case: bool) -> Vec<TextMatch> {
        self.pages
            .iter()
            .enumerate()
            .flat_map(|(page, p)| {
                p.objects.iter().filter(|o| p.is_selectable(o) && contains(o, query, match_case)).map(move |o| TextMatch { page, object: o.id })
            })
            .collect()
    }

    /// Replace `query` with `with` in the text of `only`, or of every object
    /// [`Document::find_text`] finds. Locked objects are left alone. Returns how many
    /// times the text was replaced.
    pub fn replace_text(&mut self, query: &str, with: &str, match_case: bool, only: Option<TextMatch>) -> usize {
        let targets = match only {
            Some(m) => vec![m],
            None => self.find_text(query, match_case),
        };
        targets
            .into_iter()
            .filter_map(|m| self.pages.get_mut(m.page)?.object_mut(m.object).map(|o| replace_in(o, query, with, match_case)))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;

    fn text(doc: &mut Document, content: &str) -> ObjectId {
        doc.add_object(Shape::Text(Text::new(content)), Style::new_text())
    }

    fn content(doc: &Document, page: usize, id: ObjectId) -> String {
        match &doc.pages()[page].object(id).unwrap().shape {
            Shape::Text(t) => t.content.clone(),
            _ => panic!("not text"),
        }
    }

    #[test]
    fn finding_matches_case_only_when_asked() {
        assert_eq!(find_all("Cat cat CAT", "cat", false), vec![0..3, 4..7, 8..11]);
        assert_eq!(find_all("Cat cat CAT", "cat", true), vec![4..7]);
        assert_eq!(find_all("aaaa", "aa", true), vec![0..2, 2..4], "not overlapping");
        assert_eq!(find_all("Ünïcödé ünï", "ÜNÏ", false), vec![0..5, 12..17], "byte ranges of any letters");
        assert!(find_all("abc", "", false).is_empty());
        assert_eq!(replace_all("Cat cat", "CAT", "dog", false), ("dog dog".into(), 2));
    }

    #[test]
    fn find_and_replace_cover_every_page_groups_and_powerclips() {
        let mut doc = Document::default();
        let a = text(&mut doc, "Red apple");
        let other = text(&mut doc, "pear");
        let b = text(&mut doc, "red RED");
        let group = doc.group_objects(&[b, other]).unwrap();
        doc.insert_page_after(0).unwrap();
        let c = text(&mut doc, "no match");
        let d = text(&mut doc, "Reddish");
        let number = doc.insert_page_number(&Text::new(""));
        assert_eq!(doc.find_text("red", false), vec![TextMatch { page: 0, object: a }, TextMatch { page: 0, object: group }, TextMatch { page: 1, object: d }]);
        assert_eq!(doc.find_text("Red", true).len(), 2);
        assert!(doc.find_text("2", false).is_empty(), "page numbers are not searched");

        doc.set_locked(&[d], true);
        assert_eq!(doc.replace_text("red", "Blue", false, Some(TextMatch { page: 0, object: a })), 1);
        assert_eq!(content(&doc, 0, a), "Blue apple");
        assert_eq!(doc.replace_text("red", "blue", false, None), 2, "the group's two; the locked one stays");
        assert_eq!(content(&doc, 1, d), "Reddish");
        assert_eq!(content(&doc, 1, c), "no match");
        assert_eq!(content(&doc, 1, number), "2");
    }

    #[test]
    fn hidden_layers_are_not_searched() {
        let mut doc = Document::default();
        let lower = doc.active_page().active_layer();
        text(&mut doc, "find me");
        doc.active_page_mut().set_layer_visible(lower, false).unwrap();
        assert!(doc.find_text("find", false).is_empty());
        assert_eq!(doc.replace_text("find", "x", false, None), 0);
    }
}

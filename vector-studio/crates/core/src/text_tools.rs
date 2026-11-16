//! Text menu helpers that work on the words alone: Change Case and Text Statistics.

use crate::document::{Document, Object, Shape};
use crate::text::Text;

/// A way Change Case rewrites text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    /// The first letter of each sentence in capitals, the rest as typed.
    #[default]
    Sentence,
    Lower,
    Upper,
    /// The first letter of each word in capitals, the rest as typed.
    Title,
    /// Capitals become small letters and small letters capitals.
    Toggle,
}

impl Case {
    pub const ALL: [Case; 5] = [Case::Sentence, Case::Lower, Case::Upper, Case::Title, Case::Toggle];

    pub fn label(self) -> &'static str {
        match self {
            Case::Sentence => "Sentence case.",
            Case::Lower => "lowercase",
            Case::Upper => "UPPERCASE",
            Case::Title => "Title Case",
            Case::Toggle => "tOGGLE cASE",
        }
    }
}

fn upper(c: char) -> impl Iterator<Item = char> {
    c.to_uppercase()
}

/// `text` rewritten in `case`.
pub fn change_case(text: &str, case: Case) -> String {
    match case {
        Case::Lower => text.to_lowercase(),
        Case::Upper => text.to_uppercase(),
        Case::Toggle => text
            .chars()
            .flat_map(|c| -> Box<dyn Iterator<Item = char>> {
                if c.is_uppercase() {
                    Box::new(c.to_lowercase())
                } else if c.is_lowercase() {
                    Box::new(c.to_uppercase())
                } else {
                    Box::new(std::iter::once(c))
                }
            })
            .collect(),
        Case::Title => {
            let mut out = String::with_capacity(text.len());
            let mut word_start = true;
            for c in text.chars() {
                if word_start && c.is_alphabetic() {
                    out.extend(upper(c));
                } else {
                    out.push(c);
                }
                word_start = c.is_whitespace() || matches!(c, '-' | '/' | '(' | '"' | '“');
            }
            out
        }
        Case::Sentence => {
            let mut out = String::with_capacity(text.len());
            let mut sentence_start = true;
            for c in text.chars() {
                if sentence_start && c.is_alphabetic() {
                    out.extend(upper(c));
                    sentence_start = false;
                } else {
                    out.push(c);
                }
                if matches!(c, '.' | '!' | '?' | '\n') {
                    sentence_start = true;
                } else if c.is_alphanumeric() {
                    sentence_start = false;
                }
            }
            out
        }
    }
}

/// Counts for Text Statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub objects: usize,
    /// Lines as laid out: wrapped lines of paragraph text count one by one.
    pub lines: usize,
    pub words: usize,
    /// Characters, spaces included, line breaks not.
    pub characters: usize,
    /// Font family and style of each object, each once, in the order met.
    pub fonts: Vec<String>,
}

impl TextStats {
    fn add(&mut self, text: &Text) {
        self.objects += 1;
        self.lines += text.layout().lines.len();
        self.words += text.content.split_whitespace().count();
        self.characters += text.content.chars().filter(|c| *c != '\n').count();
        let font = format!("{} {}", text.family, text.style);
        if !self.fonts.contains(&font) {
            self.fonts.push(font);
        }
    }

    /// Statistics of the text among `objects`, groups and PowerClip contents included.
    pub fn of<'a>(objects: impl IntoIterator<Item = &'a Object>) -> Self {
        let mut stats = Self::default();
        fn walk(stats: &mut TextStats, obj: &Object) {
            match &obj.shape {
                Shape::Text(t) => stats.add(t),
                Shape::Group { children } => children.iter().for_each(|c| walk(stats, c)),
                _ => {}
            }
            obj.contents.iter().for_each(|c| walk(stats, c));
        }
        for obj in objects {
            walk(&mut stats, obj);
        }
        stats
    }

    /// Statistics of every text object on every page.
    pub fn of_document(doc: &Document) -> Self {
        Self::of(doc.pages().iter().flat_map(|p| p.objects.iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;

    #[test]
    fn cases() {
        let s = "the CAT sat. on a mat! ok?\nnew line";
        assert_eq!(change_case(s, Case::Lower), "the cat sat. on a mat! ok?\nnew line");
        assert_eq!(change_case(s, Case::Upper), "THE CAT SAT. ON A MAT! OK?\nNEW LINE");
        assert_eq!(change_case(s, Case::Title), "The CAT Sat. On A Mat! Ok?\nNew Line");
        assert_eq!(change_case(s, Case::Sentence), "The CAT sat. On a mat! Ok?\nNew line");
        assert_eq!(change_case("Hello World", Case::Toggle), "hELLO wORLD");
        assert_eq!(change_case("well-known (x)", Case::Title), "Well-Known (X)");
    }

    #[test]
    fn statistics_count_text_in_groups_and_ignore_other_shapes() {
        let mut doc = Document::default();
        doc.add_object(Shape::Text(Text::new("one two\nthree")), Style::new_text());
        doc.add_object(Shape::Rectangle { rect: kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), corner_radius: 0.0 }, Style::default());
        let stats = TextStats::of_document(&doc);
        assert_eq!(stats.objects, 1);
        assert_eq!(stats.lines, 2);
        assert_eq!(stats.words, 3);
        assert_eq!(stats.characters, 12);
        assert_eq!(stats.fonts.len(), 1);
    }
}

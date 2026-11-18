//! Print Merge: records from a CSV or tab-separated text file whose first row names the
//! fields. Text holds a field as «Name»; merging makes a copy of every page for each
//! record, with the fields in its text filled in from that record.

use serde::{Deserialize, Serialize};

use crate::document::{Document, Object, PageId, Shape};

/// The fields and records of a Print Merge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeData {
    pub fields: Vec<String>,
    /// One value per field in each record.
    pub records: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeError {
    /// No field names.
    Empty,
    /// The 1-based column whose field has no name.
    UnnamedField(usize),
    DuplicateField(String),
    /// Field names but no records under them.
    NoRecords,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeError::Empty => write!(f, "the file is empty; its first row should name the fields"),
            MergeError::UnnamedField(n) => write!(f, "column {n} of the first row has no field name"),
            MergeError::DuplicateField(name) => write!(f, "two columns are called “{name}”"),
            MergeError::NoRecords => write!(f, "there are no records under the field names"),
        }
    }
}

impl std::error::Error for MergeError {}

/// How a field is written in text: «Name».
pub fn token(field: &str) -> String {
    format!("«{field}»")
}

/// The separator a header line uses: a tab if it has one, else a semicolon if it has more
/// of them than commas, else a comma. Quoted text is not counted.
fn separator(header: &str) -> char {
    let (mut quoted, mut tabs, mut semicolons, mut commas) = (false, 0, 0, 0);
    for c in header.chars() {
        match c {
            '"' => quoted = !quoted,
            '\t' if !quoted => tabs += 1,
            ';' if !quoted => semicolons += 1,
            ',' if !quoted => commas += 1,
            _ => {}
        }
    }
    if tabs > 0 {
        '\t'
    } else if semicolons > commas {
        ';'
    } else {
        ','
    }
}

/// Rows of cells (RFC 4180): quoted cells may hold separators, line breaks and doubled
/// quotes; lines end with LF or CRLF.
fn rows(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let (mut row, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    cell.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => cell.push(c),
            }
        } else if c == '"' {
            quoted = true;
        } else if c == sep {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            out.push(std::mem::take(&mut row));
        } else {
            cell.push(c);
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        out.push(row);
    }
    out
}

impl MergeData {
    /// Read CSV (comma or semicolon) or tab-separated text. The first row names the
    /// fields; each later row is a record. Blank rows are skipped, missing values are
    /// empty and values past the last field are dropped.
    pub fn parse(text: &str) -> Result<Self, MergeError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let header = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let mut all = rows(text, separator(header)).into_iter().filter(|r| r.iter().any(|c| !c.trim().is_empty()));
        let fields: Vec<String> = all.next().ok_or(MergeError::Empty)?.iter().map(|f| f.trim().to_owned()).collect();
        for (i, name) in fields.iter().enumerate() {
            if name.is_empty() {
                return Err(MergeError::UnnamedField(i + 1));
            }
            if fields[..i].contains(name) {
                return Err(MergeError::DuplicateField(name.clone()));
            }
        }
        let records: Vec<Vec<String>> = all
            .map(|mut r| {
                r.resize(fields.len(), String::new());
                r
            })
            .collect();
        if records.is_empty() {
            return Err(MergeError::NoRecords);
        }
        Ok(Self { fields, records })
    }

    /// `text` with each «field» filled in from record `record`. Names that are not
    /// fields stay as they are.
    pub fn fill(&self, text: &str, record: usize) -> String {
        let values = self.records.get(record);
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('«') {
            out.push_str(&rest[..open]);
            let after = &rest[open + '«'.len_utf8()..];
            let found = after.find('»').and_then(|close| {
                let i = self.fields.iter().position(|f| *f == after[..close])?;
                Some((close, values.map_or("", |v| v[i].as_str())))
            });
            match found {
                Some((close, value)) => {
                    out.push_str(value);
                    rest = &after[close + '»'.len_utf8()..];
                }
                None => {
                    out.push('«');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Fill in the fields of `obj` and everything in it. A symbol instance whose text
    /// changes becomes a plain group, so its symbol and other instances keep theirs.
    /// Returns whether anything changed.
    fn fill_object(&self, obj: &mut Object, record: usize) -> bool {
        let mut changed = false;
        match &mut obj.shape {
            Shape::Text(t) if t.field.is_none() => {
                let filled = self.fill(&t.content, record);
                changed = filled != t.content;
                t.content = filled;
            }
            Shape::Group { children } => children.iter_mut().for_each(|c| changed |= self.fill_object(c, record)),
            _ => {}
        }
        obj.contents.iter_mut().for_each(|c| changed |= self.fill_object(c, record));
        if changed && obj.symbol.is_some() {
            crate::symbols::unlink(obj);
        }
        changed
    }
}

impl Document {
    /// A copy of the drawing merged with its Print Merge data: for each record in turn, a
    /// copy of every page with the fields in its text (in groups and PowerClips too)
    /// filled in. The copy has no merge data. `None` without data.
    pub fn merged(&self) -> Option<Document> {
        let data = self.merge.as_ref().filter(|d| !d.records.is_empty())?;
        let mut out = self.clone();
        out.merge = None;
        let pages = std::mem::take(&mut out.pages);
        for record in 0..data.records.len() {
            for page in &pages {
                let mut copy = page.clone();
                copy.id = PageId(out.alloc_id());
                for obj in &mut copy.objects {
                    out.renumber(obj);
                    data.fill_object(obj, record);
                }
                out.pages.push(copy);
            }
        }
        out.set_active_page(0).ok()?;
        out.sync_page_numbers();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;
    use crate::text::Text;

    fn data(fields: &[&str], records: &[&[&str]]) -> MergeData {
        MergeData {
            fields: fields.iter().map(|s| s.to_string()).collect(),
            records: records.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect(),
        }
    }

    fn texts(obj: &Object, out: &mut Vec<String>) {
        match &obj.shape {
            Shape::Text(t) => out.push(t.content.clone()),
            Shape::Group { children } => children.iter().for_each(|c| texts(c, out)),
            _ => {}
        }
        obj.contents.iter().for_each(|c| texts(c, out));
    }

    fn page_texts(doc: &Document, page: usize) -> Vec<String> {
        let mut out = Vec::new();
        doc.pages()[page].objects.iter().for_each(|o| texts(o, &mut out));
        out
    }

    #[test]
    fn reads_csv_semicolons_and_tabs_with_quotes() {
        let d = MergeData::parse("\u{feff}Name,City\r\n\"Smith, Jo\",\"Line 1\nLine 2\"\r\n\r\nAl,\"say \"\"hi\"\"\"\n").unwrap();
        assert_eq!(d, data(&["Name", "City"], &[&["Smith, Jo", "Line 1\nLine 2"], &["Al", "say \"hi\""]]));
        assert_eq!(MergeData::parse("A;B\n1,5;2").unwrap(), data(&["A", "B"], &[&["1,5", "2"]]), "semicolons, decimal commas");
        assert_eq!(MergeData::parse("A\tB, C\nx\ty").unwrap(), data(&["A", "B, C"], &[&["x", "y"]]), "tabs");
        assert_eq!(MergeData::parse(" A , B \nonly\n1,2,3").unwrap(), data(&["A", "B"], &[&["only", ""], &["1", "2"]]), "padded and cut to the fields");
    }

    #[test]
    fn rejects_files_it_cannot_merge() {
        assert_eq!(MergeData::parse(" \n\n"), Err(MergeError::Empty));
        assert_eq!(MergeData::parse("A,,C\n1,2,3"), Err(MergeError::UnnamedField(2)));
        assert_eq!(MergeData::parse("A,A\n1,2"), Err(MergeError::DuplicateField("A".into())));
        assert_eq!(MergeData::parse("A,B\n"), Err(MergeError::NoRecords));
    }

    #[test]
    fn fills_known_fields_once_and_leaves_the_rest() {
        let d = data(&["Name", "Note"], &[&["Jo", "«Name»"]]);
        assert_eq!(d.fill("Hi «Name»! «Note» «Other» « »", 0), "Hi Jo! «Name» «Other» « »", "values are not filled again");
        assert_eq!(d.fill("«Name", 0), "«Name", "unclosed");
        assert_eq!(d.fill("«Name»", 5), "", "no such record");
        assert_eq!(token("Name"), "«Name»");
    }

    #[test]
    fn merging_copies_every_page_per_record() {
        let mut doc = Document::default();
        assert!(doc.merged().is_none(), "no data");
        let hello = doc.add_object(Shape::Text(Text::new("Dear «Name»")), Style::new_text());
        let other = doc.add_object(Shape::Text(Text::new("«City»")), Style::new_text());
        doc.group_objects(&[hello, other]).unwrap();
        doc.insert_page_after(0).unwrap();
        doc.add_object(Shape::Text(Text::new("Bye «Name»")), Style::new_text());
        let number = doc.insert_page_number(&Text::new(""));
        doc.merge = Some(data(&["Name", "City"], &[&["Ann", "Oslo"], &["Bo", "Rome"]]));

        let merged = doc.merged().unwrap();
        assert!(merged.merge.is_none());
        assert_eq!(merged.page_count(), 4);
        assert_eq!(merged.active_page_index(), 0);
        assert_eq!(page_texts(&merged, 0), ["Dear Ann", "Oslo"]);
        assert_eq!(page_texts(&merged, 1), ["Bye Ann", "2"]);
        assert_eq!(page_texts(&merged, 2), ["Dear Bo", "Rome"]);
        assert_eq!(page_texts(&merged, 3), ["Bye Bo", "4"], "page numbers follow the merged pages");
        let ids: Vec<_> = merged.pages().iter().flat_map(|p| p.objects.iter().map(|o| o.id)).collect();
        assert!(ids.iter().all(|id| ids.iter().filter(|x| *x == id).count() == 1), "fresh ids");
        assert!(!ids.contains(&hello) && !ids.contains(&number));
        assert_eq!(page_texts(&doc, 0), ["Dear «Name»", "«City»"], "the drawing is unchanged");
    }

    #[test]
    fn filled_symbol_instances_become_plain_groups() {
        let mut doc = Document::default();
        let card = doc.add_object(Shape::Text(Text::new("«Name»")), Style::new_text());
        let logo = doc.add_object(Shape::Text(Text::new("Logo")), Style::new_text());
        doc.new_symbol(&[card]).unwrap();
        doc.new_symbol(&[logo]).unwrap();
        doc.merge = Some(data(&["Name"], &[&["Ann"], &["Bo"]]));
        let mut merged = doc.merged().unwrap();
        merged.sync_symbols();
        assert_eq!(page_texts(&merged, 0), ["Ann", "Logo"]);
        assert_eq!(page_texts(&merged, 1), ["Bo", "Logo"], "not overwritten by the first record");
        let symbol = |p: usize, i: usize| merged.pages()[p].objects[i].symbol;
        assert!(symbol(0, 0).is_none() && symbol(1, 0).is_none());
        assert!(symbol(0, 1).is_some() && symbol(1, 1).is_some(), "instances without fields stay instances");
        assert_eq!(page_texts(&doc, 0), ["«Name»", "Logo"]);
    }
}

//! Document model: a document owns pages, each page owns its objects.

use kurbo::{Affine, BezPath, Rect, Size};
use serde::{Deserialize, Serialize};

use crate::units::Unit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// Straight (non-premultiplied) sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub color: Rgba,
    /// Width in points. 0 means hairline.
    pub width: f64,
}

impl Outline {
    /// Width drawn for a hairline (0) outline, in points.
    pub const HAIRLINE: f64 = 0.25;

    pub fn effective_width(&self) -> f64 {
        if self.width > 0.0 { self.width } else { Self::HAIRLINE }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Rgba>,
    pub outline: Option<Outline>,
}

impl Style {
    /// Style for newly drawn objects: no fill, black 0.5 pt outline.
    pub fn new_object() -> Self {
        Self {
            fill: None,
            outline: Some(Outline { color: Rgba::BLACK, width: 0.5 }),
        }
    }
}

/// Geometry in the object's local space. The object's `transform` maps it to page space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Rectangle { rect: Rect, corner_radius: f64 },
    Ellipse { rect: Rect },
    /// Regular polygon inscribed in `rect`, first vertex at the top.
    Polygon { rect: Rect, sides: u32 },
    /// Star inscribed in `rect`; inner vertices sit at `inner_ratio` of the outer radius.
    Star { rect: Rect, points: u32, inner_ratio: f64 },
    Curve { path: BezPath },
}

impl Shape {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Shape::Rectangle { .. } => "Rectangle",
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Polygon { .. } => "Polygon",
            Shape::Star { .. } => "Star",
            Shape::Curve { .. } => "Curve",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    pub name: Option<String>,
    pub shape: Shape,
    pub style: Style,
    /// Local → page transform.
    #[serde(default)]
    pub transform: Affine,
}

impl Object {
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => self.shape.kind_label().to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub name: String,
    /// Size in points.
    pub size: Size,
    /// Bottom of the stack first.
    pub objects: Vec<Object>,
}

impl Page {
    pub fn bounds(&self) -> Rect {
        Rect::from_origin_size((0.0, 0.0), self.size)
    }

    pub fn orientation(&self) -> Orientation {
        if self.size.width > self.size.height {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        }
    }

    pub fn set_orientation(&mut self, orientation: Orientation) {
        if self.orientation() != orientation {
            self.size = Size::new(self.size.height, self.size.width);
        }
    }
}

/// Standard paper sizes, portrait, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagePreset {
    pub name: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

pub const PAGE_PRESETS: &[PagePreset] = &[
    PagePreset { name: "A3", width_mm: 297.0, height_mm: 420.0 },
    PagePreset { name: "A4", width_mm: 210.0, height_mm: 297.0 },
    PagePreset { name: "A5", width_mm: 148.0, height_mm: 210.0 },
    PagePreset { name: "Letter", width_mm: 215.9, height_mm: 279.4 },
    PagePreset { name: "Legal", width_mm: 215.9, height_mm: 355.6 },
    PagePreset { name: "Tabloid", width_mm: 279.4, height_mm: 431.8 },
];

impl PagePreset {
    pub fn size(&self, orientation: Orientation) -> Size {
        let w = Unit::Millimeters.to_points(self.width_mm);
        let h = Unit::Millimeters.to_points(self.height_mm);
        match orientation {
            Orientation::Portrait => Size::new(w, h),
            Orientation::Landscape => Size::new(h, w),
        }
    }

    /// The preset matching `size` in either orientation, within 0.5 pt.
    pub fn matching(size: Size) -> Option<&'static PagePreset> {
        PAGE_PRESETS.iter().find(|p| {
            let s = p.size(Orientation::Portrait);
            let close = |a: f64, b: f64| (a - b).abs() < 0.5;
            (close(s.width, size.width) && close(s.height, size.height))
                || (close(s.width, size.height) && close(s.height, size.width))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentError {
    LastPage,
    PageOutOfRange(usize),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentError::LastPage => write!(f, "a document needs at least one page"),
            DocumentError::PageOutOfRange(i) => write!(f, "there is no page {}", i + 1),
        }
    }
}

impl std::error::Error for DocumentError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub display_unit: Unit,
    pages: Vec<Page>,
    active_page: usize,
    next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new("Untitled-1")
    }
}

impl Document {
    /// A new document with one A4 portrait page.
    pub fn new(title: impl Into<String>) -> Self {
        let mut doc = Self {
            title: title.into(),
            display_unit: Unit::Millimeters,
            pages: Vec::new(),
            active_page: 0,
            next_id: 1,
        };
        let size = PAGE_PRESETS[1].size(Orientation::Portrait);
        let page = doc.make_page(size);
        doc.pages.push(page);
        doc
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn make_page(&mut self, size: Size) -> Page {
        let id = PageId(self.alloc_id());
        Page {
            id,
            name: String::new(),
            size,
            objects: Vec::new(),
        }
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn active_page_index(&self) -> usize {
        self.active_page
    }

    pub fn active_page(&self) -> &Page {
        &self.pages[self.active_page]
    }

    pub fn active_page_mut(&mut self) -> &mut Page {
        &mut self.pages[self.active_page]
    }

    pub fn page_mut(&mut self, index: usize) -> Option<&mut Page> {
        self.pages.get_mut(index)
    }

    /// Display name: the custom name, or "Page N".
    pub fn page_label(&self, index: usize) -> String {
        match self.pages.get(index) {
            Some(p) if !p.name.trim().is_empty() => p.name.clone(),
            Some(_) => format!("Page {}", index + 1),
            None => String::new(),
        }
    }

    pub fn set_active_page(&mut self, index: usize) -> Result<(), DocumentError> {
        if index >= self.pages.len() {
            return Err(DocumentError::PageOutOfRange(index));
        }
        self.active_page = index;
        Ok(())
    }

    /// Insert an empty page after `index` with the same size, and make it active.
    pub fn insert_page_after(&mut self, index: usize) -> Result<usize, DocumentError> {
        let size = self
            .pages
            .get(index)
            .ok_or(DocumentError::PageOutOfRange(index))?
            .size;
        let page = self.make_page(size);
        self.pages.insert(index + 1, page);
        self.active_page = index + 1;
        Ok(index + 1)
    }

    /// Insert a blank page (same size as page `index`) before it, and make it active.
    pub fn insert_page_before(&mut self, index: usize) -> Result<usize, DocumentError> {
        let size = self.pages.get(index).ok_or(DocumentError::PageOutOfRange(index))?.size;
        let page = self.make_page(size);
        self.pages.insert(index, page);
        self.active_page = index;
        Ok(index)
    }

    /// Copy page `index` (with fresh ids) after itself, and make the copy active.
    pub fn duplicate_page(&mut self, index: usize) -> Result<usize, DocumentError> {
        let mut copy = self
            .pages
            .get(index)
            .ok_or(DocumentError::PageOutOfRange(index))?
            .clone();
        copy.id = PageId(self.alloc_id());
        for obj in &mut copy.objects {
            obj.id = ObjectId(self.alloc_id());
        }
        self.pages.insert(index + 1, copy);
        self.active_page = index + 1;
        Ok(index + 1)
    }

    pub fn delete_page(&mut self, index: usize) -> Result<Page, DocumentError> {
        if index >= self.pages.len() {
            return Err(DocumentError::PageOutOfRange(index));
        }
        if self.pages.len() == 1 {
            return Err(DocumentError::LastPage);
        }
        let removed = self.pages.remove(index);
        if self.active_page >= self.pages.len() || self.active_page > index {
            self.active_page = self.active_page.saturating_sub(1);
        }
        Ok(removed)
    }

    /// Add an object on top of the active page.
    pub fn add_object(&mut self, shape: Shape, style: Style) -> ObjectId {
        let id = ObjectId(self.alloc_id());
        self.active_page_mut().objects.push(Object {
            id,
            name: None,
            shape,
            style,
            transform: Affine::IDENTITY,
        });
        id
    }

    pub(crate) fn alloc_object_id(&mut self) -> ObjectId {
        ObjectId(self.alloc_id())
    }

    /// Fix up a document read from a file: ids never collide with new ones, the active
    /// page exists, and there is at least one page.
    pub fn repair(&mut self) {
        if self.pages.is_empty() {
            let size = PAGE_PRESETS[1].size(Orientation::Portrait);
            let page = self.make_page(size);
            self.pages.push(page);
        }
        self.active_page = self.active_page.min(self.pages.len() - 1);
        let max = self
            .pages
            .iter()
            .flat_map(|p| std::iter::once(p.id.0).chain(p.objects.iter().map(|o| o.id.0)))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_one_a4_portrait_page() {
        let doc = Document::new("Test");
        assert_eq!(doc.page_count(), 1);
        let page = doc.active_page();
        assert_eq!(page.orientation(), Orientation::Portrait);
        assert_eq!(PagePreset::matching(page.size).map(|p| p.name), Some("A4"));
        assert_eq!(doc.page_label(0), "Page 1");
    }

    #[test]
    fn insert_page_after_copies_size_and_activates() {
        let mut doc = Document::default();
        doc.active_page_mut().set_orientation(Orientation::Landscape);
        let idx = doc.insert_page_after(0).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(doc.active_page_index(), 1);
        assert_eq!(doc.active_page().orientation(), Orientation::Landscape);
        assert_ne!(doc.pages()[0].id, doc.pages()[1].id);
    }

    #[test]
    fn insert_page_before_puts_it_first_and_activates() {
        let mut doc = Document::default();
        let first = doc.pages()[0].id;
        assert_eq!(doc.insert_page_before(0), Ok(0));
        assert_eq!(doc.page_count(), 2);
        assert_eq!(doc.active_page_index(), 0);
        assert_eq!(doc.pages()[1].id, first);
        assert_eq!(doc.insert_page_before(5), Err(DocumentError::PageOutOfRange(5)));
    }

    #[test]
    fn duplicate_page_gives_fresh_ids() {
        let mut doc = Document::default();
        let original = doc.add_object(
            Shape::Ellipse { rect: Rect::new(0.0, 0.0, 10.0, 10.0) },
            Style::default(),
        );
        doc.duplicate_page(0).unwrap();
        let copy = &doc.pages()[1];
        assert_eq!(copy.objects.len(), 1);
        assert_ne!(copy.objects[0].id, original);
        assert_ne!(copy.id, doc.pages()[0].id);
    }

    #[test]
    fn cannot_delete_last_page() {
        let mut doc = Document::default();
        assert_eq!(doc.delete_page(0), Err(DocumentError::LastPage));
    }

    #[test]
    fn deleting_keeps_active_page_valid() {
        let mut doc = Document::default();
        doc.insert_page_after(0).unwrap();
        doc.insert_page_after(1).unwrap(); // pages: 0,1,2 ; active = 2
        doc.delete_page(2).unwrap();
        assert_eq!(doc.active_page_index(), 1);

        doc.set_active_page(1).unwrap();
        doc.delete_page(0).unwrap(); // removing a page before the active one
        assert_eq!(doc.active_page_index(), 0);
        assert_eq!(doc.page_count(), 1);
    }

    #[test]
    fn out_of_range_is_an_error() {
        let mut doc = Document::default();
        assert_eq!(doc.set_active_page(3), Err(DocumentError::PageOutOfRange(3)));
        assert_eq!(doc.insert_page_after(3), Err(DocumentError::PageOutOfRange(3)));
    }

    #[test]
    fn custom_page_name_wins_over_default_label() {
        let mut doc = Document::default();
        doc.active_page_mut().name = "Cover".into();
        assert_eq!(doc.page_label(0), "Cover");
    }

    #[test]
    fn orientation_swaps_dimensions_once() {
        let mut doc = Document::default();
        let portrait = doc.active_page().size;
        doc.active_page_mut().set_orientation(Orientation::Landscape);
        doc.active_page_mut().set_orientation(Orientation::Landscape);
        assert_eq!(doc.active_page().size, Size::new(portrait.height, portrait.width));
    }
}

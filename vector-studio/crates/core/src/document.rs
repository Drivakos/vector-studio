//! Document model: a document owns pages, each page owns its objects.

use kurbo::{Affine, BezPath, Rect, Size};
use serde::{Deserialize, Serialize};

use crate::bitmap::Bitmap;
use crate::fill::Fill;
use crate::layers::{Layer, LayerId};
use crate::text::Text;
use crate::units::Unit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// Straight (non-premultiplied) sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    /// The CMYK inks (0–255 each) the colour was given as, written as they are to CMYK
    /// output; `r`, `g` and `b` are how they look on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmyk: Option<[u8; 4]>,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a, cmyk: None }
    }

    /// This colour as the inks `cmyk`, looking like `self` on screen.
    pub const fn with_cmyk(self, cmyk: [u8; 4]) -> Self {
        Self { cmyk: Some(cmyk), ..self }
    }

    /// `#rrggbb` (alpha is left out).
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// An opaque colour from `#rrggbb` or `rrggbb`, any case.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.trim().trim_start_matches('#');
        if digits.len() != 6 || !digits.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?))
    }

    /// The colour `k` (0..=1) of the way from this one to `other`.
    pub fn mix(self, other: Rgba, k: f64) -> Rgba {
        let k = k.clamp(0.0, 1.0);
        let l = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * k).round() as u8;
        // Between two ink colours, the inks mix too.
        let cmyk = match (self.cmyk, other.cmyk) {
            (Some(p), Some(q)) => Some([0, 1, 2, 3].map(|i| l(p[i], q[i]))),
            _ => None,
        };
        Rgba { r: l(self.r, other.r), g: l(self.g, other.g), b: l(self.b, other.b), a: l(self.a, other.a), cmyk }
    }

    /// This colour with its alpha multiplied by `opacity` (0..=1).
    pub fn faded(self, opacity: f64) -> Self {
        Self { a: (f64::from(self.a) * opacity.clamp(0.0, 1.0)).round() as u8, ..self }
    }
}

/// How outline segments meet at corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// How open outline ends are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Dash pattern of an outline. Dash and gap lengths are multiples of the outline width,
/// so a pattern keeps its look at any width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    #[default]
    Solid,
    Dot,
    Dash,
    LongDash,
    DashDot,
    DashDotDot,
}

impl LineStyle {
    pub const ALL: [LineStyle; 6] = [
        LineStyle::Solid,
        LineStyle::Dot,
        LineStyle::Dash,
        LineStyle::LongDash,
        LineStyle::DashDot,
        LineStyle::DashDotDot,
    ];

    /// Dash and gap lengths in outline widths; empty for solid.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            LineStyle::Solid => &[],
            LineStyle::Dot => &[1.0, 2.0],
            LineStyle::Dash => &[4.0, 2.0],
            LineStyle::LongDash => &[8.0, 3.0],
            LineStyle::DashDot => &[4.0, 2.0, 1.0, 2.0],
            LineStyle::DashDotDot => &[4.0, 2.0, 1.0, 2.0, 1.0, 2.0],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Dot => "Dotted",
            LineStyle::Dash => "Dashed",
            LineStyle::LongDash => "Long dash",
            LineStyle::DashDot => "Dash dot",
            LineStyle::DashDotDot => "Dash dot dot",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub color: Rgba,
    /// Width in points. 0 means hairline.
    pub width: f64,
    #[serde(default)]
    pub style: LineStyle,
    #[serde(default)]
    pub join: LineJoin,
    #[serde(default)]
    pub cap: LineCap,
    /// Draw the outline under the fill, so only its outer half shows.
    #[serde(default)]
    pub behind_fill: bool,
    /// Scale the width when the object is scaled.
    #[serde(default)]
    pub scale_with_object: bool,
}

impl Outline {
    /// Width drawn for a hairline (0) outline, in points.
    pub const HAIRLINE: f64 = 0.25;
    /// Miter joins sharper than this ratio of length to width fall back to bevels.
    pub const MITER_LIMIT: f64 = 10.0;

    /// A solid outline with mitered corners and butt ends.
    pub fn new(color: Rgba, width: f64) -> Self {
        Self {
            color,
            width,
            style: LineStyle::Solid,
            join: LineJoin::Miter,
            cap: LineCap::Butt,
            behind_fill: false,
            scale_with_object: false,
        }
    }

    pub fn effective_width(&self) -> f64 {
        if self.width > 0.0 { self.width } else { Self::HAIRLINE }
    }

    /// Width in page space for an object whose local → page transform is `transform`.
    /// Only "scale with object" outlines grow with it; the scale used is the geometric
    /// mean of the two axis scales.
    pub fn page_width(&self, transform: Affine) -> f64 {
        let w = self.effective_width();
        if self.scale_with_object { w * transform.determinant().abs().sqrt() } else { w }
    }

    /// Dash lengths in points for a stroke `width` wide; empty for solid.
    pub fn dashes(&self, width: f64) -> Vec<f64> {
        self.style.pattern().iter().map(|k| k * width).collect()
    }

    /// The stroke to draw this outline `width` points wide.
    pub fn stroke(&self, width: f64) -> kurbo::Stroke {
        let join = match self.join {
            LineJoin::Miter => kurbo::Join::Miter,
            LineJoin::Round => kurbo::Join::Round,
            LineJoin::Bevel => kurbo::Join::Bevel,
        };
        let cap = match self.cap {
            LineCap::Butt => kurbo::Cap::Butt,
            LineCap::Round => kurbo::Cap::Round,
            LineCap::Square => kurbo::Cap::Square,
        };
        kurbo::Stroke::new(width)
            .with_join(join)
            .with_miter_limit(Self::MITER_LIMIT)
            .with_caps(cap)
            .with_dashes(0.0, self.dashes(width))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Fill>,
    pub outline: Option<Outline>,
    /// Uniform transparency of the whole object: 0 is opaque, 1 invisible.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub transparency: f64,
    /// Print the fill over what is below it instead of knocking it out (the inks mix).
    #[serde(default, skip_serializing_if = "is_false")]
    pub overprint_fill: bool,
    /// Print the outline over what is below it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub overprint_outline: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Style {
    /// Style for newly drawn objects: no fill, black 0.5 pt outline.
    pub fn new_object() -> Self {
        Self::outlined(Outline::new(Rgba::BLACK, 0.5))
    }

    /// Style for new text: black fill, no outline.
    pub fn new_text() -> Self {
        Self::filled(Rgba::BLACK)
    }

    /// A uniform fill and no outline.
    pub fn filled(color: Rgba) -> Self {
        Self { fill: Some(Fill::Solid(color)), ..Self::default() }
    }

    /// An outline and no fill.
    pub fn outlined(outline: Outline) -> Self {
        Self { outline: Some(outline), ..Self::default() }
    }

    /// Whether `part` overprints.
    pub fn overprints(&self, part: OverprintPart) -> bool {
        match part {
            OverprintPart::Fill => self.overprint_fill,
            OverprintPart::Outline => self.overprint_outline,
        }
    }

    pub fn set_overprint(&mut self, part: OverprintPart, on: bool) {
        match part {
            OverprintPart::Fill => self.overprint_fill = on,
            OverprintPart::Outline => self.overprint_outline = on,
        }
    }

    /// How opaque the object is: 1 − transparency.
    pub fn opacity(&self) -> f64 {
        1.0 - self.transparency.clamp(0.0, 1.0)
    }
}

/// The part of an object that Object > Overprint Fill / Overprint Outline sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverprintPart {
    Fill,
    Outline,
}

/// Whether a cut ellipse is closed through its centre (pie) or left open (arc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArcKind {
    Pie,
    Arc,
}

/// The part of an ellipse a pie or arc keeps: from `start` counterclockwise to `end`.
/// Angles are in degrees, counterclockwise on the page from the 3 o'clock direction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EllipseArc {
    pub kind: ArcKind,
    pub start: f64,
    pub end: f64,
}

impl EllipseArc {
    /// Angles used when Pie or Arc is chosen for a whole ellipse: three quarters, open at
    /// the top right.
    pub fn default_for(kind: ArcKind) -> Self {
        Self { kind, start: 90.0, end: 0.0 }
    }

    /// Degrees covered going counterclockwise from `start` to `end`, in (0, 360].
    pub fn sweep(&self) -> f64 {
        let s = (self.end - self.start).rem_euclid(360.0);
        if s < 1e-9 { 360.0 } else { s }
    }

    /// The same shape drawn the other way round: keeps the part that was cut away.
    pub fn reversed(self) -> Self {
        Self { start: self.end, end: self.start, ..self }
    }
}

/// Geometry in the object's local space. The object's `transform` maps it to page space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Rectangle { rect: Rect, corner_radius: f64 },
    Ellipse {
        rect: Rect,
        /// A pie or arc cut from the ellipse; `None` is the whole ellipse.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arc: Option<EllipseArc>,
    },
    /// Regular polygon inscribed in `rect`, first vertex at the top.
    Polygon { rect: Rect, sides: u32 },
    /// Star inscribed in `rect`; inner vertices sit at `inner_ratio` of the outer radius.
    Star { rect: Rect, points: u32, inner_ratio: f64 },
    Curve { path: BezPath },
    /// Artistic text, drawn as glyph outlines.
    Text(Text),
    /// A placed bitmap.
    Image(Bitmap),
    /// Objects moved, scaled and styled as one. Child transforms are relative to the group.
    Group { children: Vec<Object> },
}

impl Shape {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Shape::Rectangle { .. } => "Rectangle",
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Polygon { .. } => "Polygon",
            Shape::Star { .. } => "Star",
            Shape::Curve { .. } => "Curve",
            Shape::Text(t) if t.is_paragraph() => "Paragraph Text",
            Shape::Text(_) => "Artistic Text",
            Shape::Image(_) => "Bitmap",
            Shape::Group { .. } => "Group",
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
    /// Locked objects can be selected but not moved, edited, restyled or deleted.
    #[serde(default)]
    pub locked: bool,
    /// The page layer a top-level object is on (unused inside groups).
    #[serde(default)]
    pub layer: LayerId,
    /// PowerClip contents, bottom first, drawn only inside this object's outline. Their
    /// transforms are relative to this object's, so they move with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<Object>,
    /// A clone follows this master's shape, fill and outline until it is edited itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_of: Option<ObjectId>,
    /// A PowerClip frame made with Create Empty Frame: it stays a frame while empty, and
    /// objects dropped on it go inside.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub frame: bool,
    /// A symbol instance: a group whose members are a copy of this symbol's objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<crate::symbols::SymbolId>,
    /// A symmetry group: its members are drawn again, mirrored (see [`crate::symmetry`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symmetry: Option<crate::symmetry::Symmetry>,
    /// Drawn through a perspective (see [`crate::perspective`]), in its own space before
    /// `transform`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perspective: Option<crate::perspective::Perspective>,
    /// A table: this group's members are its cells (see [`crate::table`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<crate::table::Table>,
    /// A drop shadow drawn under the object (see [`crate::shadow`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<crate::shadow::DropShadow>,
    /// Contour, blend, extrude, block shadow, envelope, distortion and mesh fill (see
    /// [`crate::effects`]).
    #[serde(default, skip_serializing_if = "crate::effects::Effects::is_empty")]
    pub effects: crate::effects::Effects,
}

impl Object {
    /// A plain, unlocked object on no particular layer, placed by `shape` alone.
    pub fn new(id: ObjectId, shape: Shape, style: Style) -> Self {
        Self { id, name: None, shape, style, transform: Affine::IDENTITY, locked: false, layer: LayerId::default(), contents: Vec::new(), clone_of: None, frame: false, symbol: None, symmetry: None, perspective: None, table: None, shadow: None, effects: Default::default() }
    }

    /// Turn this object, or every member of this group, into curves. Locked objects
    /// stay as they are.
    pub fn convert_to_curves(&mut self) {
        if self.locked {
            return;
        }
        // A perspective, envelope or distortion becomes part of the curves.
        self.bake_warps();
        match &mut self.shape {
            Shape::Curve { .. } | Shape::Image(_) => {}
            Shape::Group { children } => {
                // Curves are no longer a table's cells.
                self.table = None;
                children.iter_mut().for_each(Object::convert_to_curves);
            }
            shape => *shape = Shape::Curve { path: shape.to_path() },
        }
    }

    /// Whether Convert to Curves would change anything.
    pub fn can_convert_to_curves(&self) -> bool {
        !self.locked
            && (self.is_warped()
                || self.effects.media.is_some()
                || match &self.shape {
                    Shape::Curve { .. } | Shape::Image(_) => false,
                    Shape::Group { children } => children.iter().any(Object::can_convert_to_curves),
                    _ => true,
                })
    }

    pub fn label(&self) -> String {
        match &self.name {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => match &self.shape {
                Shape::Group { .. } if self.symbol.is_some() => "Symbol".into(),
                Shape::Group { .. } if self.symmetry.is_some() => "Symmetry".into(),
                Shape::Group { .. } if self.table().is_some() => "Table".into(),
                Shape::Group { children } => format!("Group of {} objects", children.len()),
                shape if !self.contents.is_empty() => format!("PowerClip {}", shape.kind_label()),
                shape if self.frame => format!("Empty PowerClip {}", shape.kind_label()),
                shape if self.clone_of.is_some() => format!("Clone {}", shape.kind_label()),
                shape => shape.kind_label().to_string(),
            },
        }
    }

    /// This object's id and the ids of everything nested in it.
    pub fn all_ids(&self) -> Vec<ObjectId> {
        let mut out = vec![self.id];
        if let Shape::Group { children } = &self.shape {
            out.extend(children.iter().flat_map(Object::all_ids));
        }
        out.extend(self.contents.iter().flat_map(Object::all_ids));
        out
    }

    /// Apply `f` to this object's style, or to every object inside it if it is a group
    /// (a table's cell rectangles, not their text).
    pub fn restyle(&mut self, f: &mut impl FnMut(&mut Style)) {
        let table = self.table().is_some();
        match &mut self.shape {
            Shape::Group { children } => children.iter_mut().step_by(if table { 2 } else { 1 }).for_each(|c| c.restyle(f)),
            _ => f(&mut self.style),
        }
    }

    /// The styles [`Object::restyle`] changes.
    pub fn styles(&self) -> Vec<&Style> {
        match &self.shape {
            Shape::Group { children } => {
                let step = if self.table().is_some() { 2 } else { 1 };
                children.iter().step_by(step).flat_map(Object::styles).collect()
            }
            _ => vec![&self.style],
        }
    }

    /// Like [`Object::restyle`], also passing each object's local frame (for fills that
    /// are placed on the object, like fountain fills).
    pub fn restyle_framed(&mut self, f: &mut impl FnMut(&mut Style, Rect)) {
        let table = self.table().is_some();
        if let Shape::Group { children } = &mut self.shape {
            children.iter_mut().step_by(if table { 2 } else { 1 }).for_each(|c| c.restyle_framed(f));
        } else {
            let frame = self.shape.frame();
            f(&mut self.style, frame);
        }
    }

    /// The style shown for this object: its own, or the first leaf's for a group.
    pub fn shown_style(&self) -> &Style {
        match &self.shape {
            Shape::Group { children } => children.first().map_or(&self.style, Object::shown_style),
            _ => &self.style,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<crate::snap::Guide>,
    /// Bottom first; never empty once settled.
    #[serde(default)]
    pub(crate) layers: Vec<Layer>,
    #[serde(default)]
    pub(crate) active_layer: LayerId,
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

/// A colour under the objects of every page, in place of white paper.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Background {
    pub color: Rgba,
    /// Print and export it with the drawing; otherwise it only shows on screen.
    pub print_and_export: bool,
}

/// File > Document Properties: what the drawing is, for people and other apps. Title,
/// subject, author, copyright and keywords are exported to SVG and PDF; notes stay in
/// the drawing's own file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentProperties {
    pub title: String,
    pub subject: String,
    pub author: String,
    pub copyright: String,
    /// Separated by commas or semicolons.
    pub keywords: String,
    pub notes: String,
}

impl DocumentProperties {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The keywords one by one, trimmed, without empty ones.
    pub fn keyword_list(&self) -> Vec<String> {
        self.keywords.split([',', ';']).map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned).collect()
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
    pub(crate) pages: Vec<Page>,
    active_page: usize,
    next_id: u64,
    /// The page background; `None` is plain white paper that is not exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default, skip_serializing_if = "DocumentProperties::is_empty")]
    pub properties: DocumentProperties,
    /// Symbol definitions, which symbol instances copy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<crate::symbols::Symbol>,
    /// Print Merge data: the fields text can hold, and the records merged into them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<crate::merge::MergeData>,
    /// The CMYK profile and rendering intent for CMYK values and output.
    #[serde(default, skip_serializing_if = "crate::ColorSettings::is_default")]
    pub color: crate::ColorSettings,
    /// What each clone last took from its master (see [`Document::sync_clones`]).
    #[serde(skip)]
    pub(crate) clone_sync: crate::clones::CloneSync,
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
            background: None,
            properties: DocumentProperties::default(),
            symbols: Vec::new(),
            merge: None,
            color: Default::default(),
            clone_sync: Default::default(),
        };
        let size = PAGE_PRESETS[1].size(Orientation::Portrait);
        let page = doc.make_page(size);
        doc.pages.push(page);
        doc
    }

    pub(crate) fn alloc_id(&mut self) -> u64 {
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
            guides: Vec::new(),
            layers: vec![Layer::new(LayerId(1), "Layer 1")],
            active_layer: LayerId(1),
        }
    }

    /// Whether `other` has the same drawing: pages and document settings, ignoring view
    /// state such as the active page. Undo steps and unsaved changes follow this.
    pub fn same_content(&self, other: &Document) -> bool {
        self.pages == other.pages && self.background == other.background && self.properties == other.properties && self.merge == other.merge
    }

    /// The background colour to print and export, if there is one.
    pub fn exported_background(&self) -> Option<Rgba> {
        self.background.filter(|b| b.print_and_export).map(|b| b.color)
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
            self.renumber(obj);
        }
        self.pages.insert(index + 1, copy);
        self.active_page = index + 1;
        Ok(index + 1)
    }

    /// Move page `from` so it becomes page `to`; the others close up. The active page
    /// stays the same page.
    pub fn move_page(&mut self, from: usize, to: usize) -> Result<(), DocumentError> {
        let n = self.pages.len();
        for i in [from, to] {
            if i >= n {
                return Err(DocumentError::PageOutOfRange(i));
            }
        }
        let active = self.pages[self.active_page].id;
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        self.active_page = self.pages.iter().position(|p| p.id == active).unwrap_or(0);
        Ok(())
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

    /// Add an object on top of the active layer of the active page.
    pub fn add_object(&mut self, shape: Shape, style: Style) -> ObjectId {
        let id = ObjectId(self.alloc_id());
        let page = self.active_page_mut();
        let layer = page.active_layer;
        page.objects.push(Object { id, name: None, shape, style, transform: Affine::IDENTITY, locked: false, layer, contents: Vec::new(), clone_of: None, frame: false, symbol: None, symmetry: None, perspective: None, table: None, shadow: None, effects: Default::default() });
        page.settle_layers();
        id
    }

    /// Restore the layer invariants of every page (see [`Page::settle_layers`]). Edits
    /// that add or restack objects may break them; the app calls this once they finish.
    pub fn settle_layers(&mut self) {
        self.pages.iter_mut().for_each(Page::settle_layers);
    }

    /// Give `obj` and everything nested in it fresh ids.
    pub(crate) fn renumber(&mut self, obj: &mut Object) {
        obj.id = self.alloc_object_id();
        if let Shape::Group { children } = &mut obj.shape {
            for child in children {
                self.renumber(child);
            }
        }
        for content in &mut obj.contents {
            self.renumber(content);
        }
    }

    pub(crate) fn alloc_object_id(&mut self) -> ObjectId {
        ObjectId(self.alloc_id())
    }

    pub(crate) fn alloc_symbol_id(&mut self) -> crate::symbols::SymbolId {
        crate::symbols::SymbolId(self.alloc_id())
    }

    /// The id the next new object gets: objects made from now on have this one or higher.
    pub fn next_object_id(&self) -> ObjectId {
        ObjectId(self.next_id)
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
        self.settle_layers();
        self.sync_page_numbers();
        let max = self
            .pages
            .iter()
            .flat_map(|p| std::iter::once(p.id.0).chain(p.objects.iter().flat_map(Object::all_ids).map(|id| id.0)))
            .chain(self.symbols.iter().flat_map(|s| std::iter::once(s.id.0).chain(s.objects.iter().flat_map(Object::all_ids).map(|id| id.0))))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_dashes_follow_width_and_scale_option() {
        let mut o = Outline::new(Rgba::BLACK, 2.0);
        assert!(o.dashes(2.0).is_empty());
        o.style = LineStyle::DashDot;
        assert_eq!(o.dashes(2.0), vec![8.0, 4.0, 2.0, 4.0]);
        let doubled = Affine::scale(2.0);
        assert_eq!(o.page_width(doubled), 2.0);
        o.scale_with_object = true;
        assert!((o.page_width(doubled) - 4.0).abs() < 1e-12);
        o.width = 0.0;
        assert!((o.page_width(Affine::IDENTITY) - Outline::HAIRLINE).abs() < 1e-12);
    }

    #[test]
    fn old_files_without_new_outline_fields_still_load() {
        let json = r#"{"color":{"r":0,"g":0,"b":0,"a":255},"width":1.5}"#;
        let o: Outline = serde_json::from_str(json).expect("parses");
        assert_eq!(o, Outline::new(Rgba::BLACK, 1.5));
    }

    #[test]
    fn ellipses_load_without_an_arc_and_save_pies_and_arcs() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let old: Shape = serde_json::from_str(r#"{"Ellipse":{"rect":[0.0,0.0,10.0,10.0]}}"#).expect("parses");
        assert_eq!(old, Shape::Ellipse { rect, arc: None });
        let pie = Shape::Ellipse { rect, arc: Some(EllipseArc { kind: ArcKind::Pie, start: 30.0, end: 300.0 }) };
        let back: Shape = serde_json::from_str(&serde_json::to_string(&pie).unwrap()).unwrap();
        assert_eq!(back, pie);
        assert_eq!(EllipseArc { kind: ArcKind::Arc, start: 300.0, end: 30.0 }.sweep(), 90.0);
    }

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
            Shape::Ellipse { rect: Rect::new(0.0, 0.0, 10.0, 10.0), arc: None },
            Style::default(),
        );
        doc.duplicate_page(0).unwrap();
        let copy = &doc.pages()[1];
        assert_eq!(copy.objects.len(), 1);
        assert_ne!(copy.objects[0].id, original);
        assert_ne!(copy.id, doc.pages()[0].id);
    }

    #[test]
    fn moving_a_page_keeps_the_active_page() {
        let mut doc = Document::default();
        doc.insert_page_after(0).unwrap();
        doc.insert_page_after(1).unwrap();
        let ids: Vec<PageId> = doc.pages().iter().map(|p| p.id).collect();
        doc.set_active_page(1).unwrap();
        doc.move_page(0, 2).unwrap();
        assert_eq!(doc.pages().iter().map(|p| p.id).collect::<Vec<_>>(), vec![ids[1], ids[2], ids[0]]);
        assert_eq!(doc.active_page_index(), 0, "still the same page");
        doc.move_page(2, 0).unwrap();
        assert_eq!(doc.pages()[0].id, ids[0]);
        assert_eq!(doc.move_page(0, 3), Err(DocumentError::PageOutOfRange(3)));
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
    fn colours_read_and_write_hex() {
        let c = Rgba::rgb(0x12, 0xab, 0xff);
        assert_eq!(c.to_hex(), "#12abff");
        assert_eq!(Rgba::from_hex("#12ABFF"), Some(c));
        assert_eq!(Rgba::from_hex(" 12abff "), Some(c));
        assert_eq!(Rgba::from_hex("#12abf"), None);
        assert_eq!(Rgba::from_hex("#12abfg"), None);
        assert_eq!(Rgba::from_hex("#12abfé"), None);
    }

    #[test]
    fn the_background_is_content_and_exported_only_when_asked() {
        let mut doc = Document::default();
        let before = doc.clone();
        assert_eq!(doc.exported_background(), None);
        doc.background = Some(Background { color: Rgba::rgb(1, 2, 3), print_and_export: false });
        assert!(!doc.same_content(&before), "an undo step and an unsaved change");
        assert_eq!(doc.exported_background(), None, "screen only");
        doc.background = Some(Background { color: Rgba::rgb(1, 2, 3), print_and_export: true });
        assert_eq!(doc.exported_background(), Some(Rgba::rgb(1, 2, 3)));
        let mut moved = before.clone();
        moved.insert_page_after(0).unwrap();
        moved.set_active_page(0).unwrap();
        assert!(!moved.same_content(&before));
    }

    #[test]
    fn properties_split_keywords_and_count_as_content() {
        let mut doc = Document::default();
        let before = doc.clone();
        doc.properties.keywords = " logo, blue ;; print ".into();
        assert_eq!(doc.properties.keyword_list(), vec!["logo", "blue", "print"]);
        assert!(!doc.same_content(&before));
        let json = serde_json::to_string(&before).unwrap();
        assert!(!json.contains("properties"), "left out of files while empty");
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

//! Artistic text: fonts, line layout and glyph outlines.
//!
//! Text is laid out one glyph per character with the font's advance widths (no kerning or
//! complex shaping) and drawn as glyph outlines, so rendering, hit-testing, export and
//! Convert to Curves all work on the same path as every other shape.
//!
//! Fonts come from the bundled Ubuntu Light face plus the system font folders, which are
//! scanned the first time a family that is not bundled, or the family list, is needed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use kurbo::{BezPath, Point, Rect};
use serde::{Deserialize, Serialize};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// How the lines of a text object line up with its origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Center => "Center",
            TextAlign::Right => "Right",
        }
    }
}

/// Artistic text. In local space the first line's baseline runs along y = 0, and x = 0
/// is where lines start (left), meet in the middle (center) or end (right).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub content: String,
    pub family: String,
    pub style: String,
    /// Font size in points.
    pub size: f64,
    #[serde(default)]
    pub align: TextAlign,
}

impl Text {
    /// Default size for new text, in points.
    pub const DEFAULT_SIZE: f64 = 24.0;

    /// Empty text in the default font.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            family: default_family().to_string(),
            style: default_style().to_string(),
            size: Self::DEFAULT_SIZE,
            align: TextAlign::Left,
        }
    }

    pub fn layout(&self) -> TextLayout {
        layout(self)
    }

    /// Glyph outlines in local space.
    pub fn to_path(&self) -> BezPath {
        let key = (self.content.clone(), self.family.clone(), self.style.clone(), self.size.to_bits(), self.align);
        let cache = PATHS.get_or_init(Mutex::default);
        if let Some(path) = cache.lock().expect("text cache").get(&key) {
            return path.clone();
        }
        let path = self.layout().path;
        let mut cache = cache.lock().expect("text cache");
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, path.clone());
        path
    }
}

type PathKey = (String, String, String, u64, TextAlign);
static PATHS: OnceLock<Mutex<HashMap<PathKey, BezPath>>> = OnceLock::new();

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub baseline: f64,
    /// x of every caret position on the line: before each character, then after the last.
    pub carets: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub path: BezPath,
    pub lines: Vec<Line>,
    /// Height above the baseline (positive).
    pub ascent: f64,
    /// Depth below the baseline (positive).
    pub descent: f64,
}

impl TextLayout {
    /// The area the lines take, from the top of the first line's ascent to the bottom of
    /// the last line's descent. Empty text still has a caret-wide box.
    pub fn bounds(&self) -> Rect {
        let mut r: Option<Rect> = None;
        for line in &self.lines {
            let (x0, x1) = (line.carets[0], *line.carets.last().expect("a line has a caret"));
            let b = Rect::new(x0.min(x1), line.baseline - self.ascent, x0.max(x1), line.baseline + self.descent);
            r = Some(r.map_or(b, |r| r.union(b)));
        }
        r.unwrap_or_default()
    }

    /// Top and bottom of the caret before character `index` (counting `\n`), local space.
    pub fn caret(&self, index: usize) -> (Point, Point) {
        let mut left = index;
        for line in &self.lines {
            if left < line.carets.len() {
                let x = line.carets[left];
                return (Point::new(x, line.baseline - self.ascent), Point::new(x, line.baseline + self.descent));
            }
            left -= line.carets.len();
        }
        let line = self.lines.last().expect("at least one line");
        let x = *line.carets.last().expect("a line has a caret");
        (Point::new(x, line.baseline - self.ascent), Point::new(x, line.baseline + self.descent))
    }

    /// The caret index (counting `\n`) closest to `p`, local space.
    pub fn index_at(&self, p: Point) -> usize {
        let line_height = self.ascent + self.descent;
        let row = self
            .lines
            .iter()
            .position(|l| p.y < l.baseline + self.descent && p.y >= l.baseline - self.ascent.max(line_height))
            .unwrap_or(if p.y < 0.0 { 0 } else { self.lines.len() - 1 });
        let before: usize = self.lines[..row].iter().map(|l| l.carets.len()).sum();
        let carets = &self.lines[row].carets;
        let col = (0..carets.len())
            .min_by(|a, b| (carets[*a] - p.x).abs().total_cmp(&(carets[*b] - p.x).abs()))
            .unwrap_or(0);
        before + col
    }
}

fn layout(text: &Text) -> TextLayout {
    let face = library().resolve(&text.family, &text.style);
    let data = face.data();
    let font = FontRef::from_index(&data, face.index).expect("resolved fonts parse");
    let size = Size::new(text.size as f32);
    let location = LocationRef::default();
    let metrics = font.metrics(size, location);
    let ascent = f64::from(metrics.ascent);
    let descent = -f64::from(metrics.descent);
    let line_height = ascent + descent + f64::from(metrics.leading);
    let charmap = font.charmap();
    let advances = font.glyph_metrics(size, location);
    let outlines = font.outline_glyphs();

    let mut path = BezPath::new();
    let mut lines = Vec::new();
    for (row, line) in text.content.split('\n').enumerate() {
        let baseline = row as f64 * line_height;
        let glyphs: Vec<(GlyphId, f64)> = line
            .chars()
            .map(|ch| {
                let gid = charmap.map(ch).unwrap_or(GlyphId::NOTDEF);
                (gid, f64::from(advances.advance_width(gid).unwrap_or(0.0)))
            })
            .collect();
        let width: f64 = glyphs.iter().map(|g| g.1).sum();
        let mut x = match text.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -width / 2.0,
            TextAlign::Right => -width,
        };
        let mut carets = vec![x];
        for (gid, advance) in glyphs {
            if let Some(glyph) = outlines.get(gid) {
                let mut pen = Pen { path: &mut path, origin: Point::new(x, baseline) };
                // A glyph that fails to draw is left blank rather than failing the text.
                let _ = glyph.draw(DrawSettings::unhinted(size, location), &mut pen);
            }
            x += advance;
            carets.push(x);
        }
        lines.push(Line { baseline, carets });
    }
    TextLayout { path, lines, ascent, descent }
}

/// Draws font units (y up) into a page-space path (y down) at `origin`.
struct Pen<'a> {
    path: &'a mut BezPath,
    origin: Point,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Point {
        Point::new(self.origin.x + f64::from(x), self.origin.y - f64::from(y))
    }
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.move_to(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.line_to(p);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.at(cx0, cy0), self.at(x, y));
        self.path.quad_to(c, p);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        self.path.curve_to(c0, c1, p);
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

// ---------------------------------------------------------------------------------------
// Fonts

const BUNDLED: &[u8] = epaint_default_fonts::UBUNTU_LIGHT;

/// Folders scanned for system fonts.
const FONT_DIRS: &[&str] = &["/System/Library/Fonts", "/Library/Fonts", "~/Library/Fonts", "/usr/share/fonts"];

#[derive(Clone)]
enum Source {
    Bundled,
    File(PathBuf),
}

#[derive(Clone)]
struct Face {
    family: String,
    style: String,
    source: Source,
    index: u32,
}

/// Font bytes: bundled, or a memory-mapped file.
enum Data {
    Static(&'static [u8]),
    Mapped(Arc<memmap2::Mmap>),
}

impl std::ops::Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Data::Static(b) => b,
            Data::Mapped(m) => m,
        }
    }
}

impl Face {
    fn data(&self) -> Data {
        match &self.source {
            Source::Bundled => Data::Static(BUNDLED),
            Source::File(path) => match map(path) {
                Some(m) => Data::Mapped(m),
                None => Data::Static(BUNDLED),
            },
        }
    }
}

struct Library {
    bundled: Face,
    system: OnceLock<Vec<Face>>,
    mapped: Mutex<HashMap<PathBuf, Arc<memmap2::Mmap>>>,
}

fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let font = FontRef::from_index(BUNDLED, 0).expect("bundled font parses");
        let (family, style) = names(&font);
        Library {
            bundled: Face { family, style, source: Source::Bundled, index: 0 },
            system: OnceLock::new(),
            mapped: Mutex::default(),
        }
    })
}

fn map(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    let lib = library();
    let mut mapped = lib.mapped.lock().expect("font map");
    if let Some(m) = mapped.get(path) {
        return Some(m.clone());
    }
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: font files in the system font folders are not modified while mapped; if one
    // were, glyphs would draw wrongly but parsing is bounds-checked.
    let m = Arc::new(unsafe { memmap2::Mmap::map(&file) }.ok()?);
    mapped.insert(path.to_path_buf(), m.clone());
    Some(m)
}

/// Family and style names, preferring the typographic ones (name IDs 16 and 17).
fn names(font: &FontRef) -> (String, String) {
    let get = |id| font.localized_strings(id).english_or_first().map(|s| s.to_string()).filter(|s| !s.is_empty());
    let family = get(StringId::TYPOGRAPHIC_FAMILY_NAME).or_else(|| get(StringId::FAMILY_NAME)).unwrap_or_default();
    let style = get(StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| get(StringId::SUBFAMILY_NAME))
        .unwrap_or_else(|| "Regular".into());
    (family, style)
}

impl Library {
    fn system(&self) -> &[Face] {
        self.system.get_or_init(scan_system_fonts)
    }

    fn faces(&self) -> impl Iterator<Item = &Face> {
        std::iter::once(&self.bundled).chain(self.system().iter())
    }

    /// The face for `family` and `style`: that style, else the family's Regular, else its
    /// first face, else the bundled font.
    fn resolve(&self, family: &str, style: &str) -> Face {
        if family == self.bundled.family {
            return self.bundled.clone();
        }
        let in_family: Vec<&Face> = self.system().iter().filter(|f| f.family == family).collect();
        in_family
            .iter()
            .find(|f| f.style == style)
            .or_else(|| in_family.iter().find(|f| f.style == "Regular"))
            .or_else(|| in_family.first())
            .map_or_else(|| self.bundled.clone(), |f| (*f).clone())
    }
}

fn scan_system_fonts() -> Vec<Face> {
    let mut out = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = FONT_DIRS
        .iter()
        .filter_map(|d| match d.strip_prefix("~/") {
            Some(rest) => home.as_ref().map(|h| h.join(rest)),
            None => Some(PathBuf::from(d)),
        })
        .collect();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
                continue;
            }
            let Some(data) = map(&path) else { continue };
            let fonts: Vec<(u32, FontRef)> = match FileRef::new(&data) {
                Ok(FileRef::Font(f)) => vec![(0, f)],
                Ok(FileRef::Collection(c)) => (0..c.len()).filter_map(|i| c.get(i).ok().map(|f| (i, f))).collect(),
                Err(_) => continue,
            };
            for (index, font) in fonts {
                // Only fonts with outlines can be drawn.
                if font.outline_glyphs().get(GlyphId::new(0)).is_none() && font.charmap().map('A').is_none() {
                    continue;
                }
                let (family, style) = names(&font);
                if family.is_empty() || family.starts_with('.') {
                    continue;
                }
                out.push(Face { family, style, source: Source::File(path.clone()), index });
            }
        }
    }
    // Keep the mapping table small: faces are mapped again when used.
    library().mapped.lock().expect("font map").clear();
    out
}

/// The bundled font's family, used for new text and when a file's font is missing.
pub fn default_family() -> &'static str {
    &library().bundled.family
}

pub fn default_style() -> &'static str {
    &library().bundled.style
}

/// Every font family, sorted, the bundled one included.
pub fn families() -> Vec<String> {
    let mut out: Vec<String> = library().faces().map(|f| f.family.clone()).collect();
    out.sort_by_key(|f| f.to_lowercase());
    out.dedup();
    out
}

/// The styles `family` comes in, Regular first.
pub fn styles(family: &str) -> Vec<String> {
    let lib = library();
    let mut out: Vec<String> = if family == lib.bundled.family {
        vec![lib.bundled.style.clone()]
    } else {
        lib.system().iter().filter(|f| f.family == family).map(|f| f.style.clone()).collect()
    };
    out.sort_by_key(|s| (s != "Regular", s.clone()));
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    fn text(content: &str) -> Text {
        Text::new(content)
    }

    #[test]
    fn bundled_font_is_the_default_and_draws_glyphs() {
        assert!(!default_family().is_empty());
        let t = text("Hi");
        let b = t.to_path().bounding_box();
        assert!(b.width() > 5.0, "{b:?}");
        // Capitals sit on the baseline and rise above it.
        assert!(b.y1 <= 0.5 && b.y0 < -10.0, "{b:?}");
    }

    #[test]
    fn lines_stack_and_align_about_the_origin() {
        let mut t = text("ab\nlonger line");
        let l = t.layout();
        assert_eq!(l.lines.len(), 2);
        assert!(l.lines[1].baseline > l.ascent, "second line below the first");
        assert_eq!(l.lines[0].carets.len(), 3);
        assert_eq!(l.lines[0].carets[0], 0.0);

        t.align = TextAlign::Right;
        let r = t.layout();
        assert!(r.lines.iter().all(|l| l.carets.last().unwrap().abs() < 1e-9), "lines end at 0");
        t.align = TextAlign::Center;
        let c = t.layout();
        let line = &c.lines[1].carets;
        assert!((line[0] + line.last().unwrap()).abs() < 1e-9, "centred on 0");
    }

    #[test]
    fn size_scales_the_outlines() {
        let mut t = text("M");
        let small = t.to_path().bounding_box();
        t.size *= 2.0;
        let big = t.to_path().bounding_box();
        assert!((big.height() / small.height() - 2.0).abs() < 1e-3, "glyphs are scaled in f32");
    }

    #[test]
    fn carets_and_hits_round_trip() {
        let t = text("abc\nde");
        let l = t.layout();
        // Index 4 is the start of the second line ("abc" + "\n").
        let (top, bottom) = l.caret(4);
        assert_eq!(top.x, 0.0);
        assert!(bottom.y > l.lines[1].baseline);
        let mid = Point::new(l.lines[1].carets[1] + 0.1, l.lines[1].baseline - 2.0);
        assert_eq!(l.index_at(mid), 5);
        assert_eq!(l.index_at(Point::new(-50.0, -5.0)), 0);
        assert_eq!(l.caret(99), l.caret(6), "past the end clamps to the end");
    }

    #[test]
    fn empty_text_has_a_box_but_no_outline() {
        let l = text("").layout();
        assert!(l.path.is_empty());
        let b = l.bounds();
        assert!(b.height() > 0.0 && b.width() == 0.0);
    }

    #[test]
    fn unknown_family_falls_back_to_the_bundled_font() {
        let mut t = text("A");
        t.family = "No Such Font 12345".into();
        assert_eq!(t.to_path(), text("A").to_path());
    }
}

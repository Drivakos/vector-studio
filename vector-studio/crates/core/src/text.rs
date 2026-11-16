//! Artistic and paragraph text: fonts, line layout and glyph outlines.
//!
//! Artistic text breaks lines only at `\n`. Paragraph text sits in a frame: words wrap
//! to its width, and lines below its bottom are hidden (the text overflows). Text fitted
//! to a path runs along a copy of that path, one glyph after another.
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

/// Text. Artistic text (no frame): in local space the first line's baseline runs along
/// y = 0, and x = 0 is where lines start (left), meet in the middle (center) or end
/// (right). Paragraph text: the frame's top-left corner is the origin, and lines are
/// aligned within its width.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub content: String,
    pub family: String,
    pub style: String,
    /// Font size in points.
    pub size: f64,
    #[serde(default)]
    pub align: TextAlign,
    /// The paragraph frame's size; `None` for artistic text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<kurbo::Size>,
    /// Artistic text fitted to a path: the path in this text's local space. The glyphs
    /// sit on it, aligned to its start, middle or end by `align`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_path: Option<BezPath>,
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
            frame: None,
            on_path: None,
        }
    }

    pub fn is_paragraph(&self) -> bool {
        self.frame.is_some()
    }

    /// The same text as paragraph text, in a frame that just fits its lines, and the
    /// local shift that keeps it in place (new transform = old transform × shift).
    /// Paragraph text is returned unchanged.
    pub fn to_paragraph(&self) -> (Text, kurbo::Affine) {
        if self.is_paragraph() {
            return (self.clone(), kurbo::Affine::IDENTITY);
        }
        let b = self.layout().bounds();
        // A hair of slack, so the longest line does not wrap on rounding.
        let frame = kurbo::Size::new(b.width() + 0.01, b.height() + 0.01);
        (Text { frame: Some(frame), ..self.clone() }, kurbo::Affine::translate(b.origin().to_vec2()))
    }

    /// The same text as artistic text, broken into lines where the frame wrapped it, and
    /// the local shift that keeps it in place. Artistic text is returned unchanged.
    pub fn to_artistic(&self) -> (Text, kurbo::Affine) {
        let Some(frame) = self.frame else { return (self.clone(), kurbo::Affine::IDENTITY) };
        let layout = self.layout();
        let chars: Vec<char> = self.content.chars().collect();
        let mut content = String::new();
        for (i, line) in layout.lines.iter().enumerate() {
            let n = line.carets.len() - 1;
            let mut text: String = chars[line.start..line.start + n].iter().collect();
            let soft = layout.lines.get(i + 1).is_some_and(|next| next.start == line.start + n);
            if soft {
                text.truncate(text.trim_end_matches(' ').len());
            }
            if i > 0 {
                content.push('\n');
            }
            content.push_str(&text);
        }
        let x = match self.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => frame.width / 2.0,
            TextAlign::Right => frame.width,
        };
        (Text { content, frame: None, ..self.clone() }, kurbo::Affine::translate((x, layout.ascent)))
    }

    pub fn layout(&self) -> TextLayout {
        layout(self)
    }

    /// Glyph outlines in local space.
    pub fn to_path(&self) -> BezPath {
        let frame = self.frame.map(|f| (f.width.to_bits(), f.height.to_bits()));
        let guide = self.on_path.as_ref().map(BezPath::to_svg);
        let key = (self.content.clone(), self.family.clone(), self.style.clone(), self.size.to_bits(), self.align, frame, guide);
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

type PathKey = (String, String, String, u64, TextAlign, Option<(u64, u64)>, Option<String>);
static PATHS: OnceLock<Mutex<HashMap<PathKey, BezPath>>> = OnceLock::new();

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Index (in characters, counting `\n`) of the first character on the line.
    pub start: usize,
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
    /// Paragraph text: its frame in local space. Text on a path: the area its glyphs
    /// take (or the path's bounds while it is empty).
    pub frame: Option<Rect>,
    /// Paragraph text: some lines do not fit in the frame and are not drawn.
    pub overflow: bool,
}

impl TextLayout {
    /// Paragraph text: its frame. Artistic text: the area the lines take, from the top of
    /// the first line's ascent to the bottom of the last line's descent; empty text still
    /// has a caret-wide box.
    pub fn bounds(&self) -> Rect {
        if let Some(frame) = self.frame {
            return frame;
        }
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
        // A caret at a soft line break shows at the start of the next line.
        let line = self.lines.iter().rev().find(|l| l.start <= index).unwrap_or(&self.lines[0]);
        let col = (index - line.start.min(index)).min(line.carets.len() - 1);
        let x = line.carets[col];
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
        let before = self.lines[row].start;
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
    if let Some(guide) = &text.on_path {
        return layout_on_path(text, guide, &font, size, ascent, descent);
    }
    let frame = text.frame.map(|f| Rect::from_origin_size(Point::ORIGIN, f));
    // Paragraph text starts with the first line's ascent inside the top of the frame.
    let first_baseline = if frame.is_some() { ascent } else { 0.0 };

    let mut path = BezPath::new();
    let mut lines = Vec::new();
    let mut overflow = false;
    let mut start = 0;
    for paragraph in text.content.split('\n') {
        let glyphs: Vec<(char, GlyphId, f64)> = paragraph
            .chars()
            .map(|ch| {
                let gid = charmap.map(ch).unwrap_or(GlyphId::NOTDEF);
                (ch, gid, f64::from(advances.advance_width(gid).unwrap_or(0.0)))
            })
            .collect();
        let advance: Vec<f64> = glyphs.iter().map(|g| g.2).collect();
        let ranges = match frame {
            Some(f) => wrap(&glyphs.iter().map(|g| g.0).collect::<Vec<_>>(), &advance, f.width()),
            None => std::iter::once(0..glyphs.len()).collect(),
        };
        for range in ranges {
            let baseline = first_baseline + lines.len() as f64 * line_height;
            let chars = &glyphs[range.clone()];
            // Trailing spaces hang past the edge, so they do not count for alignment.
            let shown = chars.len() - chars.iter().rev().take_while(|g| g.0 == ' ').count();
            let width: f64 = chars[..shown].iter().map(|g| g.2).sum();
            let mut x = match (frame, text.align) {
                (None, TextAlign::Left) => 0.0,
                (None, TextAlign::Center) => -width / 2.0,
                (None, TextAlign::Right) => -width,
                (Some(_), TextAlign::Left) => 0.0,
                (Some(f), TextAlign::Center) => (f.width() - width) / 2.0,
                (Some(f), TextAlign::Right) => f.width() - width,
            };
            let hidden = frame.is_some_and(|f| baseline + descent > f.height() + 1e-9);
            overflow |= hidden;
            let mut carets = vec![x];
            for (_, gid, advance) in chars {
                if !hidden && let Some(glyph) = outlines.get(*gid) {
                    let mut pen = Pen { path: &mut path, origin: Point::new(x, baseline) };
                    // A glyph that fails to draw is left blank rather than failing the text.
                    let _ = glyph.draw(DrawSettings::unhinted(size, location), &mut pen);
                }
                x += advance;
                carets.push(x);
            }
            lines.push(Line { start: start + range.start, baseline, carets });
        }
        start += glyphs.len() + 1;
    }
    TextLayout { path, lines, ascent, descent, frame, overflow }
}

/// Accuracy of arc lengths along a text path, in points.
const ARC_ACCURACY: f64 = 1e-3;

/// The point `s` points along `path`, if it is that long.
pub fn point_along(path: &BezPath, s: f64) -> Option<Point> {
    use kurbo::{ParamCurve, ParamCurveArclen};
    let mut left = s;
    if left < 0.0 {
        return None;
    }
    for seg in path.segments() {
        let len = seg.arclen(ARC_ACCURACY);
        if left <= len {
            return Some(seg.eval(seg.inv_arclen(left, ARC_ACCURACY)));
        }
        left -= len;
    }
    None
}

/// Text along `guide`: each glyph's centre sits on the path at its distance along it,
/// turned to the path's direction there. Line breaks count as spaces; glyphs past the
/// end of the path are not drawn.
fn layout_on_path(text: &Text, guide: &BezPath, font: &FontRef, size: Size, ascent: f64, descent: f64) -> TextLayout {
    use kurbo::{Affine, ParamCurve, ParamCurveArclen, Shape as _};
    let location = LocationRef::default();
    let charmap = font.charmap();
    let advances = font.glyph_metrics(size, location);
    let outlines = font.outline_glyphs();
    let glyphs: Vec<(GlyphId, f64)> = text
        .content
        .chars()
        .map(|ch| {
            let gid = charmap.map(if ch == '\n' { ' ' } else { ch }).unwrap_or(GlyphId::NOTDEF);
            (gid, f64::from(advances.advance_width(gid).unwrap_or(0.0)))
        })
        .collect();
    let segments: Vec<kurbo::PathSeg> = guide.segments().collect();
    let lengths: Vec<f64> = segments.iter().map(|s| s.arclen(ARC_ACCURACY)).collect();
    let total: f64 = lengths.iter().sum();
    let width: f64 = glyphs.iter().map(|g| g.1).sum();
    let start = match text.align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (total - width) / 2.0,
        TextAlign::Right => total - width,
    };
    // Point and direction (radians) at distance `s` along the path.
    let at = |s: f64| -> Option<(Point, f64)> {
        if !(0.0..=total).contains(&s) || segments.is_empty() {
            return None;
        }
        let mut left = s;
        for (seg, len) in segments.iter().zip(&lengths) {
            if left <= *len || std::ptr::eq(seg, segments.last().expect("not empty")) {
                let t = seg.inv_arclen(left.min(*len), ARC_ACCURACY);
                let (a, b) = (seg.eval((t - 1e-4).max(0.0)), seg.eval((t + 1e-4).min(1.0)));
                return Some((seg.eval(t), (b - a).atan2()));
            }
            left -= len;
        }
        None
    };
    let mut path = BezPath::new();
    let mut carets = vec![start];
    let mut s = start;
    for (gid, advance) in glyphs {
        if let (Some((p, angle)), Some(glyph)) = (at(s + advance / 2.0), outlines.get(gid)) {
            let mut one = BezPath::new();
            let mut pen = Pen { path: &mut one, origin: Point::new(-advance / 2.0, 0.0) };
            let _ = glyph.draw(DrawSettings::unhinted(size, location), &mut pen);
            one.apply_affine(Affine::translate(p.to_vec2()) * Affine::rotate(angle));
            path.extend(one);
        }
        s += advance;
        carets.push(s);
    }
    let area = if path.is_empty() { guide.bounding_box() } else { path.bounding_box() };
    TextLayout { path, lines: vec![Line { start: 0, baseline: 0.0, carets }], ascent, descent, frame: Some(area), overflow: false }
}

/// Split one paragraph into lines no wider than `width`: break after the last space that
/// fits, or inside a word longer than a line. Spaces at a break stay on the line before.
fn wrap(chars: &[char], advance: &[f64], width: f64) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    // Where the line could break: just after the last space seen on it.
    let mut last_break = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            used += advance[i];
            last_break = Some(i + 1);
            i += 1;
            continue;
        }
        // Width of the line without its trailing spaces if this character joins it.
        let trailing: f64 = (start..i).rev().take_while(|k| chars[*k] == ' ').map(|k| advance[k]).sum();
        if i > start && used - trailing + advance[i] > width + 1e-9 {
            let end = last_break.filter(|b| *b > start).unwrap_or(i);
            lines.push(start..end);
            start = end;
            used = advance[start..i].iter().sum();
            last_break = None;
            continue;
        }
        used += advance[i];
        i += 1;
    }
    lines.push(start..chars.len());
    lines
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
    fn wrap_breaks_after_spaces_and_inside_long_words() {
        let chars: Vec<char> = "aa bb cccccc".chars().collect();
        let adv = vec![1.0; chars.len()];
        assert_eq!(wrap(&chars, &adv, 5.0), vec![0..6, 6..11, 11..12], "\"aa bb\" fits exactly; the long word is split");
        assert_eq!(wrap(&chars, &adv, 4.0), vec![0..3, 3..6, 6..10, 10..12], "breaks after spaces");
        assert_eq!(wrap(&chars, &adv, 100.0), vec![0..12]);
        assert_eq!(wrap(&[], &[], 5.0), vec![0..0]);
    }

    fn paragraph(content: &str, w: f64, h: f64) -> Text {
        Text { frame: Some(kurbo::Size::new(w, h)), ..text(content) }
    }

    #[test]
    fn paragraph_text_wraps_to_its_frame_and_starts_at_the_top() {
        let words = "one two three four five six";
        let t = paragraph(words, 80.0, 400.0);
        let l = t.layout();
        assert!(l.lines.len() > 1, "wrapped: {}", l.lines.len());
        assert_eq!(l.bounds(), Rect::new(0.0, 0.0, 80.0, 400.0));
        assert!((l.lines[0].baseline - l.ascent).abs() < 1e-9);
        let b = t.to_path().bounding_box();
        assert!(b.x0 >= -0.5 && b.x1 <= 80.5 && b.y0 >= -0.5, "inside the frame: {b:?}");
        assert!(!l.overflow);
        // Every character keeps one caret index; a soft break shows at the next line.
        let second = l.lines[1].start;
        assert_eq!(l.caret(second).0.y, l.lines[1].baseline - l.ascent);
        let mid = Point::new(l.lines[1].carets[1] + 0.1, l.lines[1].baseline - 2.0);
        assert_eq!(l.index_at(mid), second + 1);
        assert_eq!(artistic_count(&l), words.chars().count() + 1 + (l.lines.len() - 1));
    }

    /// Caret slots over all lines (each line has one more than its characters).
    fn artistic_count(l: &TextLayout) -> usize {
        l.lines.iter().map(|x| x.carets.len()).sum()
    }

    #[test]
    fn paragraph_lines_align_in_the_frame_and_overflow_is_hidden() {
        let mut t = paragraph("ab", 200.0, 400.0);
        t.align = TextAlign::Right;
        let l = t.layout();
        assert!((l.lines[0].carets.last().unwrap() - 200.0).abs() < 1e-9, "ends at the right edge");
        t.align = TextAlign::Center;
        let c = t.layout().lines[0].carets.clone();
        assert!((c[0] + c.last().unwrap() - 200.0).abs() < 1e-9, "centred in the frame");

        let short = paragraph("line one\nline two\nline three", 200.0, 30.0);
        let l = short.layout();
        assert!(l.overflow, "three lines do not fit 30 pt");
        assert_eq!(l.lines.len(), 3, "hidden lines still have carets");
        let b = short.to_path().bounding_box();
        assert!(b.y1 <= 30.5, "only the first line is drawn: {b:?}");
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> BezPath {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        p
    }

    #[test]
    fn text_on_a_path_follows_it_and_is_aligned_along_it() {
        let flat = Text { on_path: Some(line((0.0, 0.0), (500.0, 0.0))), ..text("Hello") };
        let straight = text("Hello").to_path().bounding_box();
        let on = flat.to_path().bounding_box();
        assert!((on.x0 - straight.x0).abs() < 0.01 && (on.y0 - straight.y0).abs() < 0.01, "a flat path matches plain text: {on:?} {straight:?}");

        let down = Text { on_path: Some(line((0.0, 0.0), (0.0, 500.0))), ..text("Hello") };
        let b = down.to_path().bounding_box();
        assert!(b.height() > b.width() * 2.0, "turned along a vertical path: {b:?}");
        assert_eq!(down.layout().bounds(), b, "the glyphs are the text's area");

        let centred = Text { align: TextAlign::Center, ..flat.clone() };
        let c = centred.to_path().bounding_box();
        assert!(((c.x0 + c.x1) / 2.0 - 250.0).abs() < 5.0, "centred on the path: {c:?}");

        let short = Text { on_path: Some(line((0.0, 0.0), (20.0, 0.0))), ..text("Hello") };
        assert!(short.to_path().bounding_box().x1 < 25.0, "glyphs past the end are not drawn");
        let empty = Text { on_path: Some(line((0.0, 0.0), (20.0, 0.0))), ..text("") };
        assert_eq!(empty.layout().bounds(), Rect::new(0.0, 0.0, 20.0, 0.0));
        assert_eq!(point_along(&line((0.0, 0.0), (10.0, 0.0)), 4.0), Some(Point::new(4.0, 0.0)));
        assert_eq!(point_along(&line((0.0, 0.0), (10.0, 0.0)), 11.0), None);
    }

    #[test]
    fn artistic_text_still_loads_without_a_frame() {
        let t: Text = serde_json::from_str(r#"{"content":"A","family":"x","style":"Regular","size":12.0}"#).unwrap();
        assert_eq!(t.frame, None);
        assert!(!serde_json::to_string(&t).unwrap().contains("frame"));
    }

    #[test]
    fn unknown_family_falls_back_to_the_bundled_font() {
        let mut t = text("A");
        t.family = "No Such Font 12345".into();
        assert_eq!(t.to_path(), text("A").to_path());
    }
}

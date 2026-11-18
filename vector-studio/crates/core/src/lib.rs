//! Document model for Vector Studio. No UI or GPU dependencies.

pub mod bitmap;
pub mod brush;
pub mod clones;
pub mod codes;
pub mod construct;
pub mod controls;
pub mod curves;
pub mod cutting;
pub mod dimension;
pub mod document;
pub mod fill;
pub mod find;
pub mod geometry;
pub mod history;
pub mod layers;
pub mod merge;
pub mod nodes;
pub mod ops;
pub mod page_numbers;
pub mod perspective;
pub mod powerclip;
pub mod raster;
pub mod shaping;
pub mod snap;
pub mod symbols;
pub mod symmetry;
pub mod table;
pub mod align_guides;
pub mod text;
pub mod text_tools;
pub mod units;

pub use document::{
    ArcKind, Background, Document, DocumentProperties, DocumentError, EllipseArc, LineCap, LineJoin, LineStyle, Object, ObjectId, Orientation, Outline, OverprintPart,
    PAGE_PRESETS, Page, PageId, PagePreset, Rgba, Shape, Style,
};
pub use bitmap::{Bitmap, BitmapFormat};
pub use find::TextMatch;
pub use fill::{Fill, Gradient, GradientKind, GradientStop};
pub use history::History;
pub use merge::MergeData;
pub use layers::{Layer, LayerError, LayerId};
pub use kurbo;
pub use ops::{Align, Distribute, Order};
pub use perspective::Perspective;
pub use shaping::Shaping;
pub use snap::{Guide, GuideAxis, SnapOptions, Snapper};
pub use symbols::{Symbol, SymbolId};
pub use symmetry::Symmetry;
pub use table::{Cells, Separator, Table};
pub use text::{ListStyle, Text, TextAlign, TextField};
pub use units::Unit;

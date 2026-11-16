//! Document model for Vector Studio. No UI or GPU dependencies.

pub mod bitmap;
pub mod controls;
pub mod curves;
pub mod cutting;
pub mod document;
pub mod fill;
pub mod geometry;
pub mod history;
pub mod nodes;
pub mod ops;
pub mod shaping;
pub mod snap;
pub mod text;
pub mod units;

pub use document::{
    ArcKind, Document, DocumentError, EllipseArc, LineCap, LineJoin, LineStyle, Object, ObjectId, Orientation, Outline,
    PAGE_PRESETS, Page, PageId, PagePreset, Rgba, Shape, Style,
};
pub use bitmap::{Bitmap, BitmapFormat};
pub use fill::{Fill, Gradient, GradientKind, GradientStop};
pub use history::History;
pub use kurbo;
pub use ops::{Align, Distribute, Order};
pub use shaping::Shaping;
pub use snap::{Guide, GuideAxis, SnapOptions, Snapper};
pub use text::{Text, TextAlign};
pub use units::Unit;

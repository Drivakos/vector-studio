//! Document model for Vector Studio. No UI or GPU dependencies.

pub mod controls;
pub mod document;
pub mod geometry;
pub mod history;
pub mod nodes;
pub mod ops;
pub mod shaping;
pub mod text;
pub mod units;

pub use document::{
    ArcKind, Document, DocumentError, EllipseArc, LineCap, LineJoin, LineStyle, Object, ObjectId, Orientation, Outline,
    PAGE_PRESETS, Page, PageId, PagePreset, Rgba, Shape, Style,
};
pub use history::History;
pub use kurbo;
pub use ops::{Align, Order};
pub use shaping::Shaping;
pub use text::{Text, TextAlign};
pub use units::Unit;

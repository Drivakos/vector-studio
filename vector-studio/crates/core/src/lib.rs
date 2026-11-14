//! Document model for Vector Studio. No UI or GPU dependencies.

pub mod document;
pub mod geometry;
pub mod history;
pub mod nodes;
pub mod ops;
pub mod units;

pub use document::{
    Document, DocumentError, Object, ObjectId, Orientation, Outline, PAGE_PRESETS, Page, PageId,
    PagePreset, Rgba, Shape, Style,
};
pub use history::History;
pub use kurbo;
pub use ops::{Align, Order};
pub use units::Unit;

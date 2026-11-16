//! Rendering for Vector Studio: viewport maths, ruler layout and Vello scene building.

pub mod bitmaps;
pub mod rulers;
pub mod scene;
pub mod viewport;

pub use rulers::{Tick, ruler_ticks};
pub use scene::{CanvasColors, build_page_scene};
pub use vello;
pub use viewport::Viewport;

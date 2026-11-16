//! Rendering for Vector Studio: viewport maths, ruler layout and Vello scene building.

pub mod bitmaps;
pub mod rulers;
pub mod scene;
pub mod viewport;

pub use rulers::{Tick, ruler_ticks};
pub use scene::{CanvasColors, PixelGrid, ViewMode, build_page_scene, pixel_grid, to_color};
pub use vello;
pub use viewport::Viewport;

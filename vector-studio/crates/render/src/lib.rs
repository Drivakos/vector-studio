//! Rendering for Vector Studio: viewport maths, ruler layout and Vello scene building.

pub mod bitmaps;
pub mod color;
pub mod proof;
pub mod rulers;
pub mod scene;
pub mod viewport;

pub use color::{Cmyk, GENERIC_CMYK, ProfileInfo, cmyk_profiles, profile_name, resolve_profile};
pub use proof::Proof;
pub use rulers::{Tick, ruler_ticks};
pub use scene::{CanvasColors, Look, PixelGrid, ViewMode, build_page_scene, pixel_grid, to_color};
pub use vello;
pub use viewport::Viewport;

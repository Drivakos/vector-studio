//! Free Transform tool (Pick flyout): rotate, reflect, scale or skew the selection about
//! the point where the drag starts, by how far and which way the pointer goes.

use egui_material_icons::icons::{ICON_FLIP, ICON_FORMAT_ITALIC, ICON_OPEN_IN_FULL, ICON_ROTATE_RIGHT};
use vs_core::kurbo::{Affine, Point, Vec2};

use crate::app::AppState;

/// What a Free Transform drag does, chosen on the property bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FreeMode {
    /// Turn by the angle the pointer sweeps around the anchor.
    #[default]
    Rotate,
    /// Mirror across the line from the anchor to the pointer.
    Reflect,
    /// Scale by how much nearer or farther the pointer is from the anchor.
    Scale,
    /// Slant along the axis the drag starts across.
    Skew,
}

impl FreeMode {
    pub const ALL: [FreeMode; 4] = [FreeMode::Rotate, FreeMode::Reflect, FreeMode::Scale, FreeMode::Skew];

    pub fn label(self) -> &'static str {
        match self {
            FreeMode::Rotate => "Free rotation",
            FreeMode::Reflect => "Free angle reflection",
            FreeMode::Scale => "Free scale",
            FreeMode::Skew => "Free skew",
        }
    }
}

/// x' = x + kx·y, y' = y + ky·x.
fn skew(kx: f64, ky: f64) -> Affine {
    Affine::new([1.0, ky, kx, 1.0, 0.0, 0.0])
}

/// Snap an angle to 15° steps.
fn snap(angle: f64) -> f64 {
    let step = 15f64.to_radians();
    (angle / step).round() * step
}

/// The transform a drag makes about `anchor`, the press point. Rotation turns by the
/// angle the pointer sweeps from `start` (where it was once the drag began) to `now`;
/// reflection mirrors across the line from the anchor to `now`. Scale and skew go by how
/// far the pointer has moved, against `size`, the selection's width and height: dragging
/// right or down by the width (or height) doubles it, or slants it by 45°. `constrain`
/// (Cmd) keeps rotation and reflection to 15° steps and lets scaling change width and
/// height apart.
pub fn transform(mode: FreeMode, anchor: Point, start: Point, now: Point, size: Vec2, constrain: bool) -> Affine {
    let (s, v) = (start - anchor, now - anchor);
    let (w, h) = (size.x.max(1e-9), size.y.max(1e-9));
    let about = |t: Affine| Affine::translate(anchor.to_vec2()) * t * Affine::translate(-anchor.to_vec2());
    match mode {
        _ if v.hypot() < 1e-9 => Affine::IDENTITY,
        FreeMode::Rotate if s.hypot() < 1e-9 => Affine::IDENTITY,
        FreeMode::Rotate => {
            let angle = v.atan2() - s.atan2();
            Affine::rotate_about(if constrain { snap(angle) } else { angle }, anchor)
        }
        FreeMode::Reflect => {
            let angle = if constrain { snap(v.atan2()) } else { v.atan2() };
            Affine::reflect(anchor, Vec2::from_angle(angle))
        }
        FreeMode::Scale if constrain => about(Affine::scale_non_uniform(1.0 + v.x / w, 1.0 + v.y / h)),
        FreeMode::Scale => about(Affine::scale(1.0 + (v.x + v.y) / (w + h))),
        // Mostly sideways: slant horizontally; mostly up or down: vertically.
        FreeMode::Skew if v.x.abs() >= v.y.abs() => about(skew(v.x / h, 0.0)),
        FreeMode::Skew => about(skew(0.0, v.y / w)),
    }
}

/// Property bar of the Free Transform tool: the four modes.
pub fn mode_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let segments: Vec<_> = FreeMode::ALL
        .iter()
        .map(|m| {
            let icon = match m {
                FreeMode::Rotate => ICON_ROTATE_RIGHT,
                FreeMode::Reflect => ICON_FLIP,
                FreeMode::Scale => ICON_OPEN_IN_FULL,
                FreeMode::Skew => ICON_FORMAT_ITALIC,
            };
            (icon, m.label(), *m == state.tools.free_mode)
        })
        .collect();
    if let Some(i) = crate::chrome::segmented(ui, "free_transform_mode", &segments) {
        state.tools.free_mode = FreeMode::ALL[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-9
    }

    const SIZE: Vec2 = Vec2::new(10.0, 20.0);

    #[test]
    fn rotation_turns_by_the_angle_swept_about_the_anchor() {
        let a = Point::new(10.0, 10.0);
        let t = transform(FreeMode::Rotate, a, Point::new(20.0, 10.0), Point::new(10.0, 30.0), SIZE, false);
        assert!(close(t * a, a), "the anchor stays put");
        assert!(close(t * Point::new(15.0, 10.0), Point::new(10.0, 15.0)), "a quarter turn");
        let t = transform(FreeMode::Rotate, a, Point::new(20.0, 10.0), Point::new(20.0, 11.0), SIZE, true);
        assert!(close(t * Point::new(20.0, 10.0), Point::new(20.0, 10.0)), "Cmd: under 7.5° snaps to none");
    }

    #[test]
    fn reflection_mirrors_across_the_line_to_the_pointer() {
        let a = Point::new(0.0, 0.0);
        let t = transform(FreeMode::Reflect, a, a, Point::new(0.0, 5.0), SIZE, false);
        assert!(close(t * Point::new(3.0, 1.0), Point::new(-3.0, 1.0)), "across the vertical line");
        let t = transform(FreeMode::Reflect, a, a, Point::new(5.0, 5.2), SIZE, true);
        assert!(close(t * Point::new(1.0, 0.0), Point::new(0.0, 1.0)), "Cmd: snapped to the 45° line");
    }

    #[test]
    fn scaling_is_proportional_unless_cmd_frees_it() {
        let a = Point::new(0.0, 0.0);
        let t = transform(FreeMode::Scale, a, a, Point::new(10.0, 20.0), SIZE, false);
        assert!(close(t * Point::new(3.0, 1.0), Point::new(6.0, 2.0)), "moved by the size: twice as big");
        let t = transform(FreeMode::Scale, a, a, Point::new(-5.0, -10.0), SIZE, false);
        assert!(close(t * Point::new(4.0, 4.0), Point::new(2.0, 2.0)), "up and left shrinks");
        let t = transform(FreeMode::Scale, a, a, Point::new(10.0, -10.0), SIZE, true);
        assert!(close(t * Point::new(3.0, 4.0), Point::new(6.0, 2.0)), "width and height apart");
    }

    #[test]
    fn skew_slants_along_the_way_the_pointer_moves() {
        let a = Point::new(0.0, 0.0);
        let t = transform(FreeMode::Skew, a, a, Point::new(20.0, 1.0), SIZE, false);
        assert!(close(t * Point::new(0.0, 20.0), Point::new(20.0, 20.0)), "45° horizontally");
        assert!(close(t * Point::new(4.0, 0.0), Point::new(4.0, 0.0)), "the anchor's row stays");
        let t = transform(FreeMode::Skew, a, a, Point::new(0.0, -10.0), SIZE, false);
        assert!(close(t * Point::new(10.0, 0.0), Point::new(10.0, -10.0)), "vertically");
    }

    #[test]
    fn no_distance_means_no_change() {
        let a = Point::new(1.0, 1.0);
        for m in FreeMode::ALL {
            assert_eq!(transform(m, a, Point::new(5.0, 5.0), a, SIZE, false), Affine::IDENTITY);
        }
        assert_eq!(transform(FreeMode::Rotate, a, a, Point::new(5.0, 5.0), SIZE, false), Affine::IDENTITY);
    }
}

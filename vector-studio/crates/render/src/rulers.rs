//! Ruler tick layout. Pure maths so the UI only has to draw lines and labels.

use vs_core::Unit;

/// Minimum screen distance between labelled ticks.
pub const MIN_MAJOR_SPACING: f64 = 60.0;
/// Minimum screen distance between unlabelled ticks.
pub const MIN_MINOR_SPACING: f64 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    /// Position along the ruler, in screen points from the ruler's start.
    pub pos: f64,
    pub major: bool,
    /// Present on major ticks only.
    pub label: Option<String>,
}

/// Ticks for one ruler axis.
///
/// `zoom` is screen points per document point, `origin` is the screen position
/// (along this axis, relative to the ruler start) of document coordinate 0,
/// and `length` is the ruler length in screen points.
pub fn ruler_ticks(zoom: f64, origin: f64, length: f64, unit: Unit) -> Vec<Tick> {
    if zoom <= 0.0 || length <= 0.0 {
        return Vec::new();
    }
    let px_per_unit = zoom * unit.points_per_unit();
    let major = nice_step(MIN_MAJOR_SPACING / px_per_unit);
    let minor_div = [10, 5, 2]
        .into_iter()
        .find(|d| major / *d as f64 * px_per_unit >= MIN_MINOR_SPACING)
        .unwrap_or(1);
    let minor = major / minor_div as f64;

    let start_units = -origin / px_per_unit;
    let end_units = (length - origin) / px_per_unit;
    let first = (start_units / minor).floor() as i64;
    let last = (end_units / minor).ceil() as i64;

    let mut ticks = Vec::with_capacity((last - first + 1).max(0) as usize);
    for i in first..=last {
        let value = i as f64 * minor;
        let pos = origin + value * px_per_unit;
        if !(0.0..=length).contains(&pos) {
            continue;
        }
        let is_major = i.rem_euclid(minor_div as i64) == 0;
        ticks.push(Tick {
            pos,
            major: is_major,
            label: is_major.then(|| format_value(value)),
        });
    }
    ticks
}

/// Smallest value of the form {1, 2, 5} × 10^k that is ≥ `min`.
fn nice_step(min: f64) -> f64 {
    let exp = min.log10().floor();
    let base = 10f64.powf(exp);
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * base >= min - 1e-12 {
            return m * base;
        }
    }
    10.0 * base
}

fn format_value(v: f64) -> String {
    let rounded = (v * 1000.0).round() / 1000.0;
    let s = format!("{rounded}");
    if s == "-0" { "0".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(0.7), 1.0);
        assert_eq!(nice_step(1.3), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(7.0), 10.0);
        assert_eq!(nice_step(42.0), 50.0);
        assert!((nice_step(0.013) - 0.02).abs() < 1e-12);
    }

    #[test]
    fn major_ticks_are_spaced_and_labelled() {
        let ticks = ruler_ticks(96.0 / 72.0, 30.0, 1000.0, Unit::Millimeters);
        let majors: Vec<_> = ticks.iter().filter(|t| t.major).collect();
        assert!(majors.len() >= 2);
        for pair in majors.windows(2) {
            assert!(pair[1].pos - pair[0].pos >= MIN_MAJOR_SPACING - 1e-9);
        }
        assert!(majors.iter().all(|t| t.label.is_some()));
        assert!(ticks.iter().filter(|t| !t.major).all(|t| t.label.is_none()));
    }

    #[test]
    fn zero_is_labelled_at_origin() {
        let ticks = ruler_ticks(2.0, 100.0, 500.0, Unit::Millimeters);
        let zero = ticks.iter().find(|t| t.label.as_deref() == Some("0")).unwrap();
        assert!((zero.pos - 100.0).abs() < 1e-9);
    }

    #[test]
    fn ticks_stay_inside_ruler() {
        let ticks = ruler_ticks(0.37, -900.0, 640.0, Unit::Inches);
        assert!(ticks.iter().all(|t| (0.0..=640.0).contains(&t.pos)));
    }

    #[test]
    fn minor_ticks_respect_minimum_spacing() {
        let ticks = ruler_ticks(5.0, 0.0, 800.0, Unit::Points);
        for pair in ticks.windows(2) {
            assert!(pair[1].pos - pair[0].pos >= MIN_MINOR_SPACING - 1e-9);
        }
    }

    #[test]
    fn degenerate_input_gives_no_ticks() {
        assert!(ruler_ticks(0.0, 0.0, 100.0, Unit::Millimeters).is_empty());
        assert!(ruler_ticks(1.0, 0.0, 0.0, Unit::Millimeters).is_empty());
    }
}

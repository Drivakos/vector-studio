//! Measurement units. Document geometry is always stored in points (1/72 inch).

use serde::{Deserialize, Serialize};

pub const POINTS_PER_INCH: f64 = 72.0;
pub const MM_PER_INCH: f64 = 25.4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[default]
    Millimeters,
    Centimeters,
    Inches,
    Points,
    Pixels,
}

impl Unit {
    pub const ALL: [Unit; 5] = [
        Unit::Millimeters,
        Unit::Centimeters,
        Unit::Inches,
        Unit::Points,
        Unit::Pixels,
    ];

    /// How many points one of this unit is. Pixels assume 96 DPI.
    pub fn points_per_unit(self) -> f64 {
        match self {
            Unit::Millimeters => POINTS_PER_INCH / MM_PER_INCH,
            Unit::Centimeters => POINTS_PER_INCH / MM_PER_INCH * 10.0,
            Unit::Inches => POINTS_PER_INCH,
            Unit::Points => 1.0,
            Unit::Pixels => POINTS_PER_INCH / 96.0,
        }
    }

    pub fn from_points(self, pt: f64) -> f64 {
        pt / self.points_per_unit()
    }

    pub fn to_points(self, value: f64) -> f64 {
        value * self.points_per_unit()
    }

    pub fn label(self) -> &'static str {
        match self {
            Unit::Millimeters => "millimeters",
            Unit::Centimeters => "centimeters",
            Unit::Inches => "inches",
            Unit::Points => "points",
            Unit::Pixels => "pixels",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm",
            Unit::Centimeters => "cm",
            Unit::Inches => "in",
            Unit::Points => "pt",
            Unit::Pixels => "px",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_width_round_trips_through_points() {
        let pt = Unit::Millimeters.to_points(210.0);
        assert!((pt - 595.2756).abs() < 1e-3);
        assert!((Unit::Millimeters.from_points(pt) - 210.0).abs() < 1e-9);
    }

    #[test]
    fn every_unit_round_trips() {
        for unit in Unit::ALL {
            let v = 12.345;
            assert!((unit.from_points(unit.to_points(v)) - v).abs() < 1e-9, "{unit:?}");
        }
    }

    #[test]
    fn inch_is_72_points_and_96_pixels() {
        assert_eq!(Unit::Inches.to_points(1.0), 72.0);
        assert!((Unit::Pixels.from_points(72.0) - 96.0).abs() < 1e-9);
    }
}

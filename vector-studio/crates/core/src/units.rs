//! Measurement units. Document geometry is always stored in points (1/72 inch).

use serde::{Deserialize, Serialize};

pub const POINTS_PER_INCH: f64 = 72.0;
pub const MM_PER_INCH: f64 = 25.4;

/// The 13 units offered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Inches,
    #[default]
    Millimeters,
    Picas,
    Points,
    Pixels,
    Ciceros,
    Didots,
    Feet,
    Yards,
    Miles,
    Centimeters,
    Meters,
    Kilometers,
}

/// A Didot point: 1/72 of the French royal inch, 0.376065 mm.
const DIDOT_MM: f64 = 0.376_065;

impl Unit {
    pub const ALL: [Unit; 13] = [
        Unit::Inches,
        Unit::Millimeters,
        Unit::Picas,
        Unit::Points,
        Unit::Pixels,
        Unit::Ciceros,
        Unit::Didots,
        Unit::Feet,
        Unit::Yards,
        Unit::Miles,
        Unit::Centimeters,
        Unit::Meters,
        Unit::Kilometers,
    ];

    /// How many points one of this unit is. Pixels assume 96 DPI.
    pub fn points_per_unit(self) -> f64 {
        let mm = POINTS_PER_INCH / MM_PER_INCH;
        match self {
            Unit::Inches => POINTS_PER_INCH,
            Unit::Millimeters => mm,
            Unit::Picas => 12.0,
            Unit::Points => 1.0,
            Unit::Pixels => POINTS_PER_INCH / 96.0,
            Unit::Ciceros => 12.0 * DIDOT_MM * mm,
            Unit::Didots => DIDOT_MM * mm,
            Unit::Feet => 12.0 * POINTS_PER_INCH,
            Unit::Yards => 36.0 * POINTS_PER_INCH,
            Unit::Miles => 63_360.0 * POINTS_PER_INCH,
            Unit::Centimeters => 10.0 * mm,
            Unit::Meters => 1_000.0 * mm,
            Unit::Kilometers => 1_000_000.0 * mm,
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
            Unit::Inches => "inches",
            Unit::Millimeters => "millimeters",
            Unit::Picas => "picas",
            Unit::Points => "points",
            Unit::Pixels => "pixels",
            Unit::Ciceros => "ciceros",
            Unit::Didots => "didots",
            Unit::Feet => "feet",
            Unit::Yards => "yards",
            Unit::Miles => "miles",
            Unit::Centimeters => "centimeters",
            Unit::Meters => "meters",
            Unit::Kilometers => "kilometers",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Inches => "in",
            Unit::Millimeters => "mm",
            Unit::Picas => "pc",
            Unit::Points => "pt",
            Unit::Pixels => "px",
            Unit::Ciceros => "cc",
            Unit::Didots => "dd",
            Unit::Feet => "ft",
            Unit::Yards => "yd",
            Unit::Miles => "mi",
            Unit::Centimeters => "cm",
            Unit::Meters => "m",
            Unit::Kilometers => "km",
        }
    }

    /// Stepper increment: the largest power of ten not above 1 mm, capped at 1.
    /// Gives 1 mm, 0.1 cm, 0.001 m, 0.01 in, 1 pt.
    pub fn step(self) -> f64 {
        let one_mm = self.from_points(Unit::Millimeters.points_per_unit());
        10f64.powf(one_mm.log10().floor()).min(1.0)
    }

    /// Decimal places shown in number fields: two more than the step needs, at least 3.
    pub fn decimals(self) -> usize {
        let step_digits = (-self.step().log10()).round().max(0.0) as usize;
        (step_digits + 2).max(3)
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
    fn metric_and_imperial_lengths_agree() {
        assert!((Unit::Meters.to_points(1.0) - Unit::Millimeters.to_points(1000.0)).abs() < 1e-9);
        assert!((Unit::Kilometers.to_points(1.0) - Unit::Meters.to_points(1000.0)).abs() < 1e-6);
        assert!((Unit::Feet.to_points(3.0) - Unit::Yards.to_points(1.0)).abs() < 1e-9);
        assert!((Unit::Miles.to_points(1.0) - Unit::Feet.to_points(5280.0)).abs() < 1e-6);
        assert_eq!(Unit::Picas.to_points(1.0), 12.0);
        assert!((Unit::Ciceros.to_points(1.0) - Unit::Didots.to_points(12.0)).abs() < 1e-9);
    }

    #[test]
    fn steps_are_about_a_millimetre() {
        assert_eq!(Unit::Millimeters.step(), 1.0);
        assert!((Unit::Centimeters.step() - 0.1).abs() < 1e-12);
        assert!((Unit::Meters.step() - 0.001).abs() < 1e-12);
        assert!((Unit::Kilometers.step() - 1e-6).abs() < 1e-18);
        assert!((Unit::Inches.step() - 0.01).abs() < 1e-12);
        assert_eq!(Unit::Points.step(), 1.0);
        assert_eq!(Unit::Kilometers.decimals(), 8);
        assert_eq!(Unit::Millimeters.decimals(), 3);
    }

    #[test]
    fn inch_is_72_points_and_96_pixels() {
        assert_eq!(Unit::Inches.to_points(1.0), 72.0);
        assert!((Unit::Pixels.from_points(72.0) - 96.0).abs() < 1e-9);
    }
}

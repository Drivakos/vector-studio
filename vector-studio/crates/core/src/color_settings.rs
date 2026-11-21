//! Document color settings: the CMYK printing profile and rendering intent that CMYK
//! values, CMYK PDF export and printing use. Colours themselves are kept as sRGB.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Rgba;

/// How colours a profile can't reach are brought into its gamut (ICC rendering intents).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RenderingIntent {
    /// The whole gamut squeezed in, keeping colours' relations.
    Perceptual,
    /// Colours inside kept exactly, relative to paper white; the others clipped.
    #[default]
    RelativeColorimetric,
    /// Vivid colours kept vivid.
    Saturation,
    /// Like Relative Colorimetric, without adapting to the paper's white.
    AbsoluteColorimetric,
}

impl RenderingIntent {
    pub const ALL: [RenderingIntent; 4] = [
        RenderingIntent::Perceptual,
        RenderingIntent::RelativeColorimetric,
        RenderingIntent::Saturation,
        RenderingIntent::AbsoluteColorimetric,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RenderingIntent::Perceptual => "Perceptual",
            RenderingIntent::RelativeColorimetric => "Relative colorimetric",
            RenderingIntent::Saturation => "Saturation",
            RenderingIntent::AbsoluteColorimetric => "Absolute colorimetric",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorSettings {
    /// The CMYK ICC profile file; `None` is the system's generic CMYK profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmyk_profile: Option<PathBuf>,
    #[serde(default)]
    pub intent: RenderingIntent,
}

impl ColorSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// View > Proof Colors: which printing to simulate on the canvas, and whether to mark
/// colours it can't print.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ProofSettings {
    /// The CMYK profile to simulate; `None` is the document's own.
    pub profile: Option<PathBuf>,
    pub intent: RenderingIntent,
    /// Out-of-gamut colors: show colours the printing can't reach in `warning_color`.
    pub gamut_warning: bool,
    pub warning_color: Rgba,
    /// How much of the proofed colour shows through the warning colour, 0–100 %.
    pub warning_transparency: u8,
}

impl Default for ProofSettings {
    fn default() -> Self {
        // The warning colour and transparency are this app's choice.
        Self { profile: None, intent: RenderingIntent::default(), gamut_warning: false, warning_color: Rgba::rgb(0, 255, 0), warning_transparency: 0 }
    }
}

/// CMYK ink percentages (0–100) as 8-bit ink amounts (0–255), and back.
pub fn ink_to_byte(percent: f64) -> u8 {
    (percent.clamp(0.0, 100.0) * 2.55).round() as u8
}

pub fn byte_to_ink(byte: u8) -> f64 {
    (f64::from(byte) / 2.55).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_percent_round_trips_through_bytes() {
        for p in 0..=100 {
            assert_eq!(byte_to_ink(ink_to_byte(f64::from(p))), f64::from(p));
        }
        assert_eq!(ink_to_byte(150.0), 255);
        assert_eq!(ink_to_byte(-3.0), 0);
    }

    #[test]
    fn colours_given_as_inks_keep_them_and_mix_them() {
        let k = Rgba::rgb(35, 31, 32).with_cmyk([0, 0, 0, 255]);
        let c = Rgba::rgb(0, 160, 227).with_cmyk([255, 0, 0, 0]);
        assert_eq!(k.mix(c, 0.5).cmyk, Some([128, 0, 0, 128]), "inks mix between two ink colours");
        assert_eq!(k.mix(Rgba::WHITE, 0.5).cmyk, None, "not with a screen colour");
        assert_eq!(k.faded(0.5).cmyk, k.cmyk, "transparency keeps the inks");

        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(serde_json::from_str::<Rgba>(&json).unwrap(), k);
        let plain = serde_json::to_string(&Rgba::BLACK).unwrap();
        assert!(!plain.contains("cmyk"), "screen colours are saved as before: {plain}");
        assert_eq!(serde_json::from_str::<Rgba>(r#"{"r":1,"g":2,"b":3,"a":4}"#).unwrap(), Rgba::new(1, 2, 3, 4), "older files open");
    }

    #[test]
    fn default_settings_are_left_out_of_saved_files() {
        assert!(ColorSettings::default().is_default());
        let s = ColorSettings { cmyk_profile: None, intent: RenderingIntent::Perceptual };
        assert!(!s.is_default());
    }
}

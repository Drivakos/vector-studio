//! CMYK colour through ICC profiles: the installed CMYK printing profiles, and sRGB ⇄ CMYK
//! conversion through one of them.

use std::path::{Path, PathBuf};

use lcms2::{ColorSpaceSignature, Flags, InfoType, Intent, Locale, PixelFormat, Profile, Transform};
use vs_core::{ColorSettings, RenderingIntent, Rgba};

/// macOS's Generic CMYK profile: the one used when no other is chosen, or the chosen one
/// can't be read.
pub const GENERIC_CMYK: &str = "/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc";

/// Folders macOS keeps ICC profiles in, the system's first.
fn profile_folders() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/System/Library/ColorSync/Profiles"), PathBuf::from("/Library/ColorSync/Profiles")];
    if let Some(home) = std::env::var_os("HOME") {
        out.push(Path::new(&home).join("Library/ColorSync/Profiles"));
    }
    out
}

/// A CMYK profile on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileInfo {
    pub path: PathBuf,
    /// The profile's own description, or its file name.
    pub name: String,
}

/// `path`'s profile, if it is a readable CMYK one.
fn read_cmyk(path: &Path) -> Option<Profile> {
    let profile = Profile::new_file(path).ok()?;
    (profile.color_space() == ColorSpaceSignature::CmykData).then_some(profile)
}

/// The name to show for the CMYK profile at `path`, if it is one.
pub fn profile_name(path: &Path) -> Option<String> {
    let profile = read_cmyk(path)?;
    let name = profile.info(InfoType::Description, Locale::new("en_US")).filter(|n| !n.trim().is_empty());
    Some(name.unwrap_or_else(|| path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned())))
}

/// The CMYK profiles installed on this computer, by name.
pub fn cmyk_profiles() -> Vec<ProfileInfo> {
    let mut out: Vec<ProfileInfo> = profile_folders()
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("icc") || e.eq_ignore_ascii_case("icm")))
        .filter_map(|path| Some(ProfileInfo { name: profile_name(&path)?, path }))
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// The profile file to use for `chosen`: it if it is a readable CMYK profile, else
/// [`GENERIC_CMYK`].
pub fn resolve_profile(chosen: Option<&Path>) -> PathBuf {
    chosen.filter(|p| read_cmyk(p).is_some()).map_or_else(|| PathBuf::from(GENERIC_CMYK), Path::to_path_buf)
}

pub(crate) fn lcms_intent(intent: RenderingIntent) -> Intent {
    match intent {
        RenderingIntent::Perceptual => Intent::Perceptual,
        RenderingIntent::RelativeColorimetric => Intent::RelativeColorimetric,
        RenderingIntent::Saturation => Intent::Saturation,
        RenderingIntent::AbsoluteColorimetric => Intent::AbsoluteColorimetric,
    }
}

/// sRGB ⇄ CMYK through one printing profile. CMYK values are ink amounts, 0 (none) to
/// 255 (full).
pub struct Cmyk {
    to_cmyk: Transform<[u8; 3], [u8; 4]>,
    to_rgb: Transform<[u8; 4], [u8; 3]>,
    icc: Vec<u8>,
    pub path: PathBuf,
}

impl std::fmt::Debug for Cmyk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cmyk").field("path", &self.path).finish()
    }
}

impl Cmyk {
    /// Conversion through the CMYK profile at `path` with `intent`, or `None` if it can't
    /// be read.
    pub fn new(path: &Path, intent: RenderingIntent) -> Option<Self> {
        let printer = read_cmyk(path)?;
        let srgb = Profile::new_srgb();
        let intent = lcms_intent(intent);
        let to_cmyk = Transform::new(&srgb, PixelFormat::RGB_8, &printer, PixelFormat::CMYK_8, intent).ok()?;
        // Back to the screen, colorimetrically, so a CMYK colour shows as it prints.
        let to_rgb = Transform::new_flags(&printer, PixelFormat::CMYK_8, &srgb, PixelFormat::RGB_8, Intent::RelativeColorimetric, Flags::default()).ok()?;
        let icc = printer.icc().ok()?;
        Some(Self { to_cmyk, to_rgb, icc, path: path.to_path_buf() })
    }

    /// The document's conversion: its profile (or the generic one if that can't be read)
    /// and intent.
    pub fn for_document(settings: &ColorSettings) -> Option<Self> {
        Self::new(&resolve_profile(settings.cmyk_profile.as_deref()), settings.intent)
    }

    /// The inks that print `c` (alpha is ignored).
    pub fn cmyk(&self, c: Rgba) -> [u8; 4] {
        let mut out = [[0u8; 4]];
        self.to_cmyk.transform_pixels(&[[c.r, c.g, c.b]], &mut out);
        out[0]
    }

    /// How the inks `k` look on screen, opaque.
    pub fn rgb(&self, k: [u8; 4]) -> Rgba {
        let mut out = [[0u8; 3]];
        self.to_rgb.transform_pixels(&[k], &mut out);
        Rgba::rgb(out[0][0], out[0][1], out[0][2])
    }

    /// The profile's bytes, to embed in a file.
    pub fn icc(&self) -> &[u8] {
        &self.icc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generic_profile_is_listed_and_named() {
        let profiles = cmyk_profiles();
        let generic = profiles.iter().find(|p| p.path == Path::new(GENERIC_CMYK)).expect("macOS has a Generic CMYK profile");
        assert_eq!(generic.name, "Generic CMYK Profile");
        assert!(profiles.iter().all(|p| read_cmyk(&p.path).is_some()), "only CMYK profiles: {profiles:?}");
        assert!(profile_name(Path::new("/System/Library/ColorSync/Profiles/sRGB Profile.icc")).is_none(), "an RGB profile is not offered");
    }

    #[test]
    fn a_missing_profile_falls_back_to_the_generic_one() {
        assert_eq!(resolve_profile(Some(Path::new("/nowhere/print.icc"))), Path::new(GENERIC_CMYK));
        assert_eq!(resolve_profile(None), Path::new(GENERIC_CMYK));
    }

    #[test]
    fn cmyk_conversions_follow_the_inks() {
        let cmyk = Cmyk::for_document(&ColorSettings::default()).expect("generic profile");
        assert_eq!(cmyk.cmyk(Rgba::WHITE), [0, 0, 0, 0], "white is no ink");
        let black = cmyk.cmyk(Rgba::BLACK);
        assert!(black[3] > 200, "black uses black ink: {black:?}");
        let cyan = cmyk.rgb([255, 0, 0, 0]);
        assert!(cyan.r < 100 && cyan.g > 120 && cyan.b > 180, "full cyan looks cyan: {cyan:?}");
        let paper = cmyk.rgb([0, 0, 0, 0]);
        assert_eq!(paper, Rgba::WHITE, "no ink is paper white");
        let red = Rgba::rgb(200, 30, 40);
        let back = cmyk.rgb(cmyk.cmyk(red));
        assert!(back.r.abs_diff(red.r) < 25 && back.g.abs_diff(red.g) < 25 && back.b.abs_diff(red.b) < 25, "a printable red comes back close: {back:?}");
        assert!(!cmyk.icc().is_empty());
    }
}

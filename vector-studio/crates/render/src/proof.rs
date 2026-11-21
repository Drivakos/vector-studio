//! View > Proof Colors: colours shown as they would print, by a soft proof through a
//! CMYK printing profile.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};

use lcms2::{Flags, Intent, PixelFormat, Profile, ThreadContext, Transform};
use vello::peniko::{Blob, ImageData};
use vs_core::{Bitmap, RenderingIntent, Rgba};

use crate::color::{GENERIC_CMYK, lcms_intent};

/// The printing profile proofed against when none is chosen.
pub const CMYK_PROFILE: &str = GENERIC_CMYK;

/// sRGB → printing CMYK → sRGB, so colours the inks can't print show as the nearest ones
/// they can; optionally with those colours marked in a warning colour.
pub struct Proof {
    transform: ProofTransform,
    /// The same proof with lcms's gamut check: out-of-gamut colours come out as the
    /// warning colour.
    gamut: Option<(ProofTransform, Rgba, f64)>,
    colors: RefCell<HashMap<[u8; 3], [u8; 3]>>,
    /// Proofed bitmaps, by the bytes they came from (as in [`crate::bitmaps::decoded`]).
    bitmaps: RefCell<BitmapCache>,
    id: u64,
    // Dropped after the transforms made in it.
    _context: ThreadContext,
}

type ProofTransform = Transform<[u8; 4], [u8; 4], ThreadContext>;

type BitmapCache = HashMap<usize, (Weak<Vec<u8>>, Option<ImageData>)>;

impl std::fmt::Debug for Proof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Proof")
    }
}

impl Proof {
    /// A proof against the CMYK profile at `path` with relative colorimetric intent, or
    /// `None` if it can't be read.
    pub fn from_profile(path: &Path) -> Option<Self> {
        Self::new(path, RenderingIntent::RelativeColorimetric, None)
    }

    /// A proof against the CMYK profile at `path` with `intent`. With `warning` (a colour
    /// and how much of the proofed colour shows through it, 0 to 1), colours the profile
    /// can't print are shown in that colour.
    pub fn new(path: &Path, intent: RenderingIntent, warning: Option<(Rgba, f64)>) -> Option<Self> {
        let mut context = ThreadContext::new();
        if let Some((c, _)) = warning {
            let mut codes = [0u16; 16];
            for (code, v) in codes.iter_mut().zip([c.r, c.g, c.b]) {
                *code = u16::from(v) * 257;
            }
            context.set_alarm_codes(codes);
        }
        let printer = Profile::new_file_context(&context, path).ok()?;
        if printer.color_space() != lcms2::ColorSpaceSignature::CmykData {
            return None;
        }
        let srgb = Profile::new_srgb_context(&context);
        // Absolute colorimetric also simulates the paper's white.
        let proofing = if intent == RenderingIntent::AbsoluteColorimetric { Intent::AbsoluteColorimetric } else { Intent::RelativeColorimetric };
        let make = |flags: Flags| {
            Transform::new_proofing_context(&context, &srgb, PixelFormat::RGBA_8, &srgb, PixelFormat::RGBA_8, &printer, lcms_intent(intent), proofing, flags).ok()
        };
        let transform = make(Flags::SOFT_PROOFING)?;
        let gamut = match warning {
            Some((c, show)) => Some((make(Flags::SOFT_PROOFING | Flags::GAMUT_CHECK)?, c, show.clamp(0.0, 1.0))),
            None => None,
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(Self { transform, gamut, colors: RefCell::default(), bitmaps: RefCell::default(), id, _context: context })
    }

    /// Different for every proof made, so pictures proofed with one can be told apart.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// A proof against [`CMYK_PROFILE`].
    pub fn system() -> Option<Self> {
        Self::from_profile(Path::new(CMYK_PROFILE))
    }

    /// `pixels` proofed in place, out-of-gamut ones marked if asked for.
    fn proof(&self, pixels: &mut [[u8; 4]]) {
        let Some((gamut, warning, show)) = &self.gamut else {
            self.transform.transform_in_place(pixels);
            return;
        };
        let mut marked = pixels.to_vec();
        gamut.transform_in_place(&mut marked);
        self.transform.transform_in_place(pixels);
        for (px, m) in pixels.iter_mut().zip(&marked) {
            if px[..3] != m[..3] {
                let c = warning.mix(Rgba::rgb(px[0], px[1], px[2]), *show);
                px[..3].copy_from_slice(&[c.r, c.g, c.b]);
            }
        }
    }

    /// `c` as it would print; alpha is kept.
    pub fn color(&self, c: Rgba) -> Rgba {
        let rgb = [c.r, c.g, c.b];
        let out = *self.colors.borrow_mut().entry(rgb).or_insert_with(|| {
            let mut px = [[c.r, c.g, c.b, 255]];
            self.proof(&mut px);
            [px[0][0], px[0][1], px[0][2]]
        });
        Rgba::new(out[0], out[1], out[2], c.a)
    }

    /// `bitmap` decoded and proofed, once while any object holds its bytes.
    pub fn bitmap(&self, bitmap: &Bitmap) -> Option<ImageData> {
        let key = Arc::as_ptr(&bitmap.data) as usize;
        let mut cache = self.bitmaps.borrow_mut();
        if let Some((weak, image)) = cache.get(&key)
            && weak.upgrade().is_some_and(|data| Arc::ptr_eq(&data, &bitmap.data))
        {
            return image.clone();
        }
        cache.retain(|_, (weak, _)| weak.strong_count() > 0);
        let image = crate::bitmaps::decoded(bitmap).map(|i| self.image(&i));
        cache.insert(key, (Arc::downgrade(&bitmap.data), image.clone()));
        image
    }

    /// `image` (straight-alpha RGBA) as it would print.
    pub fn image(&self, image: &ImageData) -> ImageData {
        let mut pixels: Vec<[u8; 4]> = image.data.data().as_chunks::<4>().0.to_vec();
        self.proof(&mut pixels);
        ImageData { data: Blob::new(Arc::new(pixels.concat())), ..image.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_the_inks_cannot_print_change_and_others_barely_do() {
        let Some(proof) = Proof::system() else {
            panic!("{CMYK_PROFILE} is part of macOS");
        };
        let blue = proof.color(Rgba::rgb(0, 0, 255));
        assert!(blue.r > 20 && blue.g > 20 && blue.b < 235, "pure screen blue prints duller: {blue:?}");
        let grey = proof.color(Rgba::rgb(128, 128, 128));
        assert!([grey.r, grey.g, grey.b].iter().all(|v| v.abs_diff(128) < 20), "a grey stays grey: {grey:?}");
        let white = proof.color(Rgba { a: 100, ..Rgba::WHITE });
        assert_eq!((white.r, white.g, white.b, white.a), (255, 255, 255, 100), "paper white, alpha kept");
    }

    #[test]
    fn out_of_gamut_colours_show_in_the_warning_colour() {
        let magenta = Rgba::rgb(255, 0, 255);
        let path = Path::new(CMYK_PROFILE);
        let warn = Proof::new(path, RenderingIntent::RelativeColorimetric, Some((magenta, 0.0))).unwrap();
        assert_eq!(warn.color(Rgba::rgb(0, 0, 255)), magenta, "screen blue can't be printed");
        assert_eq!(warn.color(Rgba::rgb(0, 255, 0)), magenta, "nor screen green");
        let grey = warn.color(Rgba::rgb(128, 128, 128));
        assert_ne!(grey, magenta, "a grey can");
        let plain = Proof::system().unwrap();
        assert_eq!(grey, plain.color(Rgba::rgb(128, 128, 128)), "and looks as without the warning");

        let half = Proof::new(path, RenderingIntent::RelativeColorimetric, Some((magenta, 0.5))).unwrap();
        let shown = plain.color(Rgba::rgb(0, 0, 255));
        assert_eq!(half.color(Rgba::rgb(0, 0, 255)), magenta.mix(shown, 0.5), "half the proofed colour shows through");
    }

    #[test]
    fn intents_change_the_proof() {
        let path = Path::new(CMYK_PROFILE);
        let blue = Rgba::rgb(0, 0, 255);
        let shown: Vec<Rgba> = RenderingIntent::ALL.iter().map(|&i| Proof::new(path, i, None).unwrap().color(blue)).collect();
        // The generic profile maps perceptual and saturation as relative colorimetric.
        assert_ne!(shown[3], shown[1], "absolute colorimetric simulates the paper: {shown:?}");
        assert!(Proof::new(Path::new("/System/Library/ColorSync/Profiles/sRGB Profile.icc"), RenderingIntent::Perceptual, None).is_none(), "only CMYK profiles proof");
    }
}

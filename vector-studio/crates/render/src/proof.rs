//! View > Proof Colors: colours shown as they would print, by a soft proof through a
//! CMYK printing profile.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Weak};

use lcms2::{Flags, Intent, PixelFormat, Profile, Transform};
use vello::peniko::{Blob, ImageData};
use vs_core::{Bitmap, Rgba};

/// The printing profile proofed against: macOS's Generic CMYK profile.
pub const CMYK_PROFILE: &str = "/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc";

/// sRGB → printing CMYK → sRGB, so colours the inks can't print show as the nearest ones
/// they can.
pub struct Proof {
    transform: Transform<[u8; 4], [u8; 4]>,
    colors: RefCell<HashMap<[u8; 3], [u8; 3]>>,
    /// Proofed bitmaps, by the bytes they came from (as in [`crate::bitmaps::decoded`]).
    bitmaps: RefCell<BitmapCache>,
}

type BitmapCache = HashMap<usize, (Weak<Vec<u8>>, Option<ImageData>)>;

impl std::fmt::Debug for Proof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Proof")
    }
}

impl Proof {
    /// A proof against the CMYK profile at `path`, or `None` if it can't be read.
    pub fn from_profile(path: &Path) -> Option<Self> {
        let printer = Profile::new_file(path).ok()?;
        let srgb = Profile::new_srgb();
        let transform = Transform::new_proofing(
            &srgb,
            PixelFormat::RGBA_8,
            &srgb,
            PixelFormat::RGBA_8,
            &printer,
            Intent::RelativeColorimetric,
            Intent::RelativeColorimetric,
            Flags::SOFT_PROOFING,
        )
        .ok()?;
        Some(Self { transform, colors: RefCell::default(), bitmaps: RefCell::default() })
    }

    /// A proof against [`CMYK_PROFILE`].
    pub fn system() -> Option<Self> {
        Self::from_profile(Path::new(CMYK_PROFILE))
    }

    /// `c` as it would print; alpha is kept.
    pub fn color(&self, c: Rgba) -> Rgba {
        let rgb = [c.r, c.g, c.b];
        let out = *self.colors.borrow_mut().entry(rgb).or_insert_with(|| {
            let mut px = [[c.r, c.g, c.b, 255]];
            self.transform.transform_in_place(&mut px);
            [px[0][0], px[0][1], px[0][2]]
        });
        Rgba { r: out[0], g: out[1], b: out[2], a: c.a }
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
        self.transform.transform_in_place(&mut pixels);
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
}

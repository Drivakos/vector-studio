//! Decoded bitmaps for the canvas, cached by the bytes they came from.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vs_core::{Bitmap, BitmapFormat};

type Cache = HashMap<usize, (Weak<Vec<u8>>, Option<ImageData>)>;

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// `bitmap` as RGBA pixels, or `None` if its bytes don't decode. Each file is decoded once
/// while any object still holds it.
pub fn decoded(bitmap: &Bitmap) -> Option<ImageData> {
    let key = Arc::as_ptr(&bitmap.data) as usize;
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some((weak, image)) = cache.get(&key)
        && weak.upgrade().is_some_and(|data| Arc::ptr_eq(&data, &bitmap.data))
    {
        return image.clone();
    }
    cache.retain(|_, (weak, _)| weak.strong_count() > 0);
    let image = decode(bitmap);
    cache.insert(key, (Arc::downgrade(&bitmap.data), image.clone()));
    image
}

fn decode(bitmap: &Bitmap) -> Option<ImageData> {
    let format = match bitmap.format {
        BitmapFormat::Png => image::ImageFormat::Png,
        BitmapFormat::Jpeg => image::ImageFormat::Jpeg,
    };
    let rgba = image::load_from_memory_with_format(&bitmap.data, format).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(ImageData {
        data: Blob::new(Arc::new(rgba.into_raw())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    })
}

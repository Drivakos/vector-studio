//! Decoded bitmaps for the canvas, cached by the bytes they came from, and the images of
//! two-color pattern tiles.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vs_core::{Bitmap, BitmapFormat, Rgba};

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

/// Pixels per cell of a two-color pattern tile: enough for the cells' edges to stay sharp
/// when the tile is magnified a little.
const CELL: u32 = 16;

type TileKey = ([u8; 8], Rgba, Rgba);

static TILES: Mutex<Option<HashMap<TileKey, ImageData>>> = Mutex::new(None);

/// The image of one two-color pattern tile (8 × 8 cells), made once per pattern and
/// colours so the renderer can keep it.
pub fn two_color(cells: [u8; 8], fore: Rgba, back: Rgba) -> ImageData {
    let mut guard = TILES.lock().unwrap_or_else(|e| e.into_inner());
    let tiles = guard.get_or_insert_with(HashMap::new);
    if let Some(image) = tiles.get(&(cells, fore, back)) {
        return image.clone();
    }
    if tiles.len() > 256 {
        tiles.clear();
    }
    let side = 8 * CELL;
    let mut data = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        let row = cells[(y / CELL) as usize];
        for x in 0..side {
            let c = if row & (0x80 >> (x / CELL)) != 0 { fore } else { back };
            data.extend_from_slice(&[c.r, c.g, c.b, c.a]);
        }
    }
    let image = ImageData { data: Blob::new(Arc::new(data)), format: ImageFormat::Rgba8, alpha_type: ImageAlphaType::Alpha, width: side, height: side };
    tiles.insert((cells, fore, back), image.clone());
    image
}

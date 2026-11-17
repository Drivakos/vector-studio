//! QR codes and Code 128 barcodes as vector paths, for Object > Insert QR Code and
//! Insert Barcode.

use kurbo::{BezPath, Rect, Shape as _};
use linesweeper::BinaryOp;

use crate::shaping::boolean;

/// The modules of the QR code for `text` (medium error correction): its width in modules
/// and, row by row, whether each is dark. `None` when the text is too long for any QR code.
pub fn qr_modules(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).ok()?;
    let dark = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
    Some((code.width(), dark))
}

/// Rectangles `module` wide per dark run, merged into one region with no seams.
fn merge(rects: impl IntoIterator<Item = Rect>) -> BezPath {
    let mut runs = BezPath::new();
    for r in rects {
        runs.extend(r.path_elements(0.0));
    }
    // The runs touch but never overlap, so their union is the area they cover.
    boolean(&runs, &BezPath::new(), BinaryOp::Union).filter(|p| !p.is_empty()).unwrap_or(runs)
}

/// The QR code for `text` as one filled path, `module` points a module, its top-left
/// corner at the origin. The quiet zone around it is left to the page.
pub fn qr_path(text: &str, module: f64) -> Option<BezPath> {
    let (width, dark) = qr_modules(text)?;
    let mut rects = Vec::new();
    for (y, row) in dark.chunks(width).enumerate() {
        let mut x = 0;
        while x < width {
            if !row[x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && row[x] {
                x += 1;
            }
            rects.push(Rect::new(start as f64 * module, y as f64 * module, x as f64 * module, (y + 1) as f64 * module));
        }
    }
    Some(merge(rects))
}

/// Bar and space widths in modules for each Code 128 value, starting with a bar.
const CODE128: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
    "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
    "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
    "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
    "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
    "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
    "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
    "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
    "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
    "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];

const START_B: usize = 104;
const STOP: usize = 106;

/// The Code 128 values for `text` in code set B: start, one per character, check, stop.
/// `None` when the text is empty or has a character set B lacks (outside ASCII 32–126).
pub fn code128_values(text: &str) -> Option<Vec<usize>> {
    if text.is_empty() {
        return None;
    }
    let mut values = vec![START_B];
    for c in text.chars() {
        let c = u32::from(c);
        (32..=126).contains(&c).then_some(())?;
        values.push(c as usize - 32);
    }
    let check = values.iter().enumerate().map(|(i, v)| i.max(1) * v).sum::<usize>() % 103;
    values.push(check);
    values.push(STOP);
    Some(values)
}

/// Bar and space widths in modules for `text`, starting with a bar.
pub fn code128_widths(text: &str) -> Option<Vec<u32>> {
    let values = code128_values(text)?;
    Some(values.iter().flat_map(|v| CODE128[*v].chars().map(|c| c.to_digit(10).unwrap_or(1))).collect())
}

/// The Code 128 barcode for `text` as one filled path of bars, `module` points wide a
/// module and `height` points tall, its top-left corner at the origin.
pub fn barcode_path(text: &str, module: f64, height: f64) -> Option<BezPath> {
    let widths = code128_widths(text)?;
    let mut x = 0.0;
    let mut rects = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let w = f64::from(*w) * module;
        if i % 2 == 0 {
            rects.push(Rect::new(x, 0.0, x + w, height));
        }
        x += w;
    }
    Some(merge(rects))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code128_table_is_well_formed() {
        for (v, pattern) in CODE128.iter().enumerate() {
            let w: Vec<u32> = pattern.chars().map(|c| c.to_digit(10).unwrap()).collect();
            let total: u32 = w.iter().sum();
            assert_eq!(total, if v == STOP { 13 } else { 11 }, "value {v}");
            // Every symbol's bars add up to an even number of modules.
            let bars: u32 = w.iter().step_by(2).sum();
            assert_eq!(bars % 2, 0, "value {v}");
        }
        let mut unique = CODE128.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), CODE128.len());
    }

    #[test]
    fn code128_values_and_check() {
        // Start B 104 + P 48·1 + J 42·2 + J 42·3 + 1 17·4 + 2 18·5 + 3 19·6 + C 35·7 = 879 ≡ 55.
        assert_eq!(code128_values("PJJ123C").unwrap(), vec![104, 48, 42, 42, 17, 18, 19, 35, 55, 106]);
        assert!(code128_values("").is_none());
        assert!(code128_values("café").is_none(), "é is not in code set B");
        // Quiet-zone-free width: start, 3 symbols, check at 11 modules each, then 13 for stop.
        assert_eq!(code128_widths("abc").unwrap().iter().sum::<u32>(), 5 * 11 + 13);
    }

    #[test]
    fn barcode_bars_are_one_region_per_bar() {
        let path = barcode_path("A", 1.0, 20.0).unwrap();
        let widths = code128_widths("A").unwrap();
        let bars = widths.len().div_ceil(2);
        assert_eq!(crate::shaping::subpaths(&path).len(), bars);
        let bars_area: u32 = widths.iter().step_by(2).sum();
        assert!((path.area().abs() - f64::from(bars_area) * 20.0).abs() < 1e-6);
        assert_eq!(path.bounding_box(), Rect::new(0.0, 0.0, f64::from(widths.iter().sum::<u32>()), 20.0));
    }

    #[test]
    fn qr_code_has_finder_squares_and_its_size() {
        let (width, dark) = qr_modules("https://example.com").unwrap();
        assert_eq!(width, 25, "version 2");
        // The top-left finder pattern: a dark ring, a light ring, a dark 3 × 3 centre.
        let at = |x: usize, y: usize| dark[y * width + x];
        assert!((0..7).all(|i| at(i, 0) && at(0, i) && at(i, 6) && at(6, i)));
        assert!((1..6).all(|i| !at(i, 1) && !at(1, i)));
        assert!(at(3, 3));

        let path = qr_path("https://example.com", 2.0).unwrap();
        assert_eq!(path.bounding_box(), Rect::new(0.0, 0.0, 50.0, 50.0));
        let dark_area = dark.iter().filter(|d| **d).count() as f64 * 4.0;
        assert!((path.area().abs() - dark_area).abs() < 1e-6, "{} vs {dark_area}", path.area());
        assert!(qr_modules(&"x".repeat(5000)).is_none(), "too long");
    }
}

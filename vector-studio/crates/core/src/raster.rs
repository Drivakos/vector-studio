//! Bitmap effects: colour adjustments, blurs, distortions, noise and sharpening, worked
//! out on decoded pixels. Decoding and encoding the bitmap's file is the IO crate's job.

use std::f64::consts::PI;

/// Decoded pixels: straight (not premultiplied) RGBA, 8 bits a channel, rows top first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Pixels {
    /// Pixels from RGBA bytes; `None` when the length does not match the size.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize * 4).then_some(Self { width, height, data })
    }

    /// A `width` × `height` image of one colour.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        Self { width, height, data: rgba.repeat(width as usize * height as usize) }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data[i..i + 4].copy_from_slice(&rgba);
    }
}

/// The effects of the Effects menu that work on bitmaps, without their settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectKind {
    AutoAdjust,
    ToneCurve,
    BrightnessContrast,
    ColorBalance,
    HueSaturation,
    Grayscale,
    GaussianBlur,
    MotionBlur,
    RadialBlur,
    SmartBlur,
    Pixelate,
    Ripple,
    Swirl,
    Wind,
    AddNoise,
    RemoveNoise,
    Sharpen,
    UnsharpMask,
}

impl EffectKind {
    pub const ALL: [EffectKind; 18] = [
        EffectKind::AutoAdjust,
        EffectKind::ToneCurve,
        EffectKind::BrightnessContrast,
        EffectKind::ColorBalance,
        EffectKind::HueSaturation,
        EffectKind::Grayscale,
        EffectKind::GaussianBlur,
        EffectKind::MotionBlur,
        EffectKind::RadialBlur,
        EffectKind::SmartBlur,
        EffectKind::Pixelate,
        EffectKind::Ripple,
        EffectKind::Swirl,
        EffectKind::Wind,
        EffectKind::AddNoise,
        EffectKind::RemoveNoise,
        EffectKind::Sharpen,
        EffectKind::UnsharpMask,
    ];

    /// The effect's name, as its window title.
    pub fn label(self) -> &'static str {
        match self {
            EffectKind::AutoAdjust => "Auto Adjust",
            EffectKind::ToneCurve => "Tone Curve",
            EffectKind::BrightnessContrast => "Brightness/Contrast/Intensity",
            EffectKind::ColorBalance => "Color Balance",
            EffectKind::HueSaturation => "Hue/Saturation/Lightness",
            EffectKind::Grayscale => "Grayscale",
            EffectKind::GaussianBlur => "Gaussian Blur",
            EffectKind::MotionBlur => "Motion Blur",
            EffectKind::RadialBlur => "Radial Blur",
            EffectKind::SmartBlur => "Smart Blur",
            EffectKind::Pixelate => "Pixelate",
            EffectKind::Ripple => "Ripple",
            EffectKind::Swirl => "Swirl",
            EffectKind::Wind => "Wind",
            EffectKind::AddNoise => "Add Noise",
            EffectKind::RemoveNoise => "Remove Noise",
            EffectKind::Sharpen => "Sharpen",
            EffectKind::UnsharpMask => "Unsharp Mask",
        }
    }

    /// The effect with its starting settings.
    pub fn default_effect(self) -> Effect {
        match self {
            EffectKind::AutoAdjust => Effect::AutoAdjust,
            EffectKind::ToneCurve => Effect::ToneCurve { outputs: TONE_INPUTS, channel: ToneChannel::Rgb },
            EffectKind::BrightnessContrast => Effect::BrightnessContrast { brightness: 0.0, contrast: 0.0, intensity: 0.0 },
            EffectKind::ColorBalance => Effect::ColorBalance { cyan_red: 0.0, magenta_green: 0.0, yellow_blue: 0.0 },
            EffectKind::HueSaturation => Effect::HueSaturation { hue: 0.0, saturation: 0.0, lightness: 0.0 },
            EffectKind::Grayscale => Effect::Grayscale,
            EffectKind::GaussianBlur => Effect::GaussianBlur { radius: 3.0 },
            EffectKind::MotionBlur => Effect::MotionBlur { distance: 10.0, angle: 0.0 },
            EffectKind::RadialBlur => Effect::RadialBlur { angle: 10.0 },
            EffectKind::SmartBlur => Effect::SmartBlur { amount: 50.0 },
            EffectKind::Pixelate => Effect::Pixelate { size: 8 },
            EffectKind::Ripple => Effect::Ripple { amplitude: 5.0, period: 40.0 },
            EffectKind::Swirl => Effect::Swirl { angle: 90.0 },
            EffectKind::Wind => Effect::Wind { strength: 20.0, angle: 0.0 },
            EffectKind::AddNoise => Effect::AddNoise { level: 25.0 },
            EffectKind::RemoveNoise => Effect::RemoveNoise { radius: 1 },
            EffectKind::Sharpen => Effect::Sharpen { amount: 50.0 },
            EffectKind::UnsharpMask => Effect::UnsharpMask { amount: 100.0, radius: 2.0, threshold: 0.0 },
        }
    }

    /// Whether the effect has settings, so it opens a window first.
    pub fn has_settings(self) -> bool {
        !matches!(self, EffectKind::AutoAdjust | EffectKind::Grayscale)
    }
}

/// The colour channels a tone curve changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneChannel {
    #[default]
    Rgb,
    Red,
    Green,
    Blue,
}

impl ToneChannel {
    pub const ALL: [ToneChannel; 4] = [ToneChannel::Rgb, ToneChannel::Red, ToneChannel::Green, ToneChannel::Blue];

    pub fn label(self) -> &'static str {
        match self {
            ToneChannel::Rgb => "RGB",
            ToneChannel::Red => "Red",
            ToneChannel::Green => "Green",
            ToneChannel::Blue => "Blue",
        }
    }
}

/// The input levels of a tone curve's nodes, evenly spaced from black to white.
pub const TONE_INPUTS: [f64; 5] = [0.0, 63.75, 127.5, 191.25, 255.0];

/// The output level for each input level 0 to 255 of the tone curve through the nodes
/// ([`TONE_INPUTS`], `outputs`): a smooth curve that never overshoots its nodes
/// (monotone cubic, Fritsch–Carlson).
pub fn tone_lut(outputs: [f64; 5]) -> [u8; 256] {
    let (x, y) = (TONE_INPUTS, outputs.map(|v| v.clamp(0.0, 255.0)));
    let n = x.len();
    let slopes: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i])).collect();
    let mut tangents = vec![0.0; n];
    tangents[0] = slopes[0];
    tangents[n - 1] = slopes[n - 2];
    for i in 1..n - 1 {
        // Flat at a peak or a dip, so the curve stays between its neighbours.
        if slopes[i - 1] * slopes[i] > 0.0 {
            tangents[i] = 2.0 / (1.0 / slopes[i - 1] + 1.0 / slopes[i]);
        }
    }
    let mut lut = [0u8; 256];
    for (v, out) in lut.iter_mut().enumerate() {
        let v = v as f64;
        let i = x.iter().rposition(|xi| *xi <= v).unwrap_or(0).min(n - 2);
        let h = x[i + 1] - x[i];
        let t = (v - x[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let value = (2.0 * t3 - 3.0 * t2 + 1.0) * y[i] + (t3 - 2.0 * t2 + t) * h * tangents[i] + (-2.0 * t3 + 3.0 * t2) * y[i + 1] + (t3 - t2) * h * tangents[i + 1];
        *out = clamp_u8(value);
    }
    lut
}

/// A bitmap effect and its settings. Percentages run -100 to 100 unless noted; distances
/// are in pixels and angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Stretch each channel so its darkest and lightest values (ignoring the extreme
    /// half percent) reach black and white.
    AutoAdjust,
    /// Map each level through the curve with these outputs at [`TONE_INPUTS`].
    ToneCurve { outputs: [f64; 5], channel: ToneChannel },
    /// `intensity` brightens or darkens the mid-tones and leaves black and white alone.
    BrightnessContrast { brightness: f64, contrast: f64, intensity: f64 },
    ColorBalance { cyan_red: f64, magenta_green: f64, yellow_blue: f64 },
    /// `hue` in degrees, -180 to 180.
    HueSaturation { hue: f64, saturation: f64, lightness: f64 },
    Grayscale,
    GaussianBlur { radius: f64 },
    /// Smear along `angle` (0° to the right, clockwise) over `distance`.
    MotionBlur { distance: f64, angle: f64 },
    /// Spin about the centre over `angle`.
    RadialBlur { angle: f64 },
    /// Blur that keeps edges; `amount` 0 to 100.
    SmartBlur { amount: f64 },
    /// Square cells `size` pixels wide, each its average colour.
    Pixelate { size: u32 },
    /// Waves running across: each column moves up or down by up to `amplitude`, one wave
    /// every `period`.
    Ripple { amplitude: f64, period: f64 },
    /// Twist about the centre: `angle` at the centre, fading to none at the edge.
    Swirl { angle: f64 },
    /// Light streaks blown towards `angle` over `strength`.
    Wind { strength: f64, angle: f64 },
    /// Random grain; `level` 0 to 100.
    AddNoise { level: f64 },
    /// Median of the pixels within `radius`.
    RemoveNoise { radius: u32 },
    /// `amount` 0 to 100.
    Sharpen { amount: f64 },
    /// `amount` in percent (0 to 500); `threshold` 0 to 255 leaves differences that small
    /// alone.
    UnsharpMask { amount: f64, radius: f64, threshold: f64 },
}

impl Effect {
    pub fn kind(&self) -> EffectKind {
        match self {
            Effect::AutoAdjust => EffectKind::AutoAdjust,
            Effect::ToneCurve { .. } => EffectKind::ToneCurve,
            Effect::BrightnessContrast { .. } => EffectKind::BrightnessContrast,
            Effect::ColorBalance { .. } => EffectKind::ColorBalance,
            Effect::HueSaturation { .. } => EffectKind::HueSaturation,
            Effect::Grayscale => EffectKind::Grayscale,
            Effect::GaussianBlur { .. } => EffectKind::GaussianBlur,
            Effect::MotionBlur { .. } => EffectKind::MotionBlur,
            Effect::RadialBlur { .. } => EffectKind::RadialBlur,
            Effect::SmartBlur { .. } => EffectKind::SmartBlur,
            Effect::Pixelate { .. } => EffectKind::Pixelate,
            Effect::Ripple { .. } => EffectKind::Ripple,
            Effect::Swirl { .. } => EffectKind::Swirl,
            Effect::Wind { .. } => EffectKind::Wind,
            Effect::AddNoise { .. } => EffectKind::AddNoise,
            Effect::RemoveNoise { .. } => EffectKind::RemoveNoise,
            Effect::Sharpen { .. } => EffectKind::Sharpen,
            Effect::UnsharpMask { .. } => EffectKind::UnsharpMask,
        }
    }

    /// The effect for a copy of the bitmap `factor` times its size: distances in pixels
    /// scaled to match, so a small preview looks like the result.
    pub fn scaled(self, factor: f64) -> Effect {
        let px = |v: u32| (f64::from(v) * factor).round().max(1.0) as u32;
        match self {
            Effect::GaussianBlur { radius } => Effect::GaussianBlur { radius: radius * factor },
            Effect::MotionBlur { distance, angle } => Effect::MotionBlur { distance: distance * factor, angle },
            Effect::Pixelate { size } => Effect::Pixelate { size: px(size) },
            Effect::Ripple { amplitude, period } => Effect::Ripple { amplitude: amplitude * factor, period: period * factor },
            Effect::Wind { strength, angle } => Effect::Wind { strength: strength * factor, angle },
            Effect::RemoveNoise { radius } => Effect::RemoveNoise { radius: px(radius) },
            Effect::UnsharpMask { amount, radius, threshold } => Effect::UnsharpMask { amount, radius: radius * factor, threshold },
            other => other,
        }
    }

    /// `pixels` with the effect applied, the same size.
    pub fn apply(&self, pixels: &Pixels) -> Pixels {
        match *self {
            Effect::AutoAdjust => auto_adjust(pixels),
            Effect::ToneCurve { outputs, channel } => {
                let lut = tone_lut(outputs);
                let map = |v: f64| f64::from(lut[clamp_u8(v) as usize]);
                map_rgb(pixels, |[r, g, b]| match channel {
                    ToneChannel::Rgb => [map(r), map(g), map(b)],
                    ToneChannel::Red => [map(r), g, b],
                    ToneChannel::Green => [r, map(g), b],
                    ToneChannel::Blue => [r, g, map(b)],
                })
            }
            Effect::BrightnessContrast { brightness, contrast, intensity } => {
                let gain = ((100.0 + contrast.clamp(-100.0, 100.0)) / 100.0).powi(2);
                let gamma = 2f64.powf(-intensity.clamp(-100.0, 100.0) / 100.0);
                let shift = brightness.clamp(-100.0, 100.0) * 2.55;
                map_rgb(pixels, |c| c.map(|v| (255.0 * (v / 255.0).powf(gamma) - 127.5) * gain + 127.5 + shift))
            }
            Effect::ColorBalance { cyan_red, magenta_green, yellow_blue } => {
                let k = 1.275;
                map_rgb(pixels, |[r, g, b]| [r + cyan_red * k, g + magenta_green * k, b + yellow_blue * k])
            }
            Effect::HueSaturation { hue, saturation, lightness } => map_rgb(pixels, |rgb| {
                let (h, s, l) = to_hsl(rgb.map(|v| v / 255.0));
                let s = (s * (1.0 + saturation.clamp(-100.0, 100.0) / 100.0)).clamp(0.0, 1.0);
                let lightness = lightness.clamp(-100.0, 100.0) / 100.0;
                let l = if lightness > 0.0 { l + (1.0 - l) * lightness } else { l * (1.0 + lightness) };
                from_hsl((h + hue).rem_euclid(360.0), s, l).map(|v| v * 255.0)
            }),
            Effect::Grayscale => map_rgb(pixels, |[r, g, b]| [0.299 * r + 0.587 * g + 0.114 * b; 3]),
            Effect::GaussianBlur { radius } => from_premul(pixels, &gaussian(&to_premul(pixels), pixels.width, pixels.height, radius)),
            Effect::MotionBlur { distance, angle } => motion_blur(pixels, distance, angle),
            Effect::RadialBlur { angle } => radial_blur(pixels, angle),
            Effect::SmartBlur { amount } => smart_blur(pixels, amount),
            Effect::Pixelate { size } => pixelate(pixels, size.max(1)),
            Effect::Ripple { amplitude, period } => {
                let period = period.max(1.0);
                distort(pixels, |x, y| (x, y + amplitude * (2.0 * PI * x / period).sin()))
            }
            Effect::Swirl { angle } => swirl(pixels, angle),
            Effect::Wind { strength, angle } => wind(pixels, strength, angle),
            Effect::AddNoise { level } => add_noise(pixels, level),
            Effect::RemoveNoise { radius } => median(pixels, radius),
            Effect::Sharpen { amount } => unsharp(pixels, amount.clamp(0.0, 100.0) * 2.0, 1.5, 0.0),
            Effect::UnsharpMask { amount, radius, threshold } => unsharp(pixels, amount.clamp(0.0, 500.0), radius, threshold),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Colour

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Apply `f` to the colour of every pixel; alpha stays.
fn map_rgb(pixels: &Pixels, f: impl Fn([f64; 3]) -> [f64; 3]) -> Pixels {
    let mut out = pixels.clone();
    for px in out.data.as_chunks_mut::<4>().0 {
        let rgb = f([px[0], px[1], px[2]].map(f64::from));
        px[..3].copy_from_slice(&rgb.map(clamp_u8));
    }
    out
}

/// Hue in degrees, saturation and lightness 0 to 1, from RGB 0 to 1.
fn to_hsl([r, g, b]: [f64; 3]) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-12 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

fn auto_adjust(pixels: &Pixels) -> Pixels {
    let mut ranges = [(0.0, 255.0); 3];
    for (ch, range) in ranges.iter_mut().enumerate() {
        let mut histogram = [0usize; 256];
        let mut count = 0;
        for px in pixels.data.as_chunks::<4>().0.iter().filter(|px| px[3] > 0) {
            histogram[px[ch] as usize] += 1;
            count += 1;
        }
        let clip = count / 200;
        let (mut low, mut high) = (0usize, 255usize);
        let mut seen = 0;
        while low < 255 && seen + histogram[low] <= clip {
            seen += histogram[low];
            low += 1;
        }
        seen = 0;
        while high > 0 && seen + histogram[high] <= clip {
            seen += histogram[high];
            high -= 1;
        }
        if high > low {
            *range = (low as f64, high as f64);
        }
    }
    map_rgb(pixels, |rgb| std::array::from_fn(|i| (rgb[i] - ranges[i].0) * 255.0 / (ranges[i].1 - ranges[i].0)))
}

// ---------------------------------------------------------------------------------------
// Neighbourhood effects work on premultiplied colour, so transparent pixels do not bleed
// their colour into the opaque ones next to them.

type Premul = Vec<[f64; 4]>;

fn to_premul(pixels: &Pixels) -> Premul {
    pixels
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| {
            let a = f64::from(px[3]) / 255.0;
            [f64::from(px[0]) * a, f64::from(px[1]) * a, f64::from(px[2]) * a, f64::from(px[3])]
        })
        .collect()
}

fn from_premul(like: &Pixels, buf: &[[f64; 4]]) -> Pixels {
    let mut data = Vec::with_capacity(buf.len() * 4);
    for [r, g, b, a] in buf.iter().copied() {
        let a = a.clamp(0.0, 255.0);
        let k = if a > 0.0 { 255.0 / a } else { 0.0 };
        data.extend([clamp_u8(r * k), clamp_u8(g * k), clamp_u8(b * k), clamp_u8(a)]);
    }
    Pixels { width: like.width, height: like.height, data }
}

/// The premultiplied pixel nearest (x, y), clamped to the image.
fn at(buf: &[[f64; 4]], w: u32, h: u32, x: f64, y: f64) -> [f64; 4] {
    let x = (x.round() as i64).clamp(0, i64::from(w) - 1) as usize;
    let y = (y.round() as i64).clamp(0, i64::from(h) - 1) as usize;
    buf[y * w as usize + x]
}

/// (x, y) sampled between pixels, clamped to the image.
fn bilinear(buf: &[[f64; 4]], w: u32, h: u32, x: f64, y: f64) -> [f64; 4] {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let p00 = at(buf, w, h, x0, y0);
    let p10 = at(buf, w, h, x0 + 1.0, y0);
    let p01 = at(buf, w, h, x0, y0 + 1.0);
    let p11 = at(buf, w, h, x0 + 1.0, y0 + 1.0);
    std::array::from_fn(|i| {
        let top = p00[i] + (p10[i] - p00[i]) * fx;
        let bottom = p01[i] + (p11[i] - p01[i]) * fx;
        top + (bottom - top) * fy
    })
}

fn gaussian(buf: &[[f64; 4]], w: u32, h: u32, radius: f64) -> Premul {
    if radius <= 0.0 {
        return buf.to_vec();
    }
    let half = radius.ceil() as i64;
    let sigma = (radius / 2.0).max(0.5);
    let weights: Vec<f64> = (-half..=half).map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp()).collect();
    let total: f64 = weights.iter().sum();
    let pass = |src: &[[f64; 4]], dx: i64, dy: i64| -> Premul {
        let mut out = vec![[0.0; 4]; src.len()];
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let mut sum = [0.0; 4];
                for (k, wt) in (-half..=half).zip(&weights) {
                    let p = at(src, w, h, (x + k * dx) as f64, (y + k * dy) as f64);
                    for i in 0..4 {
                        sum[i] += p[i] * wt;
                    }
                }
                out[(y * w as i64 + x) as usize] = sum.map(|v| v / total);
            }
        }
        out
    };
    let across = pass(buf, 1, 0);
    pass(&across, 0, 1)
}

/// Each pixel the average of `samples(x, y)`.
fn average(pixels: &Pixels, samples: impl Fn(f64, f64) -> Vec<(f64, f64)>) -> Pixels {
    let buf = to_premul(pixels);
    let (w, h) = (pixels.width, pixels.height);
    let mut out = vec![[0.0; 4]; buf.len()];
    for y in 0..h {
        for x in 0..w {
            let points = samples(f64::from(x), f64::from(y));
            let mut sum = [0.0; 4];
            for (sx, sy) in &points {
                let p = bilinear(&buf, w, h, *sx, *sy);
                for i in 0..4 {
                    sum[i] += p[i];
                }
            }
            out[(y * w + x) as usize] = sum.map(|v| v / points.len() as f64);
        }
    }
    from_premul(pixels, &out)
}

fn motion_blur(pixels: &Pixels, distance: f64, angle: f64) -> Pixels {
    let n = distance.max(0.0).ceil() as usize + 1;
    let (dy, dx) = angle.to_radians().sin_cos();
    average(pixels, |x, y| {
        (0..n)
            .map(|k| {
                let t = if n == 1 { 0.0 } else { (k as f64 / (n - 1) as f64 - 0.5) * distance };
                (x + dx * t, y + dy * t)
            })
            .collect()
    })
}

fn radial_blur(pixels: &Pixels, angle: f64) -> Pixels {
    let (cx, cy) = (f64::from(pixels.width - 1) / 2.0, f64::from(pixels.height - 1) / 2.0);
    let spread = angle.to_radians();
    average(pixels, |x, y| {
        let (vx, vy) = (x - cx, y - cy);
        // Enough samples that neighbouring ones are about a pixel apart along the arc.
        let n = ((vx.hypot(vy) * spread.abs()).ceil() as usize + 1).min(64);
        (0..n)
            .map(|k| {
                let t = if n == 1 { 0.0 } else { (k as f64 / (n - 1) as f64 - 0.5) * spread };
                let (s, c) = t.sin_cos();
                (cx + vx * c - vy * s, cy + vx * s + vy * c)
            })
            .collect()
    })
}

fn smart_blur(pixels: &Pixels, amount: f64) -> Pixels {
    let buf = to_premul(pixels);
    let (w, h) = (pixels.width, pixels.height);
    // Neighbours this different in colour count for little, so edges stay sharp.
    let range = 2.0 + amount.clamp(0.0, 100.0) * 0.6;
    let mut out = vec![[0.0; 4]; buf.len()];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let centre = buf[(y * w as i64 + x) as usize];
            let mut sum = [0.0; 4];
            let mut total = 0.0;
            for j in -2..=2i64 {
                for i in -2..=2i64 {
                    let p = at(&buf, w, h, (x + i) as f64, (y + j) as f64);
                    let d2 = (0..4).map(|c| (p[c] - centre[c]).powi(2)).sum::<f64>();
                    let wt = (-((i * i + j * j) as f64) / 4.5 - d2 / (2.0 * range * range)).exp();
                    for c in 0..4 {
                        sum[c] += p[c] * wt;
                    }
                    total += wt;
                }
            }
            out[(y * w as i64 + x) as usize] = sum.map(|v| v / total);
        }
    }
    from_premul(pixels, &out)
}

fn pixelate(pixels: &Pixels, size: u32) -> Pixels {
    let buf = to_premul(pixels);
    let (w, h) = (pixels.width, pixels.height);
    let mut out = buf.clone();
    for by in (0..h).step_by(size as usize) {
        for bx in (0..w).step_by(size as usize) {
            let cell = || (by..(by + size).min(h)).flat_map(move |y| (bx..(bx + size).min(w)).map(move |x| (y * w + x) as usize));
            let mut sum = [0.0; 4];
            let mut n = 0.0;
            for i in cell() {
                for c in 0..4 {
                    sum[c] += buf[i][c];
                }
                n += 1.0;
            }
            for i in cell() {
                out[i] = sum.map(|v| v / n);
            }
        }
    }
    from_premul(pixels, &out)
}

/// Each pixel taken from where `source(x, y)` says, between pixels.
fn distort(pixels: &Pixels, source: impl Fn(f64, f64) -> (f64, f64)) -> Pixels {
    average(pixels, |x, y| vec![source(x, y)])
}

fn swirl(pixels: &Pixels, angle: f64) -> Pixels {
    let (cx, cy) = (f64::from(pixels.width - 1) / 2.0, f64::from(pixels.height - 1) / 2.0);
    let reach = cx.min(cy).max(1.0);
    let turn = angle.to_radians();
    distort(pixels, |x, y| {
        let (vx, vy) = (x - cx, y - cy);
        let r = vx.hypot(vy);
        if r >= reach {
            return (x, y);
        }
        let (s, c) = (-turn * (1.0 - r / reach)).sin_cos();
        (cx + vx * c - vy * s, cy + vx * s + vy * c)
    })
}

fn wind(pixels: &Pixels, strength: f64, angle: f64) -> Pixels {
    let buf = to_premul(pixels);
    let (w, h) = (pixels.width, pixels.height);
    let (dy, dx) = angle.to_radians().sin_cos();
    let n = strength.max(0.0).round() as usize;
    let mut out = buf.clone();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            // The brightest of the pixels upwind, fading with distance: light trails.
            for k in 1..=n {
                let fade = 1.0 - k as f64 / (n + 1) as f64;
                let p = at(&buf, w, h, f64::from(x) - dx * k as f64, f64::from(y) - dy * k as f64);
                for c in 0..4 {
                    out[i][c] = out[i][c].max(p[c] * fade);
                }
            }
        }
    }
    from_premul(pixels, &out)
}

fn add_noise(pixels: &Pixels, level: f64) -> Pixels {
    let spread = level.clamp(0.0, 100.0) * 1.275;
    // A fixed seed, so the same bitmap gets the same grain every time.
    let mut seed: u32 = 0x9E37_79B9;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
    };
    let mut out = pixels.clone();
    for px in out.data.as_chunks_mut::<4>().0.iter_mut().filter(|px| px[3] > 0) {
        for v in &mut px[..3] {
            *v = clamp_u8(f64::from(*v) + next() * spread);
        }
    }
    out
}

fn median(pixels: &Pixels, radius: u32) -> Pixels {
    let buf = to_premul(pixels);
    let (w, h) = (pixels.width, pixels.height);
    let r = i64::from(radius.max(1));
    let mut out = buf.clone();
    let mut window = Vec::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let px = &mut out[(y * w as i64 + x) as usize];
            for (c, v) in px.iter_mut().enumerate() {
                window.clear();
                for j in -r..=r {
                    for i in -r..=r {
                        window.push(at(&buf, w, h, (x + i) as f64, (y + j) as f64)[c]);
                    }
                }
                window.sort_by(f64::total_cmp);
                *v = window[window.len() / 2];
            }
        }
    }
    from_premul(pixels, &out)
}

fn unsharp(pixels: &Pixels, amount: f64, radius: f64, threshold: f64) -> Pixels {
    let buf = to_premul(pixels);
    let blurred = gaussian(&buf, pixels.width, pixels.height, radius.max(0.5));
    let k = amount / 100.0;
    let out: Premul = buf
        .iter()
        .zip(&blurred)
        .map(|(p, b)| {
            let a = p[3];
            std::array::from_fn(|c| {
                if c == 3 {
                    return a;
                }
                let d = p[c] - b[c];
                if d.abs() <= threshold { p[c] } else { (p[c] + d * k).clamp(0.0, a) }
            })
        })
        .collect();
    from_premul(pixels, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4 × 1: black, dark grey, light grey, white.
    fn ramp() -> Pixels {
        Pixels::new(4, 1, [0, 64, 192, 255].iter().flat_map(|v| [*v, *v, *v, 255]).collect()).unwrap()
    }

    /// `size` × `size`, black with a white square in the middle third.
    fn square(size: u32) -> Pixels {
        let mut p = Pixels::filled(size, size, [0, 0, 0, 255]);
        for y in size / 3..size * 2 / 3 {
            for x in size / 3..size * 2 / 3 {
                p.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        p
    }

    fn grey(p: &Pixels, x: u32, y: u32) -> u8 {
        p.pixel(x, y)[0]
    }

    #[test]
    fn colour_adjustments() {
        let bright = Effect::BrightnessContrast { brightness: 20.0, contrast: 0.0, intensity: 0.0 }.apply(&ramp());
        assert_eq!(bright.pixel(0, 0), [51, 51, 51, 255]);
        let contrast = Effect::BrightnessContrast { brightness: 0.0, contrast: 50.0, intensity: 0.0 }.apply(&ramp());
        assert!(grey(&contrast, 1, 0) < 64 && grey(&contrast, 2, 0) > 192, "{contrast:?}");
        let lighter = Effect::BrightnessContrast { brightness: 0.0, contrast: 0.0, intensity: 50.0 }.apply(&ramp());
        assert!(grey(&lighter, 1, 0) > 64);
        assert_eq!((grey(&lighter, 0, 0), grey(&lighter, 3, 0)), (0, 255), "black and white stay");

        let red = Effect::ColorBalance { cyan_red: 100.0, magenta_green: 0.0, yellow_blue: -100.0 }.apply(&Pixels::filled(1, 1, [100, 100, 200, 9]));
        assert_eq!(red.pixel(0, 0), [228, 100, 73, 9], "alpha stays");

        let red = Pixels::filled(1, 1, [255, 0, 0, 255]);
        assert_eq!(Effect::HueSaturation { hue: 120.0, saturation: 0.0, lightness: 0.0 }.apply(&red).pixel(0, 0), [0, 255, 0, 255]);
        assert_eq!(Effect::HueSaturation { hue: 0.0, saturation: -100.0, lightness: 0.0 }.apply(&red).pixel(0, 0), [128, 128, 128, 255]);
        assert_eq!(Effect::HueSaturation { hue: 0.0, saturation: 0.0, lightness: 100.0 }.apply(&red).pixel(0, 0), [255, 255, 255, 255]);

        assert_eq!(Effect::Grayscale.apply(&red).pixel(0, 0), [76, 76, 76, 255]);
    }

    #[test]
    fn auto_adjust_stretches_to_black_and_white() {
        let flat = Pixels::new(2, 1, vec![100, 50, 80, 255, 150, 90, 120, 255]).unwrap();
        let out = Effect::AutoAdjust.apply(&flat);
        assert_eq!(out.pixel(0, 0), [0, 0, 0, 255]);
        assert_eq!(out.pixel(1, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn blurs_soften_edges_and_keep_flat_areas() {
        let sharp = square(30);
        for effect in [
            Effect::GaussianBlur { radius: 3.0 },
            Effect::MotionBlur { distance: 8.0, angle: 0.0 },
            Effect::RadialBlur { angle: 30.0 },
        ] {
            let out = effect.apply(&sharp);
            assert_eq!(out.pixel(0, 0), [0, 0, 0, 255], "{effect:?}: far corner stays black");
            assert_eq!((out.width, out.height), (30, 30));
            let edge: Vec<u8> = (8..12).map(|x| grey(&out, x, 15)).chain((8..12).map(|y| grey(&out, 11, y))).collect();
            assert!(edge.iter().any(|v| *v > 0 && *v < 255) || matches!(effect, Effect::RadialBlur { .. }), "{effect:?}: {edge:?}");
        }
        // Motion blur along x leaves the top and bottom edges of the square sharp.
        let out = Effect::MotionBlur { distance: 8.0, angle: 0.0 }.apply(&sharp);
        assert_eq!((grey(&out, 15, 9), grey(&out, 15, 10)), (0, 255));
        // Radial blur smears the square's corners round the centre.
        let out = Effect::RadialBlur { angle: 30.0 }.apply(&sharp);
        assert!(grey(&out, 10, 10) < 255);
    }

    #[test]
    fn smart_blur_smooths_grain_and_keeps_edges() {
        let mut p = square(30);
        p.set_pixel(3, 3, [20, 20, 20, 255]);
        let out = Effect::SmartBlur { amount: 50.0 }.apply(&p);
        assert!(grey(&out, 3, 3) < 20, "grain smoothed");
        assert_eq!((grey(&out, 9, 15), grey(&out, 10, 15)), (0, 255), "edge kept");
    }

    #[test]
    fn blurs_do_not_darken_next_to_transparency() {
        let mut p = Pixels::filled(10, 1, [0, 0, 0, 0]);
        for x in 5..10 {
            p.set_pixel(x, 0, [255, 0, 0, 255]);
        }
        let out = Effect::GaussianBlur { radius: 2.0 }.apply(&p);
        let [r, g, b, a] = out.pixel(5, 0);
        assert_eq!((r, g, b), (255, 0, 0));
        assert!(a > 0 && a < 255);
    }

    #[test]
    fn pixelate_averages_cells() {
        let out = Effect::Pixelate { size: 2 }.apply(&ramp());
        assert_eq!(grey(&out, 0, 0), 32);
        assert_eq!(grey(&out, 1, 0), 32);
        assert_eq!(grey(&out, 3, 0), 224);
    }

    #[test]
    fn distortions_move_pixels() {
        let sharp = square(30);
        let rippled = Effect::Ripple { amplitude: 3.0, period: 20.0 }.apply(&sharp);
        assert_ne!(rippled, sharp);
        assert_eq!(rippled.pixel(15, 15), [255, 255, 255, 255], "centre still inside");
        let swirled = Effect::Swirl { angle: 90.0 }.apply(&sharp);
        assert_ne!(swirled, sharp);
        assert_eq!(swirled.pixel(0, 0), [0, 0, 0, 255], "corners are past the swirl");
        // Wind to the right leaves light trails right of the square only.
        let windy = Effect::Wind { strength: 5.0, angle: 0.0 }.apply(&sharp);
        assert!(grey(&windy, 21, 15) > 0);
        assert_eq!(grey(&windy, 8, 15), 0);
    }

    #[test]
    fn noise_and_remove_noise() {
        let grey_px = Pixels::filled(20, 20, [128, 128, 128, 255]);
        let noisy = Effect::AddNoise { level: 50.0 }.apply(&grey_px);
        assert_ne!(noisy, grey_px);
        assert_eq!(noisy, Effect::AddNoise { level: 50.0 }.apply(&grey_px), "same grain each time");
        assert!(noisy.data.as_chunks::<4>().0.iter().all(|px| px[3] == 255));

        let mut speck = grey_px.clone();
        speck.set_pixel(10, 10, [255, 255, 255, 255]);
        assert_eq!(Effect::RemoveNoise { radius: 1 }.apply(&speck), grey_px);
    }

    #[test]
    fn sharpening_raises_edge_contrast() {
        let soft = Effect::GaussianBlur { radius: 3.0 }.apply(&square(30));
        let before = (grey(&soft, 9, 15), grey(&soft, 10, 15));
        for effect in [Effect::Sharpen { amount: 100.0 }, Effect::UnsharpMask { amount: 200.0, radius: 2.0, threshold: 0.0 }] {
            let out = effect.apply(&soft);
            let after = (grey(&out, 9, 15), grey(&out, 10, 15));
            assert!(after.0 <= before.0 && after.1 > before.1, "{effect:?}: {before:?} → {after:?}");
        }
        // Differences under the threshold are left alone.
        let out = Effect::UnsharpMask { amount: 200.0, radius: 2.0, threshold: 255.0 }.apply(&soft);
        assert_eq!(out, soft);
    }

    #[test]
    fn tone_curve_maps_levels_through_its_nodes() {
        let straight = tone_lut(TONE_INPUTS);
        assert!((0..256).all(|v| i32::from(straight[v]) == v as i32), "the starting curve changes nothing");
        assert_eq!(Effect::ToneCurve { outputs: TONE_INPUTS, channel: ToneChannel::Rgb }.apply(&ramp()), ramp());

        // An S curve: darks darker, lights lighter, smooth and never past black or white.
        let s_curve = tone_lut([0.0, 40.0, 127.5, 215.0, 255.0]);
        assert!(s_curve[64] < 64 && s_curve[191] > 191 && s_curve[128] == 128, "{s_curve:?}");
        assert!(s_curve.windows(2).all(|w| w[0] <= w[1]), "rises all the way");
        // A dip: flat at the node, so the curve stays between its neighbours.
        let dip = tone_lut([255.0, 100.0, 0.0, 100.0, 255.0]);
        assert_eq!(dip[128], 0);
        assert!(dip[32] >= 100 && dip[96] <= 100, "between its neighbours: {dip:?}");

        let invert = Effect::ToneCurve { outputs: [255.0, 191.25, 127.5, 63.75, 0.0], channel: ToneChannel::Red };
        let out = invert.apply(&Pixels::filled(1, 1, [0, 10, 200, 255]));
        assert_eq!(out.pixel(0, 0), [255, 10, 200, 255], "only the red channel");
    }

    #[test]
    fn scaled_effects_keep_their_look_at_another_size() {
        assert_eq!(Effect::GaussianBlur { radius: 8.0 }.scaled(0.25), Effect::GaussianBlur { radius: 2.0 });
        assert_eq!(Effect::Pixelate { size: 2 }.scaled(0.1), Effect::Pixelate { size: 1 }, "at least a pixel");
        assert_eq!(Effect::Swirl { angle: 90.0 }.scaled(0.5), Effect::Swirl { angle: 90.0 }, "angles stay");
    }

    #[test]
    fn every_kind_round_trips_through_its_default() {
        for kind in EffectKind::ALL {
            assert_eq!(kind.default_effect().kind(), kind);
        }
    }
}

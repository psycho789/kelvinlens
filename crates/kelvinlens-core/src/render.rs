//! Rendering temperatures to colour: palette mapping with a choice of colour distribution,
//! isotherms, and composites with the visible photo (MSX-style edges and thermal fusion).
//!
//! The UI, exports and reports all render here, so they agree pixel for pixel.

use serde::{Deserialize, Serialize};

use crate::analysis::Isotherm;
use crate::palette::{NO_DATA, Palette};
use crate::thermogram::{Photo, Pip};

/// How temperatures in the display range are spread over the palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Distribution {
    /// Equal temperature steps get equal palette steps.
    #[default]
    TemperatureLinear,
    /// Linear in emitted radiance (blackbody at 10 µm), as a camera's raw signal is: more palette
    /// for the hotter part of the range.
    SignalLinear,
    /// Histogram equalisation: each palette step covers the same number of pixels.
    Histogram,
    /// Detail enhancement: equalised local average plus amplified local detail, so small
    /// differences show across a wide range.
    Detail,
}

/// Everything that decides how an image is coloured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub palette: Palette,
    /// Display range in kelvin.
    pub range: [f32; 2],
    #[serde(default)]
    pub distribution: Distribution,
    #[serde(default)]
    pub isotherms: Vec<Isotherm>,
}

impl Look {
    /// The same look with a 256-colour palette (custom palettes arrive with any number of colours).
    pub fn normalized(self) -> Look {
        Look { palette: self.palette.normalized(), ..self }
    }
}

/// Palette index for a temperature with linear mapping over `range`; `None` for NaN.
fn linear_index(kelvin: f32, range: [f32; 2]) -> Option<u8> {
    if kelvin.is_nan() {
        return None;
    }
    let span = range[1] - range[0];
    if span <= 0.0 {
        return Some(if kelvin < range[0] { 0 } else { 255 });
    }
    let t = (kelvin - range[0]) / span;
    Some(((t * 256.0).floor()).clamp(0.0, 255.0) as u8)
}

/// Relative blackbody radiance at 10 µm (second radiation constant 14388 µm·K).
fn radiance(kelvin: f32) -> f64 {
    let x = 14388.0 / (10.0 * kelvin.max(1.0) as f64);
    1.0 / x.exp_m1()
}

/// Palette index of every pixel for `look`'s range and distribution.
pub fn indices(kelvin: &[f32], width: usize, height: usize, look: &Look) -> Vec<Option<u8>> {
    let range = look.range;
    match look.distribution {
        Distribution::TemperatureLinear => kelvin.iter().map(|&k| linear_index(k, range)).collect(),
        Distribution::SignalLinear => {
            let (lo, hi) = (radiance(range[0]), radiance(range[1]));
            kelvin
                .iter()
                .map(|&k| {
                    if k.is_nan() {
                        None
                    } else if hi <= lo {
                        Some(if k < range[0] { 0 } else { 255 })
                    } else {
                        Some((((radiance(k) - lo) / (hi - lo)) * 256.0).floor().clamp(0.0, 255.0) as u8)
                    }
                })
                .collect()
        }
        Distribution::Histogram => {
            let t: Vec<f32> = kelvin.iter().map(|&k| normalized(k, range)).collect();
            let eq = Equalizer::new(&t);
            t.iter().map(|&v| (!v.is_nan()).then(|| to_index(eq.map(v)))).collect()
        }
        Distribution::Detail => {
            let t: Vec<f32> = kelvin.iter().map(|&k| normalized(k, range)).collect();
            let radius = (width.min(height) / 40).clamp(2, 12);
            let base = box_blur(&t, width, height, radius);
            let eq = Equalizer::new(&base);
            const GAIN: f32 = 2.5;
            t.iter().zip(&base).map(|(&v, &b)| (!v.is_nan()).then(|| to_index(eq.map(b) + GAIN * (v - b)))).collect()
        }
    }
}

/// Temperature as 0–1 within the range (clipped), NaN kept.
fn normalized(k: f32, range: [f32; 2]) -> f32 {
    if k.is_nan() {
        return f32::NAN;
    }
    let span = range[1] - range[0];
    if span <= 0.0 { if k < range[0] { 0.0 } else { 1.0 } } else { ((k - range[0]) / span).clamp(0.0, 1.0) }
}

fn to_index(t: f32) -> u8 {
    (t * 256.0).floor().clamp(0.0, 255.0) as u8
}

/// Maps 0–1 values through their cumulative distribution.
struct Equalizer {
    cdf: Vec<f32>,
}

impl Equalizer {
    const BINS: usize = 4096;

    fn new(values: &[f32]) -> Equalizer {
        let mut hist = vec![0u32; Self::BINS];
        let mut n = 0u32;
        for &v in values {
            if !v.is_nan() {
                hist[Self::bin(v)] += 1;
                n += 1;
            }
        }
        // Midpoint CDF: a value's share of pixels below it plus half of its own bin, so a flat
        // image maps to the middle of the palette and the ends stay reachable.
        let mut cdf = vec![0.5; Self::BINS];
        if n > 0 {
            let mut below = 0u32;
            for (i, &c) in hist.iter().enumerate() {
                cdf[i] = (below as f32 + c as f32 / 2.0) / n as f32;
                below += c;
            }
        }
        Equalizer { cdf }
    }

    fn bin(v: f32) -> usize {
        ((v.clamp(0.0, 1.0) * (Self::BINS - 1) as f32).round() as usize).min(Self::BINS - 1)
    }

    fn map(&self, v: f32) -> f32 {
        self.cdf[Self::bin(v)]
    }
}

/// Mean over a square window, skipping NaN (normalised convolution with integral images).
fn box_blur(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let (w1, h1) = (width + 1, height + 1);
    let mut sum = vec![0f64; w1 * h1];
    let mut cnt = vec![0u32; w1 * h1];
    for y in 0..height {
        for x in 0..width {
            let v = values[y * width + x];
            let (s, c) = if v.is_nan() { (0.0, 0) } else { (v as f64, 1) };
            let i = (y + 1) * w1 + x + 1;
            sum[i] = s + sum[i - 1] + sum[i - w1] - sum[i - w1 - 1];
            cnt[i] = c + cnt[i - 1] + cnt[i - w1] - cnt[i - w1 - 1];
        }
    }
    let mut out = vec![f32::NAN; width * height];
    for y in 0..height {
        let (y0, y1) = (y.saturating_sub(radius), (y + radius + 1).min(height));
        for x in 0..width {
            let (x0, x1) = (x.saturating_sub(radius), (x + radius + 1).min(width));
            let s = sum[y1 * w1 + x1] - sum[y0 * w1 + x1] - sum[y1 * w1 + x0] + sum[y0 * w1 + x0];
            let c = cnt[y1 * w1 + x1] + cnt[y0 * w1 + x0] - cnt[y0 * w1 + x1] - cnt[y1 * w1 + x0];
            if c > 0 {
                out[y * width + x] = (s / c as f64) as f32;
            }
        }
    }
    out
}

/// RGB (3 bytes per pixel) for the thermal image.
pub fn rgb(kelvin: &[f32], width: usize, height: usize, look: &Look) -> Vec<u8> {
    let colors = &look.palette.colors;
    let n = colors.len();
    let mut out: Vec<u8> = indices(kelvin, width, height, look)
        .into_iter()
        .flat_map(|i| i.map(|i| colors[(i as usize * n / 256).min(n.saturating_sub(1))]).unwrap_or(NO_DATA))
        .collect();
    paint_isotherms(&mut out, kelvin, &look.isotherms);
    out
}

/// Colours isotherm pixels; later isotherms paint over earlier ones.
fn paint_isotherms(rgb: &mut [u8], kelvin: &[f32], isotherms: &[Isotherm]) {
    for iso in isotherms {
        let m = iso.kind.matcher();
        let a = iso.opacity.clamp(0.0, 1.0);
        for (i, &k) in kelvin.iter().enumerate() {
            if m(k) {
                for c in 0..3 {
                    let p = &mut rgb[i * 3 + c];
                    *p = (*p as f32 * (1.0 - a) + iso.color[c] as f32 * a).round() as u8;
                }
            }
        }
    }
}

/// RGBA with 4 bytes per pixel, as a canvas wants it.
pub fn rgba(rgb: &[u8]) -> Vec<u8> {
    let (pixels, _) = rgb.as_chunks::<3>();
    pixels.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
}

// ---------------------------------------------------------------------------- photo composites

/// How the thermal image is shown over the photo.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Fusion {
    /// Thermal colours with the photo's edges embossed on them (like FLIR MSX), over the part of
    /// the photo the thermal image covers. `strength` 0–1.
    Msx { strength: f32 },
    /// The photo, with thermal colours only where the temperature is in `lo..=hi` (kelvin).
    Interval { lo: f32, hi: f32 },
}

/// A composite image and where it sits: `placement` maps thermal pixels onto it the same way the
/// picture-in-picture geometry maps them onto the photo.
#[derive(Clone, Debug, PartialEq)]
pub struct Composite {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    /// Thermal source rectangle `[x, y, w, h]` and where it lands in composite pixels.
    pub source: [f32; 4],
    pub destination: [f32; 4],
}

/// A manual correction of where the thermal image sits on the photo: shift `dx`, `dy` (photo
/// pixels) and `scale` (1.0 = as recorded), applied around the thermal image's centre.
pub type Alignment = [f32; 3];

pub const NO_ALIGNMENT: Alignment = [0.0, 0.0, 1.0];

/// `placement` with a manual alignment applied to the destination.
pub fn placement_aligned(
    pip: Option<Pip>,
    width: usize,
    height: usize,
    photo: &Photo,
    align: Alignment,
) -> ([f32; 4], [f32; 4]) {
    let (src, [x, y, w, h]) = placement(pip, width, height, photo);
    let s = if align[2].is_finite() && align[2] > 0.05 { align[2] } else { 1.0 };
    (src, [x + align[0] - w * (s - 1.0) / 2.0, y + align[1] - h * (s - 1.0) / 2.0, w * s, h * s])
}

/// Thermal pixels → photo pixels for the whole thermal frame. The PiP geometry maps the camera's
/// picture-in-picture crop; the same linear mapping is extended to the full frame (as FLIR does
/// for blending and fusion). Without geometry the thermal image is stretched over the photo.
/// `src/viewer.ts` implements the same mapping for the overlay; keep the two in step.
pub fn placement(pip: Option<Pip>, width: usize, height: usize, photo: &Photo) -> ([f32; 4], [f32; 4]) {
    let full = [0.0, 0.0, width as f32, height as f32];
    match pip {
        Some(p) if p.source.w > 0 && p.source.h > 0 => {
            let sx = p.destination.w as f32 / p.source.w as f32;
            let sy = p.destination.h as f32 / p.source.h as f32;
            let x = p.destination.x as f32 - p.source.x as f32 * sx;
            let y = p.destination.y as f32 - p.source.y as f32 * sy;
            (full, [x, y, width as f32 * sx, height as f32 * sy])
        }
        _ => (full, [0.0, 0.0, photo.width as f32, photo.height as f32]),
    }
}

/// What `fuse` needs: the temperatures with their size and colouring, the photo and where the
/// thermal image sits on it, and how to combine them.
pub struct FuseRequest<'a> {
    pub kelvin: &'a [f32],
    pub width: usize,
    pub height: usize,
    pub look: &'a Look,
    pub photo: &'a Photo,
    pub pip: Option<Pip>,
    pub fusion: Fusion,
    /// Widest output in pixels.
    pub max_width: usize,
    pub align: Alignment,
}

/// Renders a thermal/photo composite at most `req.max_width` pixels wide.
pub fn fuse(req: &FuseRequest) -> Composite {
    let (src, dst) = placement_aligned(req.pip, req.width, req.height, req.photo, req.align);
    let thermal = rgb(req.kelvin, req.width, req.height, req.look);
    match req.fusion {
        Fusion::Msx { strength } => fuse_msx(req, &thermal, src, dst, strength),
        Fusion::Interval { lo, hi } => fuse_interval(req, &thermal, src, dst, (lo, hi)),
    }
}

/// MSX: the thermal image with the photo's edges added wherever the photo overlaps it.
fn fuse_msx(req: &FuseRequest, thermal: &[u8], src: [f32; 4], dst: [f32; 4], strength: f32) -> Composite {
    let (width, height, photo) = (req.width, req.height, req.photo);
    // Output covers the whole thermal image, at most `max_width` wide; the photo's edges
    // are added wherever the photo overlaps it.
    let scale = (req.max_width as f32 / width as f32).clamp(1.0, 8.0).floor().max(1.0);
    let (ow, oh) = (width * scale as usize, height * scale as usize);
    // Photo pixels per output pixel, for box-averaging the luminance.
    let foot = (dst[2] / src[2] / scale).max(1.0);
    let to_photo = |ox: usize, oy: usize| {
        let (tx, ty) = ((ox as f32 + 0.5) / scale, (oy as f32 + 0.5) / scale);
        (dst[0] + (tx - src[0]) * dst[2] / src[2], dst[1] + (ty - src[1]) * dst[3] / src[3])
    };
    let mut lum = vec![f32::NAN; ow * oh];
    for oy in 0..oh {
        for ox in 0..ow {
            let (px, py) = to_photo(ox, oy);
            lum[oy * ow + ox] = photo_luminance_at(photo, px, py, foot);
        }
    }
    let radius = (ow.min(oh) / 160).clamp(1, 4);
    let blur = box_blur(&lum, ow, oh, radius);
    let gain = 2.2 * strength.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(ow * oh * 3);
    for oy in 0..oh {
        for ox in 0..ow {
            let c = sample_rgb(thermal, width, height, (ox as f32 + 0.5) / scale, (oy as f32 + 0.5) / scale);
            let i = oy * ow + ox;
            let edge = if lum[i].is_finite() && blur[i].is_finite() { (lum[i] - blur[i]) * gain } else { 0.0 };
            out.extend(c.map(|v| (v + edge * 255.0).round().clamp(0.0, 255.0) as u8));
        }
    }
    Composite {
        width: ow,
        height: oh,
        rgb: out,
        source: [0.0, 0.0, width as f32, height as f32],
        destination: [0.0, 0.0, width as f32 * scale, height as f32 * scale],
    }
}

/// Interval: the thermal image where the temperature is within `range`, the photo elsewhere.
fn fuse_interval(req: &FuseRequest, thermal: &[u8], src: [f32; 4], dst: [f32; 4], range: (f32, f32)) -> Composite {
    let (kelvin, width, height, photo) = (req.kelvin, req.width, req.height, req.photo);
    let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
    let scale = (req.max_width as f32 / photo.width as f32).min(1.0);
    let ow = ((photo.width as f32 * scale).round() as usize).max(1);
    let oh = ((photo.height as f32 * scale).round() as usize).max(1);
    let mut out = Vec::with_capacity(ow * oh * 3);
    for oy in 0..oh {
        for ox in 0..ow {
            let (px, py) = ((ox as f32 + 0.5) / scale, (oy as f32 + 0.5) / scale);
            let tx = src[0] + (px - dst[0]) * src[2] / dst[2];
            let ty = src[1] + (py - dst[1]) * src[3] / dst[3];
            let inside = tx >= src[0] && ty >= src[1] && tx < src[0] + src[2] && ty < src[1] + src[3];
            let k = if inside { nearest(kelvin, width, height, tx, ty) } else { f32::NAN };
            if k >= lo && k <= hi {
                out.extend(sample_rgb(thermal, width, height, tx, ty).map(|v| v.round().clamp(0.0, 255.0) as u8));
            } else {
                let (sx, sy) = ((px as usize).min(photo.width - 1), (py as usize).min(photo.height - 1));
                let j = (sy * photo.width + sx) * 3;
                out.extend_from_slice(&photo.rgb[j..j + 3]);
            }
        }
    }
    let destination = [dst[0] * scale, dst[1] * scale, dst[2] * scale, dst[3] * scale];
    Composite { width: ow, height: oh, rgb: out, source: src, destination }
}

fn nearest(kelvin: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    if x < 0.0 || y < 0.0 {
        return f32::NAN;
    }
    let (ix, iy) = (x as usize, y as usize);
    if ix >= width || iy >= height { f32::NAN } else { kelvin[iy * width + ix] }
}

/// Bilinear sample of an RGB image at continuous pixel coordinates (pixel centres at +0.5).
fn sample_rgb(rgb: &[u8], width: usize, height: usize, x: f32, y: f32) -> [f32; 3] {
    let fx = (x - 0.5).clamp(0.0, (width - 1) as f32);
    let fy = (y - 0.5).clamp(0.0, (height - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
    let px = |x: usize, y: usize, c: usize| rgb[(y * width + x) * 3 + c] as f32;
    [0, 1, 2].map(|c| {
        let top = px(x0, y0, c) * (1.0 - ax) + px(x1, y0, c) * ax;
        let bottom = px(x0, y1, c) * (1.0 - ax) + px(x1, y1, c) * ax;
        top * (1.0 - ay) + bottom * ay
    })
}

/// Luminance (0–1) of the photo around `(x, y)`, averaged over a `foot`-pixel square; NaN outside
/// the photo.
fn photo_luminance_at(photo: &Photo, x: f32, y: f32, foot: f32) -> f32 {
    let half = foot / 2.0;
    let (x0, x1) = ((x - half).floor().max(0.0) as i64, ((x + half).ceil() as i64).min(photo.width as i64));
    let (y0, y1) = ((y - half).floor().max(0.0) as i64, ((y + half).ceil() as i64).min(photo.height as i64));
    if x < 0.0 || y < 0.0 || x >= photo.width as f32 || y >= photo.height as f32 || x1 <= x0 || y1 <= y0 {
        return f32::NAN;
    }
    let (mut s, mut n) = (0f32, 0u32);
    for py in y0..y1 {
        for px in x0..x1 {
            let j = (py as usize * photo.width + px as usize) * 3;
            s += 0.299 * photo.rgb[j] as f32 + 0.587 * photo.rgb[j + 1] as f32 + 0.114 * photo.rgb[j + 2] as f32;
            n += 1;
        }
    }
    s / n as f32 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::IsothermKind;
    use crate::palette;

    fn look(distribution: Distribution) -> Look {
        Look { palette: palette::by_id("white-hot").unwrap(), range: [280.0, 300.0], distribution, isotherms: vec![] }
    }

    #[test]
    fn linear_index_clips_and_maps_range_ends() {
        let r = [280.0, 300.0];
        assert_eq!(linear_index(270.0, r), Some(0));
        assert_eq!(linear_index(280.0, r), Some(0));
        assert_eq!(linear_index(299.99, r), Some(255));
        assert_eq!(linear_index(300.0, r), Some(255));
        assert_eq!(linear_index(290.0, r), Some(128));
        assert_eq!(linear_index(f32::NAN, r), None);
        assert_eq!(linear_index(290.0, [290.0, 290.0]), Some(255));
    }

    #[test]
    fn rgb_uses_no_data_colour_for_nan() {
        let out = rgb(&[280.0, f32::NAN, 300.0], 3, 1, &look(Distribution::TemperatureLinear));
        assert_eq!(out, vec![0, 0, 0, 128, 128, 128, 255, 255, 255]);
    }

    #[test]
    fn signal_linear_gives_hot_end_more_palette() {
        let lin = indices(&[290.0], 1, 1, &look(Distribution::TemperatureLinear))[0].unwrap();
        let sig = indices(&[290.0], 1, 1, &look(Distribution::SignalLinear))[0].unwrap();
        assert_eq!(lin, 128);
        // Radiance rises faster than linearly, so mid-range temperature sits below mid-palette.
        assert!(sig < 128 && sig > 90, "{sig}");
        let ends = indices(&[280.0, 300.0], 2, 1, &look(Distribution::SignalLinear));
        assert_eq!(ends, vec![Some(0), Some(255)]);
    }

    #[test]
    fn histogram_spreads_clustered_values() {
        // 90 pixels at 281 K and 10 at 299 K: linear puts the cluster near black; equalisation
        // moves it to the middle of its share.
        let mut k = vec![281.0; 90];
        k.extend(vec![299.0; 10]);
        let idx = indices(&k, 10, 10, &look(Distribution::Histogram));
        assert_eq!(idx[0], Some(115)); // 0.45 · 256
        assert_eq!(idx[99], Some(243)); // 0.95 · 256
        let flat = indices(&[290.0; 4], 2, 2, &look(Distribution::Histogram));
        assert_eq!(flat[0], Some(128));
    }

    #[test]
    fn detail_mode_keeps_order_and_shows_small_steps() {
        // A wide gradient with a 0.2 K bump; linear mapping over a 20 K range barely shows it.
        let (w, h) = (40, 40);
        let mut k: Vec<f32> = (0..w * h).map(|i| 280.0 + 20.0 * (i % w) as f32 / w as f32).collect();
        k[20 * w + 20] += 0.2;
        let detail = indices(&k, w, h, &look(Distribution::Detail));
        let linear = indices(&k, w, h, &look(Distribution::TemperatureLinear));
        let step = |idx: &[Option<u8>]| idx[20 * w + 20].unwrap() as i32 - idx[20 * w + 19].unwrap() as i32;
        assert!(step(&detail) > step(&linear), "{} vs {}", step(&detail), step(&linear));
        // Left to right still goes from cold to hot.
        assert!(detail[20 * w + 2].unwrap() < detail[20 * w + 37].unwrap());
        assert!(indices(&[f32::NAN], 1, 1, &look(Distribution::Detail))[0].is_none());
    }

    #[test]
    fn isotherms_paint_over_palette() {
        let mut l = look(Distribution::TemperatureLinear);
        l.isotherms = vec![
            Isotherm { kind: IsothermKind::Above { t: 295.0 }, color: [255, 0, 0], opacity: 1.0 },
            Isotherm { kind: IsothermKind::Below { t: 285.0 }, color: [0, 0, 255], opacity: 0.5 },
        ];
        let out = rgb(&[280.0, 290.0, 299.0], 3, 1, &l);
        assert_eq!(&out[0..3], &[0, 0, 128]); // black half-blended with blue
        assert_eq!(&out[3..6], &[128, 128, 128]);
        assert_eq!(&out[6..9], &[255, 0, 0]);
    }

    fn photo() -> Photo {
        // 8×8 photo: left half black, right half white (a vertical edge in the middle).
        let rgb = (0..64).flat_map(|i| if i % 8 < 4 { [0u8; 3] } else { [255u8; 3] }).collect();
        Photo { width: 8, height: 8, rgb }
    }

    #[test]
    fn msx_embosses_photo_edges_on_flat_thermal() {
        let k = vec![290.0; 16];
        let (l, p) = (look(Distribution::TemperatureLinear), photo());
        let req = FuseRequest {
            kelvin: &k,
            width: 4,
            height: 4,
            look: &l,
            photo: &p,
            pip: None,
            fusion: Fusion::Msx { strength: 1.0 },
            max_width: 8,
            align: NO_ALIGNMENT,
        };
        // 4×4 thermal over the 8×8 photo; output scaled 2× (max width 8) covers the thermal image.
        let c = fuse(&req);
        assert_eq!((c.width, c.height), (8, 8));
        let lum = |x: usize| c.rgb[(4 * 8 + x) * 3];
        // Flat thermal is mid-grey; the dark side of the edge darkens and the bright side brightens.
        assert!(lum(3) < 128 && lum(4) > 128, "{} {}", lum(3), lum(4));
        assert_eq!(lum(0), 128);
        assert_eq!((c.source, c.destination), ([0.0, 0.0, 4.0, 4.0], [0.0, 0.0, 8.0, 8.0]));
        // Where the photo does not reach (thermal wider than the photo), no edges are invented.
        let wide = Pip {
            source: super::super::thermogram::Rect { x: 0, y: 0, w: 4, h: 4 },
            destination: super::super::thermogram::Rect { x: -8, y: 0, w: 16, h: 8 },
        };
        let c = fuse(&FuseRequest { pip: Some(wide), ..req });
        assert_eq!(c.rgb[0], 128);
        // Strength 0 is plain thermal.
        let plain = fuse(&FuseRequest { fusion: Fusion::Msx { strength: 0.0 }, ..req });
        assert!(plain.rgb.iter().all(|&v| v == 128));
    }

    #[test]
    fn interval_fusion_shows_thermal_only_in_range() {
        let k = vec![285.0, 295.0, 285.0, 295.0];
        let (l, p) = (look(Distribution::TemperatureLinear), photo());
        let c = fuse(&FuseRequest {
            kelvin: &k,
            width: 2,
            height: 2,
            look: &l,
            photo: &p,
            pip: None,
            fusion: Fusion::Interval { lo: 290.0, hi: 300.0 },
            max_width: 1000,
            align: NO_ALIGNMENT,
        });
        // Left column (285 K) shows the photo (black); right column (295 K) shows thermal.
        let at = |x: usize, y: usize| &c.rgb[(y * 8 + x) * 3..(y * 8 + x) * 3 + 3];
        assert_eq!(at(0, 0), &[0, 0, 0]);
        // 295 K is 0.75 of the range: palette index 192.
        assert_eq!(at(7, 0), &[192, 192, 192]);
    }

    #[test]
    fn placement_extends_the_pip_mapping_to_the_whole_frame() {
        use crate::thermogram::Rect;
        let pip = Pip {
            source: Rect { x: 160, y: 120, w: 320, h: 240 },
            destination: Rect { x: 1000, y: 800, w: 400, h: 300 },
        };
        let p = Photo { width: 2592, height: 1944, rgb: vec![] };
        let (src, dst) = placement(Some(pip), 640, 480, &p);
        assert_eq!(src, [0.0, 0.0, 640.0, 480.0]);
        // 1.25 photo pixels per thermal pixel; thermal (160, 120) still lands on (1000, 800).
        assert_eq!(dst, [800.0, 650.0, 800.0, 600.0]);
    }

    #[test]
    fn alignment_shifts_and_scales_around_the_centre() {
        let p = Photo { width: 100, height: 100, rgb: vec![] };
        let (_, d) = placement_aligned(None, 10, 10, &p, [5.0, -3.0, 1.1]);
        assert!((d[0] - 0.0).abs() < 1e-4 && (d[1] + 8.0).abs() < 1e-4, "{d:?}");
        assert!((d[2] - 110.0).abs() < 1e-4 && (d[3] - 110.0).abs() < 1e-4);
        assert_eq!(placement_aligned(None, 10, 10, &p, NO_ALIGNMENT).1, [0.0, 0.0, 100.0, 100.0]);
    }

    #[test]
    fn look_normalized_resamples_the_palette_to_256_colours() {
        let mut l = look(Distribution::Detail);
        l.palette.colors = vec![[0, 0, 0], [255, 255, 255]];
        let n = l.clone().normalized();
        assert_eq!(n.palette.colors.len(), 256);
        assert_eq!((n.range, n.distribution), (l.range, l.distribution));
    }

    #[test]
    fn look_json_round_trip() {
        let l = look(Distribution::Detail);
        let back: Look = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back, l);
        let minimal: Look =
            serde_json::from_str(r#"{"palette":{"id":"x","name":"X","colors":[[0,0,0]]},"range":[1,2]}"#).unwrap();
        assert_eq!(minimal.distribution, Distribution::TemperatureLinear);
    }
}

//! InfiRay "IJPEG" radiometric JPEGs, as saved by the P2 Pro phone app.
//!
//! Layout as documented by ExifTool's `InfiRay.pm` (all little-endian):
//! - APP2 starting `....IJPEG\0`: sizes of the three data blocks.
//! - Consecutive APP3 segments, joined: the grey IR picture, then the temperature map, then an
//!   optional visible picture. Temperatures are `u16` in 1/64 K, sensor orientation.
//! - APP5: emissivity, reflected and environment temperature, distance, humidity, and which
//!   readings (centre, max, min) the camera showed.
//! - APP9: sensor name and serial number.
//!
//! The file does not say how the saved picture is rotated relative to the sensor, so the
//! orientation is found by matching the temperatures against the picture itself.

use crate::bytes::{f32_le, text, u16_at, u32_at};
use crate::jpeg;
use crate::params::Params;

// APP2 `IJPEG` header (offsets from ExifTool's InfiRay.pm).
const HEADER_MIN_LEN: usize = 0x50;
/// The header's six-byte marker `IJPEG\0`.
const HEADER_MAGIC_AT: usize = 4;
/// Bytes of the grey IR picture at the start of the joined APP3 data (`u64`).
const HEADER_IR_SIZE_AT: usize = 0x20;
/// Bytes of the temperature map that follows it (`u64`).
const HEADER_TEMP_SIZE_AT: usize = 0x30;
/// Width of the temperature map in pixels (`u16`).
const HEADER_TEMP_WIDTH_AT: usize = 0x3a;
/// The app's palette number (`u16`).
const HEADER_PALETTE_AT: usize = 0x11;

// APP5 measurement settings (little-endian `f32` unless noted).
const APP5_MIN_LEN: usize = 38;
const APP5_ATMOSPHERE_AT: usize = 0x00; // degrees Celsius
const APP5_DISTANCE_AT: usize = 0x04; // metres
const APP5_EMISSIVITY_AT: usize = 0x08;
const APP5_HUMIDITY_AT: usize = 0x0c; // 0-1
const APP5_REFLECTED_AT: usize = 0x10; // degrees Celsius
/// One byte, 1 when the camera drew its centre reading. It reads the pixel just up and left of
/// the middle, at `(width / 2 - 0.5, height / 2 - 0.5)` (see `thermogram.rs`).
const APP5_SHOW_CENTRE_AT: usize = 0x21;

// APP9 sensor information.
const APP9_MIN_LEN: usize = 0x148;
const APP9_SERIAL_AT: usize = 0x0c0;
const APP9_SERIAL_LEN: usize = 32;
const APP9_FOCAL_LENGTH_AT: usize = 0x144;

// EXIF IFD0 tags and the size of one IFD entry.
const TAG_MAKE: u16 = 0x010f;
const TAG_MODEL: u16 = 0x0110;
const TAG_DATE_TIME: u16 = 0x0132;
const IFD_ENTRY_LEN: usize = 12;

/// One decoded IJPEG file, in the orientation of the saved picture.
#[derive(Clone, Debug)]
pub struct InfiRayJpeg {
    pub width: usize,
    pub height: usize,
    pub kelvin: Vec<f32>,
    pub params: Params,
    pub make: Option<String>,
    pub model: Option<String>,
    pub date_time: Option<String>,
    pub serial_number: Option<String>,
    pub focal_length: Option<f32>,
    /// The camera showed its centre reading on the picture.
    pub show_centre: bool,
    /// The app's palette number (3 is iron, per the P2 Pro protocol).
    pub palette: Option<u16>,
}

/// A transform from sensor pixels to picture pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Normal,
    Rotate180,
    FlipHorizontal,
    FlipVertical,
    Rotate90,
    Rotate270,
    Transpose,
    Transverse,
}

impl Orientation {
    fn swaps_axes(self) -> bool {
        matches!(
            self,
            Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Transpose | Orientation::Transverse
        )
    }

    /// Source (sensor) pixel for output pixel `(x, y)`, given the sensor size `w × h`.
    fn source(self, x: usize, y: usize, w: usize, h: usize) -> (usize, usize) {
        match self {
            Orientation::Normal => (x, y),
            Orientation::Rotate180 => (w - 1 - x, h - 1 - y),
            Orientation::FlipHorizontal => (w - 1 - x, y),
            Orientation::FlipVertical => (x, h - 1 - y),
            // Clockwise: output column x comes from sensor row h-1-x.
            Orientation::Rotate90 => (y, h - 1 - x),
            Orientation::Rotate270 => (w - 1 - y, x),
            Orientation::Transpose => (y, x),
            Orientation::Transverse => (w - 1 - y, h - 1 - x),
        }
    }
}

/// Whether `bytes` is an InfiRay IJPEG file.
pub fn is_ijpeg(bytes: &[u8]) -> bool {
    segments(bytes).is_some_and(|s| s.iter().any(|(m, d)| *m == 0xe2 && is_header(d)))
}

fn is_header(d: &[u8]) -> bool {
    d.len() >= HEADER_MIN_LEN && &d[HEADER_MAGIC_AT..HEADER_MAGIC_AT + 6] == b"IJPEG\0"
}

/// APP segments before the image data, as (marker, payload). `None` unless the file is a
/// well-formed JPEG up to its scan.
fn segments(bytes: &[u8]) -> Option<Vec<(u8, &[u8])>> {
    let scan = jpeg::segments(bytes).filter(|s| s.well_formed)?;
    Some(scan.segments.into_iter().map(|s| (s.marker, s.data)).collect())
}

fn u64_le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Decodes an IJPEG file. `None` if it is not one or the data is inconsistent.
pub fn decode(bytes: &[u8]) -> Option<InfiRayJpeg> {
    let segs = segments(bytes)?;
    let header = segs.iter().find(|(m, d)| *m == 0xe2 && is_header(d))?.1;
    let ir_size = u64_le(header, HEADER_IR_SIZE_AT)? as usize;
    let temp_size = u64_le(header, HEADER_TEMP_SIZE_AT)? as usize;
    let temp_width = u16_at(header, HEADER_TEMP_WIDTH_AT, true)? as usize;

    // ExifTool joins every APP3 segment that follows the header.
    let data: Vec<u8> = segs.iter().filter(|(m, _)| *m == 0xe3).flat_map(|(_, d)| d.iter().copied()).collect();
    let temps = data.get(ir_size..ir_size.checked_add(temp_size)?)?;
    let count = temp_size / 2;
    if temp_width == 0 || count == 0 || !count.is_multiple_of(temp_width) {
        return None;
    }
    let (sw, sh) = (temp_width, count / temp_width);
    let sensor: Vec<f32> = temps.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c) as f32 / 64.0).collect();

    let mut params = Params::default();
    let mut show_centre = false;
    if let Some((_, p)) = segs.iter().find(|(m, d)| *m == 0xe5 && d.len() >= APP5_MIN_LEN) {
        let c = |v: f32| v + 273.15;
        params.atmosphere_k = f32_le(p, APP5_ATMOSPHERE_AT).map(c);
        params.distance_m = f32_le(p, APP5_DISTANCE_AT);
        params.emissivity = f32_le(p, APP5_EMISSIVITY_AT).filter(|e| *e > 0.0 && *e <= 1.0);
        params.humidity = f32_le(p, APP5_HUMIDITY_AT);
        params.reflected_k = f32_le(p, APP5_REFLECTED_AT).map(c);
        show_centre = p[APP5_SHOW_CENTRE_AT] == 1;
    }
    let sensor_info = segs.iter().find(|(m, d)| *m == 0xe9 && d.len() >= APP9_MIN_LEN).map(|(_, d)| *d);
    let (make, model, date_time) = exif_ifd0(&segs);

    let orientation = match picture_luma(bytes) {
        Some((lw, lh, luma)) => best_orientation(&sensor, sw, sh, &luma, lw, lh),
        None => Orientation::Normal,
    };
    let (width, height) = if orientation.swaps_axes() { (sh, sw) } else { (sw, sh) };
    let kelvin = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (sx, sy) = orientation.source(x, y, sw, sh);
            sensor[sy * sw + sx]
        })
        .collect();

    Some(InfiRayJpeg {
        width,
        height,
        kelvin,
        params,
        make,
        model,
        date_time,
        serial_number: sensor_info.and_then(|d| text(d, APP9_SERIAL_AT, APP9_SERIAL_LEN)),
        focal_length: sensor_info.and_then(|d| f32_le(d, APP9_FOCAL_LENGTH_AT)).filter(|f| *f > 0.0),
        show_centre,
        palette: u16_at(header, HEADER_PALETTE_AT, true),
    })
}

/// The saved picture as greyscale.
fn picture_luma(bytes: &[u8]) -> Option<(usize, usize, Vec<f32>)> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()?.to_luma8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some((w, h, img.into_raw().into_iter().map(f32::from).collect()))
}

/// The orientation under which the temperatures best match the picture's brightness (or its
/// inverse, for black-hot palettes). Only orientations with the picture's aspect are tried.
fn best_orientation(sensor: &[f32], sw: usize, sh: usize, luma: &[f32], lw: usize, lh: usize) -> Orientation {
    use Orientation::*;
    let portrait_picture = lh > lw;
    let portrait_sensor = sh > sw;
    let candidates: &[Orientation] = if portrait_picture == portrait_sensor {
        &[Normal, Rotate180, FlipHorizontal, FlipVertical]
    } else {
        &[Rotate90, Rotate270, Transpose, Transverse]
    };
    let mut best = (candidates[0], f32::MIN);
    for &o in candidates {
        let (w, h) = if o.swaps_axes() { (sh, sw) } else { (sw, sh) };
        // Sample every other pixel; that is plenty to tell orientations apart.
        let (mut n, mut sa, mut sb, mut saa, mut sbb, mut sab) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
        for y in (0..h).step_by(2) {
            for x in (0..w).step_by(2) {
                let (sx, sy) = o.source(x, y, sw, sh);
                let a = sensor[sy * sw + sx] as f64;
                let lx = ((x as f64 + 0.5) * lw as f64 / w as f64) as usize;
                let ly = ((y as f64 + 0.5) * lh as f64 / h as f64) as usize;
                let b = luma[ly.min(lh - 1) * lw + lx.min(lw - 1)] as f64;
                n += 1.0;
                sa += a;
                sb += b;
                saa += a * a;
                sbb += b * b;
                sab += a * b;
            }
        }
        let cov = sab - sa * sb / n;
        let var = (saa - sa * sa / n) * (sbb - sb * sb / n);
        let r = if var > 0.0 { (cov / var.sqrt()).abs() as f32 } else { 0.0 };
        if r > best.1 {
            best = (o, r);
        }
    }
    best.0
}

/// Make, model and date from the EXIF IFD0, if present.
fn exif_ifd0(segs: &[(u8, &[u8])]) -> (Option<String>, Option<String>, Option<String>) {
    let none = (None, None, None);
    let Some((_, d)) = segs.iter().find(|(m, d)| *m == 0xe1 && d.starts_with(b"Exif\0\0")) else {
        return none;
    };
    let t = &d[6..];
    let le = match t.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return none,
    };
    let Some(ifd) = u32_at(t, 4, le).map(|v| v as usize) else { return none };
    let Some(entries) = u16_at(t, ifd, le) else { return none };
    let (mut make, mut model, mut date) = (None, None, None);
    for i in 0..entries as usize {
        let e = ifd + 2 + i * IFD_ENTRY_LEN;
        let (Some(tag), Some(kind), Some(count)) = (u16_at(t, e, le), u16_at(t, e + 2, le), u32_at(t, e + 4, le))
        else {
            break;
        };
        if kind != 2 {
            continue;
        }
        let count = count as usize;
        let at = if count <= 4 { e + 8 } else { u32_at(t, e + 8, le).unwrap_or(0) as usize };
        let value = text(t, at, count);
        match tag {
            TAG_MAKE => make = value,
            TAG_MODEL => model = value,
            TAG_DATE_TIME => date = value,
            _ => {}
        }
    }
    (make, model, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_sources_stay_in_bounds_and_rotate_clockwise() {
        let (w, h) = (4, 3);
        for o in [
            Orientation::Normal,
            Orientation::Rotate180,
            Orientation::FlipHorizontal,
            Orientation::FlipVertical,
            Orientation::Rotate90,
            Orientation::Rotate270,
            Orientation::Transpose,
            Orientation::Transverse,
        ] {
            let (ow, oh) = if o.swaps_axes() { (h, w) } else { (w, h) };
            let mut seen = vec![false; w * h];
            for y in 0..oh {
                for x in 0..ow {
                    let (sx, sy) = o.source(x, y, w, h);
                    assert!(sx < w && sy < h, "{o:?}");
                    seen[sy * w + sx] = true;
                }
            }
            assert!(seen.iter().all(|s| *s), "{o:?} is not a permutation");
        }
        // Clockwise: the sensor's bottom-left corner lands top-left.
        assert_eq!(Orientation::Rotate90.source(0, 0, w, h), (0, h - 1));
    }

    #[test]
    fn finds_the_orientation_that_matches_the_picture() {
        // A sensor image with a hot top-left corner, shown rotated clockwise in a picture
        // twice its size.
        let (sw, sh) = (8, 6);
        let sensor: Vec<f32> =
            (0..sw * sh).map(|i| ((i % sw) * 3 + (i / sw) * 7) as f32 * if i == 0 { 50.0 } else { 1.0 }).collect();
        let (lw, lh) = (sh * 2, sw * 2);
        let mut luma = vec![0f32; lw * lh];
        for y in 0..lh {
            for x in 0..lw {
                let (sx, sy) = Orientation::Rotate90.source(x / 2, y / 2, sw, sh);
                luma[y * lw + x] = 255.0 - sensor[sy * sw + sx]; // black-hot
            }
        }
        assert_eq!(best_orientation(&sensor, sw, sh, &luma, lw, lh), Orientation::Rotate90);
    }

    /// A JPEG with an IJPEG header (APP2) and 16 bytes of APP3 data.
    fn ijpeg_with_sizes(ir_size: u64, temp_size: u64) -> Vec<u8> {
        let mut header = [0u8; HEADER_MIN_LEN];
        header[HEADER_MAGIC_AT..HEADER_MAGIC_AT + 6].copy_from_slice(b"IJPEG\0");
        header[HEADER_IR_SIZE_AT..HEADER_IR_SIZE_AT + 8].copy_from_slice(&ir_size.to_le_bytes());
        header[HEADER_TEMP_SIZE_AT..HEADER_TEMP_SIZE_AT + 8].copy_from_slice(&temp_size.to_le_bytes());
        header[HEADER_TEMP_WIDTH_AT..HEADER_TEMP_WIDTH_AT + 2].copy_from_slice(&4u16.to_le_bytes());
        let mut out = vec![0xff, 0xd8];
        for (marker, payload) in [(0xe2u8, &header[..]), (0xe3, &[0u8; 16][..])] {
            out.extend([0xff, marker]);
            out.extend((payload.len() as u16 + 2).to_be_bytes());
            out.extend(payload);
        }
        out.extend([0xff, 0xd9, 0, 0]);
        out
    }

    #[test]
    fn rejects_overflowing_sizes() {
        // 1 + usize::MAX overflows; decode must return None instead of panicking.
        assert!(is_ijpeg(&ijpeg_with_sizes(1, usize::MAX as u64)));
        assert!(decode(&ijpeg_with_sizes(1, usize::MAX as u64)).is_none());
        // Sizes that fit in `usize` but exceed the data are rejected too.
        assert!(decode(&ijpeg_with_sizes(8, 64)).is_none());
    }

    #[test]
    fn rejects_other_files() {
        assert!(!is_ijpeg(b"not a jpeg"));
        assert!(decode(&[0xff, 0xd8, 0xff, 0xd9]).is_none());
    }
}

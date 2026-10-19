//! Exports: temperature tables (CSV) and encoded images.

use std::fmt::Write as _;
use std::io::Cursor;

use image::{ExtendedColorType, ImageEncoder};

use crate::{Error, Result, Unit};

/// The temperature matrix as CSV: one row per image row, no header, two decimals in `unit`.
/// Pixels without a temperature are empty cells.
pub fn csv(kelvin: &[f32], width: usize, unit: Unit) -> String {
    let mut out = String::with_capacity(kelvin.len() * 7);
    for row in kelvin.chunks(width.max(1)) {
        for (i, &k) in row.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if k.is_finite() {
                let _ = write!(out, "{:.2}", unit.from_kelvin(k));
            }
        }
        out.push('\n');
    }
    out
}

pub fn png(rgb: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgb, width as u32, height as u32, ExtendedColorType::Rgb8)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

pub fn jpeg(rgb: &[u8], width: usize, height: usize, quality: u8) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(rgb, width as u32, height as u32, ExtendedColorType::Rgb8)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_one_line_per_row_and_blank_nan() {
        let csv = csv(&[273.15, 283.15, f32::NAN, 293.15], 2, Unit::Celsius);
        assert_eq!(csv, "0.00,10.00\n,20.00\n");
    }

    #[test]
    fn png_and_jpeg_round_trip_dimensions() {
        let rgb = vec![200u8; 4 * 3 * 3];
        let png = png(&rgb, 4, 3).unwrap();
        let jpg = jpeg(&rgb, 4, 3, 90).unwrap();
        for bytes in [png, jpg] {
            let img = image::load_from_memory(&bytes).unwrap();
            assert_eq!((img.width(), img.height()), (4, 3));
        }
    }
}

//! Colour palettes and rendering temperatures to RGB.
//!
//! The UI renders with the same palettes and the same mapping (`index`), so on-screen images,
//! exported PNGs and report images agree pixel for pixel.

use libblackbody::palettes;
use serde::Serialize;

/// 256 colours, index 0 for the coldest end of the range.
#[derive(Clone, Debug, Serialize)]
pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: Vec<[u8; 3]>,
}

/// Colour for pixels without a temperature (NaN).
pub const NO_DATA: [u8; 3] = [128, 128, 128];

pub fn all() -> Vec<Palette> {
    vec![
        from_stops("iron", "Iron", IRON),
        from_stops("rainbow", "Rainbow", RAINBOW),
        from_stops("white-hot", "White hot", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])]),
        from_stops("black-hot", "Black hot", &[(0.0, [255, 255, 255]), (1.0, [0, 0, 0])]),
        from_table("inferno", "Inferno", &palettes::INFERNO),
        from_table("turbo", "Turbo", &palettes::TURBO),
        from_table("viridis", "Viridis", &palettes::VIRIDIS),
        from_table("magma", "Magma", &palettes::MAGMA),
    ]
}

pub fn by_id(id: &str) -> Option<Palette> {
    all().into_iter().find(|p| p.id == id)
}

const IRON: &[(f32, [u8; 3])] = &[
    (0.00, [0, 0, 10]),
    (0.10, [32, 0, 96]),
    (0.25, [110, 0, 150]),
    (0.40, [180, 20, 130]),
    (0.55, [225, 70, 50]),
    (0.70, [245, 130, 0]),
    (0.85, [255, 200, 30]),
    (1.00, [255, 255, 230]),
];

const RAINBOW: &[(f32, [u8; 3])] = &[
    (0.00, [10, 0, 40]),
    (0.15, [60, 0, 160]),
    (0.30, [0, 60, 255]),
    (0.45, [0, 200, 220]),
    (0.60, [40, 220, 40]),
    (0.75, [250, 230, 0]),
    (0.90, [255, 60, 0]),
    (1.00, [255, 255, 255]),
];

fn from_stops(id: &'static str, name: &'static str, stops: &[(f32, [u8; 3])]) -> Palette {
    let colors = (0..256)
        .map(|i| {
            let t = i as f32 / 255.0;
            let upper = stops.iter().position(|(at, _)| *at >= t).unwrap_or(stops.len() - 1).max(1);
            let ((a, ca), (b, cb)) = (stops[upper - 1], stops[upper]);
            let f = if b > a { ((t - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
            [0, 1, 2].map(|c| (ca[c] as f32 + (cb[c] as f32 - ca[c] as f32) * f).round() as u8)
        })
        .collect();
    Palette { id, name, colors }
}

fn from_table(id: &'static str, name: &'static str, table: &[[f32; 3]; 256]) -> Palette {
    let colors = table.iter().map(|c| c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)).collect();
    Palette { id, name, colors }
}

/// Palette index for `kelvin` within `range`; `None` for NaN.
pub fn index(kelvin: f32, range: [f32; 2]) -> Option<usize> {
    if kelvin.is_nan() {
        return None;
    }
    let span = range[1] - range[0];
    if span <= 0.0 {
        return Some(if kelvin < range[0] { 0 } else { 255 });
    }
    let t = (kelvin - range[0]) / span;
    Some(((t * 256.0).floor()).clamp(0.0, 255.0) as usize)
}

/// Renders temperatures to RGB bytes (3 per pixel) with `palette` over `range` (kelvin).
pub fn render(kelvin: &[f32], range: [f32; 2], palette: &Palette) -> Vec<u8> {
    kelvin.iter().flat_map(|&k| index(k, range).map(|i| palette.colors[i]).unwrap_or(NO_DATA)).collect()
}

/// A horizontal colour bar, `width` × 1 pixels, coldest on the left.
pub fn bar(palette: &Palette, width: usize) -> Vec<u8> {
    (0..width).flat_map(|i| palette.colors[i * 256 / width.max(1)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_has_256_colours_and_unique_id() {
        let all = all();
        for p in &all {
            assert_eq!(p.colors.len(), 256, "{}", p.id);
            assert_eq!(all.iter().filter(|q| q.id == p.id).count(), 1);
        }
    }

    #[test]
    fn stops_are_hit_exactly_at_the_ends() {
        let iron = by_id("iron").unwrap();
        assert_eq!(iron.colors[0], [0, 0, 10]);
        assert_eq!(iron.colors[255], [255, 255, 230]);
        let white = by_id("white-hot").unwrap();
        assert_eq!((white.colors[0], white.colors[255]), ([0, 0, 0], [255, 255, 255]));
    }

    #[test]
    fn index_clips_and_maps_range_ends() {
        let r = [280.0, 300.0];
        assert_eq!(index(270.0, r), Some(0));
        assert_eq!(index(280.0, r), Some(0));
        assert_eq!(index(299.99, r), Some(255));
        assert_eq!(index(300.0, r), Some(255));
        assert_eq!(index(290.0, r), Some(128));
        assert_eq!(index(f32::NAN, r), None);
        assert_eq!(index(290.0, [290.0, 290.0]), Some(255));
    }

    #[test]
    fn render_uses_no_data_colour_for_nan() {
        let p = by_id("white-hot").unwrap();
        let rgb = render(&[280.0, f32::NAN, 300.0], [280.0, 300.0], &p);
        assert_eq!(rgb, vec![0, 0, 0, 128, 128, 128, 255, 255, 255]);
    }
}

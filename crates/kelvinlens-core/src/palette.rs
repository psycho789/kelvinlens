//! Colour palettes. Every palette has 256 colours, index 0 for the coldest end of the range.
//!
//! The built-in palettes are defined here; custom palettes and the camera's own palette arrive
//! as colour tables (see `resample`). Mapping temperatures to palette indices lives in
//! `render.rs`, so on-screen images, exports and reports agree pixel for pixel.

use std::sync::OnceLock;

use libblackbody::palettes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub colors: Vec<[u8; 3]>,
}

/// Colour for pixels without a temperature (NaN).
pub const NO_DATA: [u8; 3] = [128, 128, 128];

static ALL: OnceLock<Vec<Palette>> = OnceLock::new();

/// The built-in palettes, built on first use.
pub fn all() -> &'static [Palette] {
    ALL.get_or_init(build_all)
}

fn build_all() -> Vec<Palette> {
    vec![
        from_stops("iron", "Iron", IRON),
        from_stops("rainbow", "Rainbow", RAINBOW),
        from_stops("white-hot", "White hot", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])]),
        from_stops("black-hot", "Black hot", &[(0.0, [255, 255, 255]), (1.0, [0, 0, 0])]),
        from_stops("arctic", "Arctic", ARCTIC),
        from_stops("lava", "Lava", LAVA),
        from_table("inferno", "Inferno", &palettes::INFERNO),
        from_table("turbo", "Turbo", &palettes::TURBO),
        from_table("viridis", "Viridis", &palettes::VIRIDIS),
        from_table("magma", "Magma", &palettes::MAGMA),
    ]
}

pub fn by_id(id: &str) -> Option<Palette> {
    all().iter().find(|p| p.id == id).cloned()
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

/// Cold blues through white to warm golds, for scenes near ambient.
const ARCTIC: &[(f32, [u8; 3])] = &[
    (0.00, [8, 16, 64]),
    (0.30, [20, 90, 200]),
    (0.50, [150, 210, 245]),
    (0.55, [250, 245, 235]),
    (0.75, [245, 170, 40]),
    (1.00, [255, 240, 120]),
];

/// Black through deep red and orange to white, for hot equipment.
const LAVA: &[(f32, [u8; 3])] = &[
    (0.00, [0, 0, 0]),
    (0.30, [90, 10, 30]),
    (0.55, [200, 30, 20]),
    (0.75, [250, 120, 10]),
    (0.90, [255, 210, 80]),
    (1.00, [255, 255, 255]),
];

/// A palette from colour stops at positions 0–1 (sorted), interpolated linearly in RGB.
pub fn from_stops(id: &str, name: &str, stops: &[(f32, [u8; 3])]) -> Palette {
    Palette { id: id.into(), name: name.into(), colors: stops_to_colors(stops) }
}

pub fn stops_to_colors(stops: &[(f32, [u8; 3])]) -> Vec<[u8; 3]> {
    match stops {
        [] => return vec![NO_DATA; 256],
        [(_, c)] => return vec![*c; 256],
        _ => {}
    }
    (0..256)
        .map(|i| {
            let t = i as f32 / 255.0;
            let upper = stops.iter().position(|(at, _)| *at >= t).unwrap_or(stops.len() - 1).max(1);
            let ((a, ca), (b, cb)) = (stops[upper - 1], stops[upper]);
            let f = if b > a { ((t - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
            [0, 1, 2].map(|c| (ca[c] as f32 + (cb[c] as f32 - ca[c] as f32) * f).round() as u8)
        })
        .collect()
}

fn from_table(id: &str, name: &str, table: &[[f32; 3]; 256]) -> Palette {
    let colors = table.iter().map(|c| c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)).collect();
    Palette { id: id.into(), name: name.into(), colors }
}

/// Stretches or shrinks any colour table to 256 entries by linear interpolation.
pub fn resample(colors: &[[u8; 3]]) -> Vec<[u8; 3]> {
    if colors.len() == 256 || colors.len() < 2 {
        return if colors.is_empty() {
            vec![NO_DATA; 256]
        } else if colors.len() == 1 {
            vec![colors[0]; 256]
        } else {
            colors.to_vec()
        };
    }
    let n = colors.len() - 1;
    (0..256)
        .map(|i| {
            let pos = i as f32 / 255.0 * n as f32;
            let lo = (pos.floor() as usize).min(n);
            let hi = (lo + 1).min(n);
            let f = pos - lo as f32;
            [0, 1, 2].map(|c| (colors[lo][c] as f32 + (colors[hi][c] as f32 - colors[lo][c] as f32) * f).round() as u8)
        })
        .collect()
}

impl Palette {
    /// The same palette with exactly 256 colours.
    pub fn normalized(mut self) -> Palette {
        self.colors = resample(&self.colors);
        self
    }
}

/// A horizontal colour bar, `width` × 1 pixels, coldest on the left.
pub fn bar(palette: &Palette, width: usize) -> Vec<u8> {
    let n = palette.colors.len().max(1);
    (0..width).flat_map(|i| palette.colors[(i * n / width.max(1)).min(n - 1)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_has_256_colours_and_unique_id() {
        let all = all();
        for p in all {
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
    fn resample_keeps_ends_and_interpolates() {
        let r = resample(&[[0, 0, 0], [255, 255, 255]]);
        assert_eq!((r.len(), r[0], r[255]), (256, [0, 0, 0], [255, 255, 255]));
        assert_eq!(r[128], [128, 128, 128]);
        let short = resample(&(0..224u32).map(|i| [i as u8, 0, 0]).collect::<Vec<_>>());
        assert_eq!((short.len(), short[0][0], short[255][0]), (256, 0, 223));
        assert_eq!(resample(&[]).len(), 256);
    }
}

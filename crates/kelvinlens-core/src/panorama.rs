//! Panoramas: stitching overlapping thermal images taken side by side into one radiometric image.
//!
//! Images are placed by translation only (the camera turned or moved between shots, with the
//! same lens and orientation), which is how handheld panoramas are taken. Each image is aligned
//! to the previous one by normalised cross-correlation of temperatures, searched coarse to fine.
//! Overlaps are blended with weights that fall off towards each image's edges, so seams don't show.

use serde::Serialize;

/// Largest stitched canvas, in pixels (width × height); larger results are refused.
const MAX_CANVAS_PIXELS: usize = 40_000_000;

/// Smallest overlap between neighbours, as a share of the smaller image's area.
const MIN_OVERLAP: f32 = 0.15;

/// A stitched image: temperatures (NaN where no image covers) and each image's top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stitched {
    pub width: usize,
    pub height: usize,
    #[serde(skip)]
    pub kelvin: Vec<f32>,
    pub offsets: Vec<[i64; 2]>,
    /// Correlation score (−1..1) of each image with the previous one; 1.0 for the first.
    pub scores: Vec<f32>,
}

/// One input image (temperatures in kelvin).
pub struct Tile<'a> {
    pub kelvin: &'a [f32],
    pub width: usize,
    pub height: usize,
}

/// Both images of a pair at one pyramid scale.
struct Level {
    a: Vec<f32>,
    aw: usize,
    ah: usize,
    b: Vec<f32>,
    bw: usize,
    bh: usize,
}

impl Level {
    fn new(a: &Tile, b: &Tile) -> Level {
        Level { a: a.kelvin.to_vec(), aw: a.width, ah: a.height, b: b.kelvin.to_vec(), bw: b.width, bh: b.height }
    }

    /// The next level down: both images halved by 2×2 averaging.
    fn halved(&self) -> Level {
        let (a, aw, ah) = half(&self.a, self.aw, self.ah);
        let (b, bw, bh) = half(&self.b, self.bw, self.bh);
        Level { a, aw, ah, b, bw, bh }
    }

    /// Smallest overlap, in pixels at this scale, that `ncc` accepts.
    fn min_area(&self) -> f32 {
        MIN_OVERLAP * (self.aw * self.ah).min(self.bw * self.bh) as f32
    }
}

/// Offset of `b` relative to `a` (b's top-left in a's pixels) that best matches the overlap, and
/// its correlation score.
pub fn align(a: &Tile, b: &Tile) -> Option<([i64; 2], f32)> {
    // Image pyramids, halving until the smaller side is below ~40 pixels.
    let mut levels = vec![Level::new(a, b)];
    while levels.last().is_some_and(|l| l.aw.min(l.ah).min(l.bw).min(l.bh) > 40) {
        let next = levels.last().unwrap().halved();
        levels.push(next);
    }

    // Exhaustive search at the coarsest level. Smooth thermal scenes correlate well almost
    // anywhere at low resolution, so keep the best few distinct candidates and let full
    // resolution decide between them.
    let coarsest = levels.last().unwrap();
    let mut coarse: Vec<([i64; 2], f32)> = Vec::new();
    for dy in -(coarsest.bh as i64) + 1..coarsest.ah as i64 {
        for dx in -(coarsest.bw as i64) + 1..coarsest.aw as i64 {
            if let Some(s) = ncc(coarsest, dx, dy) {
                coarse.push(([dx, dy], s));
            }
        }
    }
    coarse.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut candidates: Vec<[i64; 2]> = Vec::new();
    for (o, _) in coarse {
        if candidates.iter().all(|c| (c[0] - o[0]).abs() > 2 || (c[1] - o[1]).abs() > 2) {
            candidates.push(o);
            if candidates.len() == 12 {
                break;
            }
        }
    }

    // Refine each candidate ±2 pixels per finer level; keep the best at full resolution, and
    // among near-equal scores the larger overlap.
    let mut best: Option<([i64; 2], f32, i64)> = None;
    for start in candidates {
        let mut offset = start;
        let mut score = f32::MIN;
        for level in levels.iter().rev().skip(1) {
            let centre = [offset[0] * 2, offset[1] * 2];
            let mut refined: Option<([i64; 2], f32)> = None;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let o = [centre[0] + dx, centre[1] + dy];
                    if let Some(s) = ncc(level, o[0], o[1])
                        && refined.is_none_or(|(_, rs)| s > rs)
                    {
                        refined = Some((o, s));
                    }
                }
            }
            match refined {
                Some((o, s)) => (offset, score) = (o, s),
                None => {
                    offset = centre;
                    score = f32::MIN;
                }
            }
        }
        if levels.len() == 1 {
            score = ncc(&levels[0], offset[0], offset[1]).unwrap_or(f32::MIN);
        }
        if score == f32::MIN {
            continue;
        }
        let overlap = ((offset[0] + b.width as i64).min(a.width as i64) - offset[0].max(0))
            * ((offset[1] + b.height as i64).min(a.height as i64) - offset[1].max(0));
        let better = match best {
            None => true,
            Some((_, bs, bo)) => score > bs + 0.002 || (score > bs - 0.002 && overlap > bo),
        };
        if better {
            best = Some((offset, score, overlap));
        }
    }
    best.map(|(o, s, _)| (o, s))
}

/// Normalised cross-correlation of `b` placed at (dx, dy) over `a`, on their finite overlap.
fn ncc(level: &Level, dx: i64, dy: i64) -> Option<f32> {
    let Level { a, aw, ah, b, bw, bh } = level;
    let (aw, ah, bw, bh) = (*aw, *ah, *bw, *bh);
    let x0 = dx.max(0);
    let y0 = dy.max(0);
    let x1 = (dx + bw as i64).min(aw as i64);
    let y1 = (dy + bh as i64).min(ah as i64);
    if x1 <= x0 || y1 <= y0 || (((x1 - x0) * (y1 - y0)) as f32) < level.min_area() {
        return None;
    }
    // Sample on a grid so large overlaps cost about the same as small ones.
    let step = ((((x1 - x0) * (y1 - y0)) as f64 / 4096.0).sqrt().floor() as i64).max(1);
    let (mut n, mut sa, mut sb, mut saa, mut sbb, mut sab) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let va = a[y as usize * aw + x as usize] as f64;
            let vb = b[(y - dy) as usize * bw + (x - dx) as usize] as f64;
            if va.is_finite() && vb.is_finite() {
                n += 1.0;
                sa += va;
                sb += vb;
                saa += va * va;
                sbb += vb * vb;
                sab += va * vb;
            }
            x += step;
        }
        y += step;
    }
    if n < 16.0 {
        return None;
    }
    let cov = sab - sa * sb / n;
    let va = saa - sa * sa / n;
    let vb = sbb - sb * sb / n;
    if va <= 1e-9 || vb <= 1e-9 {
        return None;
    }
    Some((cov / (va * vb).sqrt()) as f32)
}

/// Half-size image by 2×2 averaging (NaN skipped).
fn half(k: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let (w2, h2) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![f32::NAN; w2 * h2];
    for y in 0..h2 {
        for x in 0..w2 {
            let (mut s, mut n) = (0f32, 0);
            for (sx, sy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
                if sx < w && sy < h && k[sy * w + sx].is_finite() {
                    s += k[sy * w + sx];
                    n += 1;
                }
            }
            if n > 0 {
                out[y * w2 + x] = s / n as f32;
            }
        }
    }
    (out, w2, h2)
}

/// Stitches `tiles` in order. `offsets` gives fixed positions (top-left, pixels, relative to the
/// first tile) for tiles the user placed; `None` entries are found automatically from the tile
/// before. Errors when an automatic tile has no usable overlap with its predecessor.
pub fn stitch(tiles: &[Tile], offsets: &[Option<[i64; 2]>]) -> Result<Stitched, String> {
    if tiles.len() < 2 {
        return Err("A panorama needs at least two images".into());
    }
    let mut placed: Vec<[i64; 2]> = vec![[0, 0]];
    let mut scores = vec![1.0];
    for i in 1..tiles.len() {
        match offsets.get(i).copied().flatten() {
            Some(o) => {
                placed.push(o);
                scores.push(f32::NAN);
            }
            None => {
                let (rel, score) = align(&tiles[i - 1], &tiles[i])
                    .ok_or_else(|| format!("Image {} does not overlap image {} enough to align", i + 1, i))?;
                let prev = placed[i - 1];
                placed.push([prev[0] + rel[0], prev[1] + rel[1]]);
                scores.push(score);
            }
        }
    }
    let min_x = placed.iter().map(|o| o[0]).min().unwrap();
    let min_y = placed.iter().map(|o| o[1]).min().unwrap();
    let offsets: Vec<[i64; 2]> = placed.iter().map(|o| [o[0] - min_x, o[1] - min_y]).collect();
    let width = tiles.iter().zip(&offsets).map(|(t, o)| o[0] as usize + t.width).max().unwrap();
    let height = tiles.iter().zip(&offsets).map(|(t, o)| o[1] as usize + t.height).max().unwrap();
    if width.saturating_mul(height) > MAX_CANVAS_PIXELS {
        return Err("The panorama would be too large; check the image order and alignment".into());
    }

    // Feathered blend: weight = distance to the tile's nearest edge (+1).
    let mut sum = vec![0f64; width * height];
    let mut wsum = vec![0f64; width * height];
    for (t, o) in tiles.iter().zip(&offsets) {
        for y in 0..t.height {
            for x in 0..t.width {
                let v = t.kelvin[y * t.width + x];
                if !v.is_finite() {
                    continue;
                }
                let edge = x.min(y).min(t.width - 1 - x).min(t.height - 1 - y) as f64 + 1.0;
                let i = (o[1] as usize + y) * width + o[0] as usize + x;
                sum[i] += v as f64 * edge;
                wsum[i] += edge;
            }
        }
    }
    let kelvin = sum.iter().zip(&wsum).map(|(s, w)| if *w > 0.0 { (s / w) as f32 } else { f32::NAN }).collect();
    Ok(Stitched { width, height, kelvin, offsets, scores })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth but non-repeating 200×120 scene.
    fn scene() -> (Vec<f32>, usize, usize) {
        let (w, h) = (200, 120);
        let k = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                290.0 + 5.0 * (x / 17.0).sin() * (y / 23.0).cos() + 3.0 * ((x + 2.0 * y) / 31.0).sin() + x * 0.02
            })
            .collect();
        (k, w, h)
    }

    fn crop(k: &[f32], w: usize, x0: usize, y0: usize, cw: usize, ch: usize) -> Vec<f32> {
        (0..ch).flat_map(|y| (0..cw).map(move |x| k[(y0 + y) * w + x0 + x])).collect()
    }

    #[test]
    fn recovers_offsets_of_overlapping_crops_exactly() {
        let (k, w, _) = scene();
        let a = crop(&k, w, 0, 10, 90, 80);
        let b = crop(&k, w, 55, 18, 90, 80);
        let c = crop(&k, w, 107, 5, 90, 80);
        let tiles = [
            Tile { kelvin: &a, width: 90, height: 80 },
            Tile { kelvin: &b, width: 90, height: 80 },
            Tile { kelvin: &c, width: 90, height: 80 },
        ];
        let s = stitch(&tiles, &[]).unwrap();
        // Relative to the topmost tile (c at y 5): a at (0, 5), b at (55, 13), c at (107, 0).
        assert_eq!(s.offsets, vec![[0, 5], [55, 13], [107, 0]]);
        assert_eq!((s.width, s.height), (197, 93));
        assert!(s.scores[1] > 0.99 && s.scores[2] > 0.99, "{:?}", s.scores);
        // Every covered pixel equals the scene: blending identical values changes nothing.
        let mut checked = 0;
        for y in 0..s.height {
            for x in 0..s.width {
                let v = s.kelvin[y * s.width + x];
                if v.is_finite() {
                    let truth = k[(y + 5) * w + x];
                    assert!((v - truth).abs() < 1e-3, "({x},{y}) {v} vs {truth}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 12_000);
        // Corners no tile covers are NaN.
        assert!(s.kelvin[0].is_nan() == (s.offsets.iter().all(|o| o != &[0, 0])));
    }

    #[test]
    fn manual_offsets_override_alignment() {
        let (k, w, _) = scene();
        let a = crop(&k, w, 0, 0, 60, 60);
        let b = crop(&k, w, 40, 0, 60, 60);
        let tiles = [Tile { kelvin: &a, width: 60, height: 60 }, Tile { kelvin: &b, width: 60, height: 60 }];
        let s = stitch(&tiles, &[None, Some([45, 3])]).unwrap();
        assert_eq!(s.offsets, vec![[0, 0], [45, 3]]);
        assert!(s.scores[1].is_nan());
    }

    #[test]
    fn refuses_a_canvas_over_the_pixel_limit() {
        let flat = vec![290.0f32; 50 * 50];
        let tiles = [Tile { kelvin: &flat, width: 50, height: 50 }, Tile { kelvin: &flat, width: 50, height: 50 }];
        // Offset (100_000, 100_000) makes a 100_050 × 100_050 canvas, far above MAX_CANVAS_PIXELS.
        let err = stitch(&tiles, &[None, Some([100_000, 100_000])]).unwrap_err();
        assert!(err.contains("too large"), "{err}");
    }

    #[test]
    fn refuses_images_without_overlap() {
        let flat = vec![290.0f32; 50 * 50];
        let tiles = [Tile { kelvin: &flat, width: 50, height: 50 }, Tile { kelvin: &flat, width: 50, height: 50 }];
        assert!(stitch(&tiles, &[]).unwrap_err().contains("does not overlap"));
        assert!(stitch(&tiles[..1], &[]).is_err());
    }
}

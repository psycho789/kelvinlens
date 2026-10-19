//! Measurement tools and their temperature statistics.
//!
//! Coordinates are continuous thermal-image pixels: pixel `(i, j)` covers `i..i+1` by
//! `j..j+1`, so its centre is `(i + 0.5, j + 0.5)`. An area tool includes every pixel whose
//! centre lies inside it.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    Spot { x: f32, y: f32 },
    Box { x: f32, y: f32, w: f32, h: f32 },
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub label: String,
    pub shape: Shape,
}

/// Statistics over a tool's pixels, in kelvin. NaN pixels are skipped; `count` is the number used.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
    pub min_at: [usize; 2],
    pub max_at: [usize; 2],
    pub count: usize,
}

/// The pixels a shape covers, clipped to the image.
pub fn pixels(shape: &Shape, width: usize, height: usize) -> Vec<[usize; 2]> {
    let inside = |x: f32, y: f32| x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height;
    let cell = |x: f32, y: f32| [x as usize, y as usize];

    match *shape {
        Shape::Spot { x, y } => {
            if inside(x, y) {
                vec![cell(x, y)]
            } else {
                Vec::new()
            }
        }
        Shape::Box { x, y, w, h } => {
            let (x0, x1) = (x.min(x + w), x.max(x + w));
            let (y0, y1) = (y.min(y + h), y.max(y + h));
            area(width, height, [x0, y0, x1, y1], |px, py| px >= x0 && px <= x1 && py >= y0 && py <= y1)
                .unwrap_or_else(|| centre(width, height, (x0 + x1) / 2.0, (y0 + y1) / 2.0))
        }
        Shape::Ellipse { cx, cy, rx, ry } => {
            let (rx, ry) = (rx.abs(), ry.abs());
            let bounds = [cx - rx, cy - ry, cx + rx, cy + ry];
            area(width, height, bounds, |px, py| {
                if rx == 0.0 || ry == 0.0 {
                    return false;
                }
                let (dx, dy) = ((px - cx) / rx, (py - cy) / ry);
                dx * dx + dy * dy <= 1.0
            })
            .unwrap_or_else(|| centre(width, height, cx, cy))
        }
        Shape::Line { x1, y1, x2, y2 } => {
            // Sample twice per pixel along the longer axis so no pixel the line crosses is skipped.
            let steps = ((x2 - x1).abs().max((y2 - y1).abs()) * 2.0).ceil().max(1.0) as usize;
            let mut out: Vec<[usize; 2]> = Vec::new();
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let (x, y) = (x1 + (x2 - x1) * t, y1 + (y2 - y1) * t);
                if inside(x, y) && !out.contains(&cell(x, y)) {
                    out.push(cell(x, y));
                }
            }
            out
        }
    }
}

/// Pixels in `bounds` (x0, y0, x1, y1) whose centre passes `test`; `None` if there are none.
fn area(width: usize, height: usize, bounds: [f32; 4], test: impl Fn(f32, f32) -> bool) -> Option<Vec<[usize; 2]>> {
    let clamp = |v: f32, max: usize| (v.max(0.0) as usize).min(max);
    let (x0, y0) = (clamp(bounds[0].floor(), width), clamp(bounds[1].floor(), height));
    let (x1, y1) = (clamp(bounds[2].ceil(), width), clamp(bounds[3].ceil(), height));
    let out: Vec<[usize; 2]> = (y0..y1)
        .flat_map(|py| (x0..x1).map(move |px| [px, py]))
        .filter(|&[px, py]| test(px as f32 + 0.5, py as f32 + 0.5))
        .collect();
    (!out.is_empty()).then_some(out)
}

/// A tool smaller than a pixel measures the pixel under its centre.
fn centre(width: usize, height: usize, x: f32, y: f32) -> Vec<[usize; 2]> {
    if x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height {
        vec![[x as usize, y as usize]]
    } else {
        Vec::new()
    }
}

/// Statistics over the shape's pixels, or `None` if it covers no finite temperature.
pub fn stats(kelvin: &[f32], width: usize, height: usize, shape: &Shape) -> Option<Stats> {
    let mut acc: Option<Stats> = None;
    let mut sum = 0.0f64;
    for [x, y] in pixels(shape, width, height) {
        let v = kelvin[y * width + x];
        if !v.is_finite() {
            continue;
        }
        sum += v as f64;
        let s = acc.get_or_insert(Stats { min: v, max: v, avg: v, min_at: [x, y], max_at: [x, y], count: 0 });
        if v < s.min {
            s.min = v;
            s.min_at = [x, y];
        }
        if v > s.max {
            s.max = v;
            s.max_at = [x, y];
        }
        s.count += 1;
    }
    acc.map(|mut s| {
        s.avg = (sum / s.count as f64) as f32;
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×3 image whose value is 10·y + x.
    fn image() -> Vec<f32> {
        (0..3).flat_map(|y| (0..4).map(move |x| (10 * y + x) as f32)).collect()
    }

    #[test]
    fn spot_reads_the_pixel_under_it() {
        let s = stats(&image(), 4, 3, &Shape::Spot { x: 2.5, y: 1.5 }).unwrap();
        assert_eq!((s.min, s.max, s.avg, s.count), (12.0, 12.0, 12.0, 1));
        assert!(stats(&image(), 4, 3, &Shape::Spot { x: 4.5, y: 0.5 }).is_none());
        assert!(stats(&image(), 4, 3, &Shape::Spot { x: -0.5, y: 0.5 }).is_none());
    }

    #[test]
    fn box_covers_pixel_centres_and_reports_locations() {
        let s = stats(&image(), 4, 3, &Shape::Box { x: 1.0, y: 0.0, w: 2.0, h: 2.0 }).unwrap();
        // Pixels x 1..=2, y 0..=1: 1, 2, 11, 12.
        assert_eq!((s.min, s.max, s.count), (1.0, 12.0, 4));
        assert_eq!((s.min_at, s.max_at), ([1, 0], [2, 1]));
        assert!((s.avg - 6.5).abs() < 1e-6);
    }

    #[test]
    fn box_drawn_backwards_is_the_same_box() {
        let a = stats(&image(), 4, 3, &Shape::Box { x: 1.0, y: 0.0, w: 2.0, h: 2.0 });
        let b = stats(&image(), 4, 3, &Shape::Box { x: 3.0, y: 2.0, w: -2.0, h: -2.0 });
        assert_eq!(a, b);
    }

    #[test]
    fn tiny_box_measures_its_centre_pixel() {
        let s = stats(&image(), 4, 3, &Shape::Box { x: 2.2, y: 1.2, w: 0.2, h: 0.2 }).unwrap();
        assert_eq!((s.min, s.count), (12.0, 1));
    }

    #[test]
    fn ellipse_includes_centres_inside() {
        // Circle of radius 1 around the centre of pixel (1, 1) covers it and its 4 neighbours.
        let px = pixels(&Shape::Ellipse { cx: 1.5, cy: 1.5, rx: 1.0, ry: 1.0 }, 4, 3);
        let mut px = px.clone();
        px.sort();
        assert_eq!(px, vec![[0, 1], [1, 0], [1, 1], [1, 2], [2, 1]]);
    }

    #[test]
    fn line_visits_each_crossed_pixel_once() {
        let px = pixels(&Shape::Line { x1: 0.5, y1: 0.5, x2: 3.5, y2: 0.5 }, 4, 3);
        assert_eq!(px, vec![[0, 0], [1, 0], [2, 0], [3, 0]]);
        let diag = pixels(&Shape::Line { x1: 0.5, y1: 0.5, x2: 2.5, y2: 2.5 }, 4, 3);
        assert_eq!(diag.first(), Some(&[0, 0]));
        assert_eq!(diag.last(), Some(&[2, 2]));
    }

    #[test]
    fn nan_pixels_are_skipped() {
        let mut img = image();
        img[1] = f32::NAN;
        let s = stats(&img, 4, 3, &Shape::Box { x: 0.0, y: 0.0, w: 2.0, h: 1.0 }).unwrap();
        assert_eq!((s.min, s.max, s.count), (0.0, 0.0, 1));
        img[0] = f32::NAN;
        assert!(stats(&img, 4, 3, &Shape::Box { x: 0.0, y: 0.0, w: 2.0, h: 1.0 }).is_none());
    }

    #[test]
    fn shapes_serialize_with_kind_tag() {
        let m = Measurement { label: "Bx1".into(), shape: Shape::Box { x: 1.0, y: 2.0, w: 3.0, h: 4.0 } };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"label":"Bx1","shape":{"kind":"box","x":1.0,"y":2.0,"w":3.0,"h":4.0}}"#);
        assert_eq!(serde_json::from_str::<Measurement>(&json).unwrap(), m);
    }
}

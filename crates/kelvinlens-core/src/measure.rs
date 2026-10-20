//! Measurement tools, their temperature statistics, profiles and alarms.
//!
//! Coordinates are continuous thermal-image pixels: pixel `(i, j)` covers `i..i+1` by
//! `j..j+1`, so its centre is `(i + 0.5, j + 0.5)`. An area tool includes every pixel whose
//! centre lies inside it. The magic wand selects the pixels connected to its seed whose
//! temperature is within a tolerance of the seed's, so its area follows the temperatures.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    Spot {
        x: f32,
        y: f32,
    },
    Box {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    /// A closed area through at least three points.
    Polygon {
        points: Vec<[f32; 2]>,
    },
    /// An open path through at least two points.
    Polyline {
        points: Vec<[f32; 2]>,
    },
    /// Pixels connected to the seed `(x, y)` within `tolerance` kelvin of the seed's temperature.
    Wand {
        x: f32,
        y: f32,
        tolerance: f32,
    },
}

impl Shape {
    /// Area tools measure a region; the others measure a point or a path.
    pub fn is_area(&self) -> bool {
        matches!(self, Shape::Box { .. } | Shape::Ellipse { .. } | Shape::Polygon { .. } | Shape::Wand { .. })
    }

    /// Tools along which a temperature profile can be drawn.
    pub fn is_path(&self) -> bool {
        matches!(self, Shape::Line { .. } | Shape::Polyline { .. })
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Shape::Spot { .. } => "Spot",
            Shape::Box { .. } => "Box",
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Line { .. } => "Line",
            Shape::Polygon { .. } => "Polygon",
            Shape::Polyline { .. } => "Polyline",
            Shape::Wand { .. } => "Magic wand",
        }
    }
}

/// Which statistic an alarm or a formula reference reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stat {
    Min,
    #[default]
    Max,
    Avg,
}

impl Stat {
    pub fn of(self, s: &Stats) -> f32 {
        match self {
            Stat::Min => s.min,
            Stat::Max => s.max,
            Stat::Avg => s.avg,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stat::Min => "min",
            Stat::Max => "max",
            Stat::Avg => "avg",
        }
    }
}

/// Raises an alarm when the chosen statistic is above `above` or below `below` (kelvin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Alarm {
    pub stat: Stat,
    pub above: Option<f32>,
    pub below: Option<f32>,
}

impl Alarm {
    /// `Some(true)` when triggered, `Some(false)` when armed and clear, `None` when no limit is set
    /// or there is no temperature to test.
    pub fn check(&self, stats: Option<&Stats>) -> Option<bool> {
        if self.above.is_none() && self.below.is_none() {
            return None;
        }
        let v = self.stat.of(stats?);
        Some(self.above.is_some_and(|a| v > a) || self.below.is_some_and(|b| v < b))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub label: String,
    pub shape: Shape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alarm: Option<Alarm>,
}

impl Measurement {
    pub fn new(label: impl Into<String>, shape: Shape) -> Self {
        Measurement { label: label.into(), shape, alarm: None }
    }
}

/// Statistics over a tool's pixels, in kelvin. NaN pixels are skipped; `count` is the number used.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
    pub std_dev: f32,
    pub min_at: [usize; 2],
    pub max_at: [usize; 2],
    pub count: usize,
}

/// The pixels a shape covers, clipped to the image. `kelvin` is only read by the magic wand.
pub fn pixels(shape: &Shape, kelvin: &[f32], width: usize, height: usize) -> Vec<[usize; 2]> {
    let inside = |x: f32, y: f32| x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height;

    match shape {
        &Shape::Spot { x, y } => {
            if inside(x, y) {
                vec![[x as usize, y as usize]]
            } else {
                Vec::new()
            }
        }
        &Shape::Box { x, y, w, h } => {
            let (x0, x1) = (x.min(x + w), x.max(x + w));
            let (y0, y1) = (y.min(y + h), y.max(y + h));
            area(width, height, [x0, y0, x1, y1], |px, py| px >= x0 && px <= x1 && py >= y0 && py <= y1)
                .unwrap_or_else(|| centre(width, height, (x0 + x1) / 2.0, (y0 + y1) / 2.0))
        }
        &Shape::Ellipse { cx, cy, rx, ry } => {
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
        &Shape::Line { x1, y1, x2, y2 } => path_pixels(&[[x1, y1], [x2, y2]], width, height),
        Shape::Polyline { points } => path_pixels(points, width, height),
        Shape::Polygon { points } => {
            if points.len() < 3 {
                return path_pixels(points, width, height);
            }
            let (mut bounds, mut sum) = ([f32::MAX, f32::MAX, f32::MIN, f32::MIN], [0.0, 0.0]);
            for p in points {
                bounds = [bounds[0].min(p[0]), bounds[1].min(p[1]), bounds[2].max(p[0]), bounds[3].max(p[1])];
                sum = [sum[0] + p[0], sum[1] + p[1]];
            }
            let n = points.len() as f32;
            area(width, height, bounds, |px, py| point_in_polygon(points, px, py))
                .unwrap_or_else(|| centre(width, height, sum[0] / n, sum[1] / n))
        }
        &Shape::Wand { x, y, tolerance } => wand(kelvin, width, height, x, y, tolerance),
    }
}

/// Pixels a path crosses, in order, each once. Sampled twice per pixel along each segment.
fn path_pixels(points: &[[f32; 2]], width: usize, height: usize) -> Vec<[usize; 2]> {
    let mut out: Vec<[usize; 2]> = Vec::new();
    let mut push = |x: f32, y: f32| {
        if x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height {
            let c = [x as usize, y as usize];
            if !out.contains(&c) {
                out.push(c);
            }
        }
    };
    if let [p] = points {
        push(p[0], p[1]);
    }
    for seg in points.windows(2) {
        let ([x1, y1], [x2, y2]) = (seg[0], seg[1]);
        let steps = ((x2 - x1).abs().max((y2 - y1).abs()) * 2.0).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            push(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t);
        }
    }
    out
}

/// Even-odd rule; points exactly on an edge may fall either way.
fn point_in_polygon(points: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let ([xi, yi], [xj, yj]) = (points[i], points[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// 4-connected flood fill from the seed pixel over finite temperatures within `tolerance`.
fn wand(kelvin: &[f32], width: usize, height: usize, x: f32, y: f32, tolerance: f32) -> Vec<[usize; 2]> {
    if !(x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height) || kelvin.len() != width * height {
        return Vec::new();
    }
    let (sx, sy) = (x as usize, y as usize);
    let seed = kelvin[sy * width + sx];
    if !seed.is_finite() {
        return Vec::new();
    }
    let tolerance = tolerance.abs();
    let mut seen = vec![false; width * height];
    let mut queue = VecDeque::from([[sx, sy]]);
    seen[sy * width + sx] = true;
    let mut out = Vec::new();
    while let Some([px, py]) = queue.pop_front() {
        out.push([px, py]);
        let neighbours = [
            (px > 0).then(|| [px - 1, py]),
            (px + 1 < width).then(|| [px + 1, py]),
            (py > 0).then(|| [px, py - 1]),
            (py + 1 < height).then(|| [px, py + 1]),
        ];
        for [nx, ny] in neighbours.into_iter().flatten() {
            let i = ny * width + nx;
            if !seen[i] {
                seen[i] = true;
                let v = kelvin[i];
                if v.is_finite() && (v - seed).abs() <= tolerance {
                    queue.push_back([nx, ny]);
                }
            }
        }
    }
    out
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
    stats_of(kelvin, width, &pixels(shape, kelvin, width, height))
}

pub fn stats_of(kelvin: &[f32], width: usize, pixels: &[[usize; 2]]) -> Option<Stats> {
    let mut acc: Option<Stats> = None;
    let (mut sum, mut sum2) = (0.0f64, 0.0f64);
    for &[x, y] in pixels {
        let v = kelvin[y * width + x];
        if !v.is_finite() {
            continue;
        }
        sum += v as f64;
        sum2 += (v as f64) * (v as f64);
        let s =
            acc.get_or_insert(Stats { min: v, max: v, avg: v, std_dev: 0.0, min_at: [x, y], max_at: [x, y], count: 0 });
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
        let n = s.count as f64;
        let mean = sum / n;
        s.avg = mean as f32;
        s.std_dev = (sum2 / n - mean * mean).max(0.0).sqrt() as f32;
        s
    })
}

/// Share (0–1) of the shape's finite pixels that pass `test`; `None` if it has none.
pub fn coverage(kelvin: &[f32], width: usize, height: usize, shape: &Shape, test: impl Fn(f32) -> bool) -> Option<f32> {
    let (mut hit, mut total) = (0usize, 0usize);
    for [x, y] in pixels(shape, kelvin, width, height) {
        let v = kelvin[y * width + x];
        if v.is_finite() {
            total += 1;
            hit += test(v) as usize;
        }
    }
    (total > 0).then(|| hit as f32 / total as f32)
}

/// One sample of a temperature profile.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ProfilePoint {
    /// Distance from the start of the path, in thermal pixels.
    pub d: f32,
    /// Temperature in kelvin; NaN where there is none.
    pub k: f32,
}

/// Temperatures along a line or polyline, one sample per pixel of path length (nearest pixel).
/// Empty for other tools.
pub fn profile(kelvin: &[f32], width: usize, height: usize, shape: &Shape) -> Vec<ProfilePoint> {
    let points: Vec<[f32; 2]> = match shape {
        &Shape::Line { x1, y1, x2, y2 } => vec![[x1, y1], [x2, y2]],
        Shape::Polyline { points } => points.clone(),
        _ => return Vec::new(),
    };
    let sample = |x: f32, y: f32| {
        if x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height {
            kelvin[y as usize * width + x as usize]
        } else {
            f32::NAN
        }
    };
    let mut out = Vec::new();
    let mut travelled = 0.0f32;
    for (i, seg) in points.windows(2).enumerate() {
        let ([x1, y1], [x2, y2]) = (seg[0], seg[1]);
        let len = (x2 - x1).hypot(y2 - y1);
        let steps = len.ceil().max(1.0) as usize;
        // Each segment starts where the previous one ended; skip the shared point.
        for s in usize::from(i > 0)..=steps {
            let t = s as f32 / steps as f32;
            out.push(ProfilePoint { d: travelled + len * t, k: sample(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t) });
        }
        travelled += len;
    }
    out
}

/// Outline of a set of pixels as closed loops of pixel-corner coordinates (for drawing a wand
/// selection). Holes come out as their own loops.
pub fn outline(pixels: &[[usize; 2]], width: usize, height: usize) -> Vec<Vec<[f32; 2]>> {
    let mut mask = vec![false; width * height];
    for &[x, y] in pixels {
        if x < width && y < height {
            mask[y * width + x] = true;
        }
    }
    let set = |x: i64, y: i64| {
        x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height && mask[y as usize * width + x as usize]
    };

    // Directed boundary edges with the region on the left (clockwise in screen coordinates).
    let mut next: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let mut add = |a: (i64, i64), b: (i64, i64)| next.entry(a).or_default().push(b);
    for &[px, py] in pixels {
        let (x, y) = (px as i64, py as i64);
        if !set(x, y - 1) {
            add((x, y), (x + 1, y));
        }
        if !set(x + 1, y) {
            add((x + 1, y), (x + 1, y + 1));
        }
        if !set(x, y + 1) {
            add((x + 1, y + 1), (x, y + 1));
        }
        if !set(x - 1, y) {
            add((x, y + 1), (x, y));
        }
    }

    let mut loops = Vec::new();
    let mut starts: Vec<(i64, i64)> = next.keys().copied().collect();
    starts.sort();
    for start in starts {
        while next.get(&start).is_some_and(|v| !v.is_empty()) {
            let mut lp = vec![[start.0 as f32, start.1 as f32]];
            let mut at = start;
            while let Some(to) = next.get_mut(&at).and_then(|v| v.pop()) {
                at = to;
                if at == start {
                    break;
                }
                lp.push([at.0 as f32, at.1 as f32]);
            }
            loops.push(simplify(lp));
        }
    }
    loops
}

/// Drops corners that lie on a straight run.
fn simplify(points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    let n = points.len();
    if n < 4 {
        return points;
    }
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            (b[0] - a[0]) * (c[1] - b[1]) != (b[1] - a[1]) * (c[0] - b[0])
        })
        .map(|i| points[i])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×3 image whose value is 10·y + x.
    fn image() -> Vec<f32> {
        (0..3).flat_map(|y| (0..4).map(move |x| (10 * y + x) as f32)).collect()
    }

    fn px(shape: &Shape) -> Vec<[usize; 2]> {
        let mut p = pixels(shape, &image(), 4, 3);
        p.sort();
        p
    }

    #[test]
    fn spot_reads_the_pixel_under_it() {
        let s = stats(&image(), 4, 3, &Shape::Spot { x: 2.5, y: 1.5 }).unwrap();
        assert_eq!((s.min, s.max, s.avg, s.count, s.std_dev), (12.0, 12.0, 12.0, 1, 0.0));
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
        // Population standard deviation of 1, 2, 11, 12.
        assert!((s.std_dev - 5.0249).abs() < 1e-3, "{}", s.std_dev);
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
        assert_eq!(
            px(&Shape::Ellipse { cx: 1.5, cy: 1.5, rx: 1.0, ry: 1.0 }),
            vec![[0, 1], [1, 0], [1, 1], [1, 2], [2, 1]]
        );
    }

    #[test]
    fn line_visits_each_crossed_pixel_once() {
        let line = pixels(&Shape::Line { x1: 0.5, y1: 0.5, x2: 3.5, y2: 0.5 }, &image(), 4, 3);
        assert_eq!(line, vec![[0, 0], [1, 0], [2, 0], [3, 0]]);
        let diag = pixels(&Shape::Line { x1: 0.5, y1: 0.5, x2: 2.5, y2: 2.5 }, &image(), 4, 3);
        assert_eq!(diag.first(), Some(&[0, 0]));
        assert_eq!(diag.last(), Some(&[2, 2]));
    }

    #[test]
    fn polygon_uses_pixel_centres_inside() {
        // Triangle below the diagonal: centres strictly inside; those on the diagonal edge may
        // fall either way and here fall outside.
        let tri = Shape::Polygon { points: vec![[0.0, 0.0], [0.0, 3.0], [3.0, 3.0]] };
        assert_eq!(px(&tri), vec![[0, 1], [0, 2], [1, 2]]);
        // A polygon around a whole box equals the box.
        let square = Shape::Polygon { points: vec![[1.0, 0.0], [3.0, 0.0], [3.0, 2.0], [1.0, 2.0]] };
        assert_eq!(
            stats(&image(), 4, 3, &square),
            stats(&image(), 4, 3, &Shape::Box { x: 1.0, y: 0.0, w: 2.0, h: 2.0 })
        );
        // Degenerate: fewer than three points behaves like a path.
        assert_eq!(px(&Shape::Polygon { points: vec![[0.5, 0.5], [2.5, 0.5]] }), vec![[0, 0], [1, 0], [2, 0]]);
    }

    #[test]
    fn polyline_follows_every_segment_once() {
        let pl = Shape::Polyline { points: vec![[0.5, 0.5], [3.5, 0.5], [3.5, 2.5]] };
        let p = pixels(&pl, &image(), 4, 3);
        assert_eq!(p, vec![[0, 0], [1, 0], [2, 0], [3, 0], [3, 1], [3, 2]]);
    }

    #[test]
    fn wand_grows_within_tolerance_and_stays_connected() {
        // 5×1 strip: 10 11 30 11 10. From the left, tolerance 1.5 selects 10, 11 but not past 30.
        let strip = [10.0, 11.0, 30.0, 11.0, 10.0];
        let mut p = pixels(&Shape::Wand { x: 0.5, y: 0.5, tolerance: 1.5 }, &strip, 5, 1);
        p.sort();
        assert_eq!(p, vec![[0, 0], [1, 0]]);
        let all = pixels(&Shape::Wand { x: 0.5, y: 0.5, tolerance: 25.0 }, &strip, 5, 1);
        assert_eq!(all.len(), 5);
        // NaN neither seeds nor joins.
        let gap = [10.0, f32::NAN, 10.0];
        assert_eq!(pixels(&Shape::Wand { x: 0.5, y: 0.5, tolerance: 5.0 }, &gap, 3, 1), vec![[0, 0]]);
        assert!(pixels(&Shape::Wand { x: 1.5, y: 0.5, tolerance: 5.0 }, &gap, 3, 1).is_empty());
    }

    #[test]
    fn wand_follows_new_temperatures() {
        let mut img = image();
        let wand = Shape::Wand { x: 0.5, y: 0.5, tolerance: 1.0 };
        assert_eq!(stats(&img, 4, 3, &wand).unwrap().count, 2); // 0 and 1
        img[2] = 0.9;
        assert_eq!(stats(&img, 4, 3, &wand).unwrap().count, 3);
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
    fn coverage_counts_finite_pixels_passing() {
        let whole = Shape::Box { x: 0.0, y: 0.0, w: 4.0, h: 3.0 };
        assert_eq!(coverage(&image(), 4, 3, &whole, |v| v >= 20.0), Some(4.0 / 12.0));
        let mut img = image();
        img[0] = f32::NAN;
        assert_eq!(coverage(&img, 4, 3, &whole, |_| true), Some(1.0));
    }

    #[test]
    fn profile_samples_each_pixel_of_length() {
        let p = profile(&image(), 4, 3, &Shape::Line { x1: 0.5, y1: 1.5, x2: 3.5, y2: 1.5 });
        let ks: Vec<f32> = p.iter().map(|s| s.k).collect();
        assert_eq!(ks, vec![10.0, 11.0, 12.0, 13.0]);
        assert_eq!(p.last().unwrap().d, 3.0);
        let pl = profile(&image(), 4, 3, &Shape::Polyline { points: vec![[0.5, 0.5], [2.5, 0.5], [2.5, 2.5]] });
        let ks: Vec<f32> = pl.iter().map(|s| s.k).collect();
        assert_eq!(ks, vec![0.0, 1.0, 2.0, 12.0, 22.0]);
        assert_eq!(pl.last().unwrap().d, 4.0);
        assert!(profile(&image(), 4, 3, &Shape::Spot { x: 1.0, y: 1.0 }).is_empty());
    }

    #[test]
    fn alarm_checks_chosen_statistic() {
        let s = stats(&image(), 4, 3, &Shape::Box { x: 1.0, y: 0.0, w: 2.0, h: 2.0 }).unwrap();
        let above = Alarm { stat: Stat::Max, above: Some(11.5), below: None };
        assert_eq!(above.check(Some(&s)), Some(true));
        let avg = Alarm { stat: Stat::Avg, above: Some(11.5), below: Some(2.0) };
        assert_eq!(avg.check(Some(&s)), Some(false));
        assert_eq!(Alarm::default().check(Some(&s)), None);
        assert_eq!(above.check(None), None);
    }

    #[test]
    fn outline_of_square_and_of_shape_with_hole() {
        let square = outline(&[[1, 1], [2, 1], [1, 2], [2, 2]], 4, 4);
        assert_eq!(square.len(), 1);
        let mut corners = square[0].clone();
        corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(corners, vec![[1.0, 1.0], [1.0, 3.0], [3.0, 1.0], [3.0, 3.0]]);

        let ring: Vec<[usize; 2]> = (0..3).flat_map(|y| (0..3).map(move |x| [x, y])).filter(|&p| p != [1, 1]).collect();
        let loops = outline(&ring, 3, 3);
        assert_eq!(loops.len(), 2, "{loops:?}");
        assert!(loops.iter().all(|l| l.len() == 4));
    }

    #[test]
    fn shapes_serialize_with_kind_tag() {
        let m = Measurement::new("Bx1", Shape::Box { x: 1.0, y: 2.0, w: 3.0, h: 4.0 });
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"label":"Bx1","shape":{"kind":"box","x":1.0,"y":2.0,"w":3.0,"h":4.0}}"#);
        assert_eq!(serde_json::from_str::<Measurement>(&json).unwrap(), m);
        let poly: Shape = serde_json::from_str(r#"{"kind":"polygon","points":[[0,0],[1,0],[1,1]]}"#).unwrap();
        assert_eq!(poly, Shape::Polygon { points: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]] });
        let with_alarm: Measurement = serde_json::from_str(
            r#"{"label":"Sp1","shape":{"kind":"spot","x":1,"y":1},"alarm":{"stat":"avg","above":300}}"#,
        )
        .unwrap();
        assert_eq!(with_alarm.alarm, Some(Alarm { stat: Stat::Avg, above: Some(300.0), below: None }));
    }
}

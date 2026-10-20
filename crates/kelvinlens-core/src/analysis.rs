//! Isotherms and colour alarms, building-physics helpers, and automatic inspection.
//!
//! The condensation and insulation alarms follow FLIR's camera manual: the condensation alarm
//! marks surfaces where the air's water vapour would reach a relative humidity limit, and the
//! insulation alarm marks surfaces whose thermal index falls below a limit. FLIR does not publish
//! its formulas; we use the Magnus formula (Alduchov & Eskridge 1996) for saturation vapour
//! pressure and the ISO 13788 temperature factor `fRsi = (θsi − θe) / (θi − θe)` as the
//! thermal index (×100).

use serde::{Deserialize, Serialize};

use crate::measure::{self, Shape};

/// Which pixels an isotherm colours.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum IsothermKind {
    /// Hotter than `t` (kelvin).
    Above { t: f32 },
    /// Colder than `t` (kelvin).
    Below { t: f32 },
    /// Between `lo` and `hi` (kelvin), inclusive.
    Interval { lo: f32, hi: f32 },
    /// Surfaces where air at `air_k` with relative humidity `rh` (0–1) would reach `limit` (0–1)
    /// relative humidity at the surface. `limit` 1.0 is the dew point; about 0.7 risks mould.
    Condensation { air_k: f32, rh: f32, limit: f32 },
    /// Surfaces whose thermal index `(T − outside) / (inside − outside)` is below `index` (0–1).
    Insulation { inside_k: f32, outside_k: f32, index: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Isotherm {
    #[serde(flatten)]
    pub kind: IsothermKind,
    pub color: [u8; 3],
    /// 1.0 paints the colour solid; less lets the palette show through.
    #[serde(default = "one")]
    pub opacity: f32,
}

fn one() -> f32 {
    1.0
}

impl IsothermKind {
    /// The surface temperature (kelvin) at or below which the condensation alarm triggers.
    pub fn condensation_limit_k(air_k: f32, rh: f32, limit: f32) -> f32 {
        // Vapour pressure in the air, then the surface temperature whose saturation pressure is
        // that pressure divided by the limit.
        let e = rh.clamp(0.0, 1.0) * saturation_hpa(air_k - 273.15);
        dew_point_c(e / limit.clamp(0.01, 1.0)) + 273.15
    }

    /// A predicate for one temperature (kelvin). NaN never matches.
    pub fn matcher(&self) -> impl Fn(f32) -> bool + use<> {
        let kind = *self;
        let condensation = match kind {
            IsothermKind::Condensation { air_k, rh, limit } => Self::condensation_limit_k(air_k, rh, limit),
            _ => f32::NAN,
        };
        move |k: f32| {
            if !k.is_finite() {
                return false;
            }
            match kind {
                IsothermKind::Above { t } => k > t,
                IsothermKind::Below { t } => k < t,
                IsothermKind::Interval { lo, hi } => k >= lo.min(hi) && k <= hi.max(lo),
                IsothermKind::Condensation { .. } => k <= condensation,
                IsothermKind::Insulation { inside_k, outside_k, index } => {
                    let span = inside_k - outside_k;
                    span.abs() > f32::EPSILON && (k - outside_k) / span < index
                }
            }
        }
    }
}

/// Saturation vapour pressure over water in hPa (Magnus, Alduchov & Eskridge 1996).
pub fn saturation_hpa(celsius: f32) -> f32 {
    6.1094 * (17.625 * celsius / (celsius + 243.04)).exp()
}

/// Temperature (°C) at which `vapour_hpa` is the saturation pressure: the inverse of
/// `saturation_hpa`.
pub fn dew_point_c(vapour_hpa: f32) -> f32 {
    let g = (vapour_hpa / 6.1094).ln();
    243.04 * g / (17.625 - g)
}

/// Relative humidity (0–1) the air would have at a surface of `surface_k`.
pub fn surface_humidity(air_k: f32, rh: f32, surface_k: f32) -> f32 {
    rh * saturation_hpa(air_k - 273.15) / saturation_hpa(surface_k - 273.15)
}

/// ISO 13788 temperature factor, the thermal index as a fraction.
pub fn thermal_index(surface_k: f32, inside_k: f32, outside_k: f32) -> f32 {
    (surface_k - outside_k) / (inside_k - outside_k)
}

/// Mask of pixels matched by any of `isotherms`.
pub fn isotherm_mask(kelvin: &[f32], isotherms: &[Isotherm]) -> Vec<bool> {
    let matchers: Vec<_> = isotherms.iter().map(|i| i.kind.matcher()).collect();
    kelvin.iter().map(|&k| matchers.iter().any(|m| m(k))).collect()
}

// ---------------------------------------------------------------------------- auto-inspect

/// What the automatic inspection looks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InspectMode {
    /// Hot spots, graded with NETA MTS Table 100.18 (electrical).
    #[default]
    Hot,
    /// Cold spots (heat loss, moisture, air leaks).
    Cold,
    Both,
}

/// What the temperature rise of a hot spot is compared with, for NETA grading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Basis {
    /// The reference is a similar component under similar load (NETA "O/S").
    #[default]
    Similar,
    /// The reference is the ambient air temperature (NETA "O/A").
    Ambient,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InspectSettings {
    pub mode: InspectMode,
    /// Reference temperature (kelvin); `None` uses the image's median.
    pub reference_k: Option<f32>,
    pub basis: Basis,
    /// Smallest difference from the reference that counts as an anomaly (kelvin).
    pub min_delta: f32,
    /// Smallest anomaly, in pixels; smaller ones are treated as noise.
    pub min_pixels: usize,
    /// Most findings to return, largest difference first.
    pub max_findings: usize,
    /// For cold spots with indoor and outdoor temperatures: grade by thermal index instead.
    pub inside_k: Option<f32>,
    pub outside_k: Option<f32>,
}

impl Default for InspectSettings {
    fn default() -> Self {
        InspectSettings {
            mode: InspectMode::Hot,
            reference_k: None,
            basis: Basis::Similar,
            min_delta: 1.0,
            min_pixels: 4,
            max_findings: 12,
            inside_k: None,
            outside_k: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Severity {
    /// 1 (least) to 4 (most severe).
    pub level: u8,
    pub label: String,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub label: String,
    pub hot: bool,
    /// Bounding box of the anomaly, in thermal pixels.
    pub area: Shape,
    pub pixels: usize,
    /// Hottest (or coldest) temperature in the anomaly, and where (kelvin, pixel).
    pub extreme_k: f32,
    pub at: [usize; 2],
    pub reference_k: f32,
    /// `extreme − reference` (kelvin); negative for cold spots.
    pub delta_k: f32,
    /// Thermal index of the coldest point (0–1), when indoor and outdoor temperatures are known.
    pub thermal_index: Option<f32>,
    pub severity: Severity,
}

/// NETA MTS Table 100.18 grading of a temperature rise.
pub fn neta_severity(delta_k: f32, basis: Basis) -> Severity {
    let (level, label, action) = match basis {
        Basis::Similar => match delta_k {
            d if d > 15.0 => (4, "Major discrepancy", "Repair immediately"),
            d if d >= 4.0 => (3, "Probable deficiency", "Repair as time permits"),
            _ => (1, "Possible deficiency", "Warrants investigation"),
        },
        Basis::Ambient => match delta_k {
            d if d > 40.0 => (4, "Major discrepancy", "Repair immediately"),
            d if d > 20.0 => (3, "Serious deficiency", "Monitor until corrective measures can be accomplished"),
            d if d > 10.0 => (2, "Probable deficiency", "Repair as time permits"),
            _ => (1, "Possible deficiency", "Warrants investigation"),
        },
    };
    Severity { level, label: label.into(), action: action.into() }
}

/// Grading of a cold spot by thermal index; 0.7 is the usual limit for new buildings (FLIR
/// suggests 60–80 %), and below 0.5 condensation is likely in heated rooms.
pub fn thermal_index_severity(index: f32) -> Severity {
    let (level, label, action) = match index {
        i if i < 0.5 => (4, "Severe heat loss", "Condensation likely; repair insulation or air leak"),
        i if i < 0.6 => (3, "Poor insulation", "Mould risk; plan repair"),
        i if i < 0.7 => (2, "Below recommended insulation", "Investigate"),
        _ => (1, "Minor cold area", "Monitor"),
    };
    Severity { level, label: label.into(), action: action.into() }
}

/// Grading of a cold spot by how much colder it is than the reference, when no indoor and
/// outdoor temperatures are given. These bands are a convention, not a standard.
pub fn cold_delta_severity(delta_k: f32) -> Severity {
    let d = -delta_k;
    let (level, label, action) = match d {
        d if d > 10.0 => (4, "Major cold anomaly", "Investigate promptly"),
        d if d > 5.0 => (3, "Significant cold anomaly", "Investigate"),
        d if d > 2.0 => (2, "Moderate cold anomaly", "Check for moisture or air leak"),
        _ => (1, "Minor cold anomaly", "Monitor"),
    };
    Severity { level, label: label.into(), action: action.into() }
}

/// Median of the finite temperatures.
pub fn median(kelvin: &[f32]) -> Option<f32> {
    let mut v: Vec<f32> = kelvin.iter().copied().filter(|k| k.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    let mid = v.len() / 2;
    let (_, m, _) = v.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    Some(*m)
}

/// Finds hot and/or cold anomalies: 8-connected groups of pixels at least `min_delta` from the
/// reference, at least `min_pixels` large, graded for severity.
pub fn inspect(kelvin: &[f32], width: usize, height: usize, settings: &InspectSettings) -> Vec<Finding> {
    let Some(reference) = settings.reference_k.or_else(|| median(kelvin)) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    let modes: &[bool] = match settings.mode {
        InspectMode::Hot => &[true],
        InspectMode::Cold => &[false],
        InspectMode::Both => &[true, false],
    };
    for &hot in modes {
        let delta = settings.min_delta.abs();
        let mask: Vec<bool> = kelvin
            .iter()
            .map(|&k| k.is_finite() && if hot { k >= reference + delta } else { k <= reference - delta })
            .collect();
        for region in components(&mask, width, height) {
            if region.len() < settings.min_pixels.max(1) {
                continue;
            }
            let s = measure::stats_of(kelvin, width, &region).expect("region pixels are finite");
            let (extreme, at) = if hot { (s.max, s.max_at) } else { (s.min, s.min_at) };
            let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
            for &[x, y] in &region {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            let delta_k = extreme - reference;
            let thermal_index = match (hot, settings.inside_k, settings.outside_k) {
                (false, Some(i), Some(o)) if (i - o).abs() > f32::EPSILON => Some(thermal_index(extreme, i, o)),
                _ => None,
            };
            let severity = match (hot, thermal_index) {
                (true, _) => neta_severity(delta_k, settings.basis),
                (false, Some(ti)) => thermal_index_severity(ti),
                (false, None) => cold_delta_severity(delta_k),
            };
            findings.push(Finding {
                label: String::new(),
                hot,
                area: Shape::Box { x: x0 as f32, y: y0 as f32, w: (x1 - x0 + 1) as f32, h: (y1 - y0 + 1) as f32 },
                pixels: region.len(),
                extreme_k: extreme,
                at,
                reference_k: reference,
                delta_k,
                thermal_index,
                severity,
            });
        }
    }
    findings.sort_by(|a, b| b.severity.level.cmp(&a.severity.level).then(b.delta_k.abs().total_cmp(&a.delta_k.abs())));
    findings.truncate(settings.max_findings.max(1));
    for (i, f) in findings.iter_mut().enumerate() {
        f.label = format!("A{}", i + 1);
    }
    findings
}

/// 8-connected groups of `true` pixels.
fn components(mask: &[bool], width: usize, height: usize) -> Vec<Vec<[usize; 2]>> {
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut region = Vec::new();
        while let Some(i) = stack.pop() {
            let (x, y) = (i % width, i / width);
            region.push([x, y]);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                        continue;
                    }
                    let j = ny as usize * width + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        out.push(region);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: f32 = 273.15;

    #[test]
    fn magnus_matches_reference_values() {
        // Saturation pressure at 20 °C is 23.39 hPa (Magnus is within 0.3 % of it); the dew point
        // of 20 °C air at 50 % is 9.3 °C.
        assert!((saturation_hpa(20.0) - 23.39).abs() < 0.08, "{}", saturation_hpa(20.0));
        let dew = dew_point_c(0.5 * saturation_hpa(20.0));
        assert!((dew - 9.26).abs() < 0.05, "{dew}");
        // Inverse round trip.
        assert!((dew_point_c(saturation_hpa(-5.0)) + 5.0).abs() < 1e-3);
    }

    #[test]
    fn condensation_limit_is_dew_point_at_100_percent() {
        let k = IsothermKind::condensation_limit_k(20.0 + C, 0.5, 1.0);
        assert!((k - C - 9.26).abs() < 0.05);
        // A 70 % limit triggers at a warmer surface than the dew point.
        let mould = IsothermKind::condensation_limit_k(20.0 + C, 0.5, 0.7);
        assert!(mould > k);
        assert!((surface_humidity(20.0 + C, 0.5, mould) - 0.7).abs() < 1e-3);
        let m = IsothermKind::Condensation { air_k: 20.0 + C, rh: 0.5, limit: 1.0 }.matcher();
        assert!(m(9.0 + C) && !m(9.5 + C) && !m(f32::NAN));
    }

    #[test]
    fn insulation_uses_thermal_index() {
        // Inside 20 °C, outside 0 °C: a 13 °C surface has index 0.65.
        assert!((thermal_index(13.0 + C, 20.0 + C, C) - 0.65).abs() < 1e-5);
        let m = IsothermKind::Insulation { inside_k: 20.0 + C, outside_k: C, index: 0.7 }.matcher();
        assert!(m(13.0 + C) && !m(15.0 + C));
    }

    #[test]
    fn simple_isotherms() {
        let above = IsothermKind::Above { t: 300.0 }.matcher();
        assert!(above(300.5) && !above(300.0));
        let iv = IsothermKind::Interval { lo: 310.0, hi: 300.0 }.matcher();
        assert!(iv(300.0) && iv(305.0) && iv(310.0) && !iv(310.1));
        let mask = isotherm_mask(
            &[290.0, 305.0, 320.0],
            &[Isotherm { kind: IsothermKind::Below { t: 295.0 }, color: [0, 0, 255], opacity: 1.0 }],
        );
        assert_eq!(mask, vec![true, false, false]);
    }

    #[test]
    fn neta_bands() {
        assert_eq!(neta_severity(2.0, Basis::Similar).level, 1);
        assert_eq!(neta_severity(4.0, Basis::Similar).level, 3);
        assert_eq!(neta_severity(16.0, Basis::Similar).action, "Repair immediately");
        assert_eq!(neta_severity(10.0, Basis::Ambient).level, 1);
        assert_eq!(neta_severity(15.0, Basis::Ambient).level, 2);
        assert_eq!(neta_severity(30.0, Basis::Ambient).level, 3);
        assert_eq!(neta_severity(41.0, Basis::Ambient).level, 4);
    }

    /// 10×10 image at 300 K with a 3×3 hot spot at 320 K and a 2×2 cold spot at 290 K.
    fn scene() -> Vec<f32> {
        let mut k = vec![300.0; 100];
        for y in 2..5 {
            for x in 2..5 {
                k[y * 10 + x] = 320.0;
            }
        }
        k[3 * 10 + 3] = 325.0;
        for y in 7..9 {
            for x in 6..8 {
                k[y * 10 + x] = 290.0;
            }
        }
        k
    }

    #[test]
    fn inspect_finds_and_grades_hot_spot() {
        let f = inspect(&scene(), 10, 10, &InspectSettings::default());
        assert_eq!(f.len(), 1);
        let a = &f[0];
        assert_eq!((a.label.as_str(), a.hot, a.pixels, a.at), ("A1", true, 9, [3, 3]));
        assert_eq!(a.area, Shape::Box { x: 2.0, y: 2.0, w: 3.0, h: 3.0 });
        assert_eq!((a.reference_k, a.delta_k), (300.0, 25.0));
        assert_eq!(a.severity.level, 4);
    }

    #[test]
    fn inspect_cold_with_thermal_index_and_noise_filter() {
        let s = InspectSettings {
            mode: InspectMode::Both,
            inside_k: Some(300.0),
            outside_k: Some(270.0),
            ..Default::default()
        };
        let f = inspect(&scene(), 10, 10, &s);
        assert_eq!(f.len(), 2);
        let cold = f.iter().find(|f| !f.hot).unwrap();
        // (290 − 270) / (300 − 270) = 0.667
        assert!((cold.thermal_index.unwrap() - 0.6667).abs() < 1e-3);
        assert_eq!(cold.severity.level, 2);
        let strict = InspectSettings { min_pixels: 5, mode: InspectMode::Cold, ..Default::default() };
        assert!(inspect(&scene(), 10, 10, &strict).is_empty());
    }

    #[test]
    fn median_ignores_nan() {
        assert_eq!(median(&[3.0, f32::NAN, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[f32::NAN]), None);
    }

    #[test]
    fn isotherm_json_is_flat() {
        let i = Isotherm { kind: IsothermKind::Above { t: 310.0 }, color: [255, 0, 0], opacity: 0.5 };
        let json = serde_json::to_string(&i).unwrap();
        assert_eq!(json, r#"{"kind":"above","t":310.0,"color":[255,0,0],"opacity":0.5}"#);
        let back: Isotherm =
            serde_json::from_str(r#"{"kind":"insulation","insideK":293,"outsideK":273,"index":0.7,"color":[0,0,255]}"#)
                .unwrap();
        assert_eq!(back.opacity, 1.0);
    }
}

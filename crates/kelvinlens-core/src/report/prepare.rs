//! Turns `ReportInfo` + `ReportImage` into `Prepared`: every value computed and formatted once,
//! shared by the PDF and HTML writers.
use std::collections::BTreeMap;

use base64::Engine as _;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};

use super::{
    Chart, FindingRow, LegendRow, MeasurementRow, OverviewRow, Paper, Prepared, PreparedImage, ReportImage, ReportInfo,
    Series, Template, TemplateKind, trades,
};
use crate::analysis::{self, Basis, Isotherm, IsothermKind};
use crate::formula::{self, ResultKind};
use crate::measure::{self, Shape};
use crate::params::{Param, Params};
use crate::render::{self, Distribution};
use crate::{Error, Result, ThermalImage, Unit, export};

/// Photos are scaled down to this width; report pages don't need more.
const PHOTO_MAX_WIDTH: u32 = 1600;

const DISCLAIMER: &str = "Temperatures are computed from the camera's radiometric data with the \
parameters listed for each image. Their accuracy depends on the camera's calibration and on \
those parameters matching the real conditions.";

// Print palette for white paper. The UI has a lighter palette for the dark theme in src/charts.ts SERIES_COLORS; keep the order (series i has the same hue family in both).
const SERIES_COLORS: [&str; 8] =
    ["#dc2626", "#2563eb", "#16a34a", "#9333ea", "#ea580c", "#0891b2", "#be185d", "#4d7c0f"];

pub(super) fn prepare(info: &ReportInfo, images: &[ReportImage], generator: &str) -> Result<Prepared> {
    let template = info.template.clone().unwrap_or_default();
    let unit = info.unit;
    let t = |k: f32| unit.format(k);

    let mut prepared_images = Vec::with_capacity(images.len());
    for (i, item) in images.iter().enumerate() {
        prepared_images.push(prepare_image(i + 1, item, info, &template)?);
    }

    let mut cover_rows: Vec<[String; 2]> = vec![
        ["Site".into(), info.site.trim().into()],
        ["Inspector".into(), info.inspector.trim().into()],
        ["Date".into(), info.date.trim().into()],
    ];
    for f in &template.cover_fields {
        cover_rows.push([f.clone(), info.fields.get(f).map(|v| v.trim().to_string()).unwrap_or_default()]);
    }
    cover_rows.push(["Images".into(), images.len().to_string()]);
    cover_rows.retain(|r| !r[1].is_empty());

    let c = info.conditions;
    let mut conditions: Vec<[String; 2]> = Vec::new();
    if let Some(k) = c.inside_k {
        conditions.push(["Indoor temperature".into(), t(k)]);
    }
    if let Some(k) = c.outside_k {
        conditions.push(["Outdoor temperature".into(), t(k)]);
    }
    if let Some(rh) = c.humidity {
        conditions.push(["Indoor relative humidity".into(), format!("{:.0} %", rh * 100.0)]);
        if let Some(inside) = c.inside_k {
            let dew = analysis::dew_point_c(rh * analysis::saturation_hpa(inside - 273.15)) + 273.15;
            conditions.push(["Dew point".into(), t(dew)]);
            let mould = IsothermKind::condensation_limit_k(inside, rh, 0.8);
            conditions.push(["Mould risk below (80 % surface RH)".into(), t(mould)]);
        }
    }
    if let Some(k) = c.ambient_k {
        conditions.push(["Ambient temperature".into(), t(k)]);
    }
    if template.kind == TemplateKind::Electrical {
        let basis = match c.basis {
            Basis::Similar => "Rise over a similar component (NETA MTS Table 100.18, O/S)",
            Basis::Ambient => "Rise over ambient air (NETA MTS Table 100.18, O/A)",
        };
        conditions.push(["Severity basis".into(), basis.into()]);
    }

    let mut overview = Vec::new();
    for (img, item) in prepared_images.iter().zip(images) {
        for f in &item.findings {
            overview.push(OverviewRow {
                image: format!("{}. {}", img.number, img.name),
                label: f.label.clone(),
                kind: if f.hot { "Hot".into() } else { "Cold".into() },
                delta: signed(unit, f.delta_k),
                level: f.severity.level,
                severity: f.severity.label.clone(),
                action: f.severity.action.clone(),
            });
        }
    }
    overview.sort_by_key(|r| std::cmp::Reverse(r.level));

    let logo = template.logo.as_deref().filter(|s| !s.trim().is_empty()).map(decode_logo).transpose()?;

    Ok(Prepared {
        title: if info.title.trim().is_empty() { "Thermal inspection report".into() } else { info.title.trim().into() },
        site: info.site.trim().into(),
        inspector: info.inspector.trim().into(),
        date: info.date.trim().into(),
        summary: info.summary.trim().into(),
        paper: match info.paper {
            Paper::A4 => "a4",
            Paper::UsLetter => "us-letter",
        },
        generator: generator.into(),
        disclaimer: if template.disclaimer.trim().is_empty() {
            DISCLAIMER.into()
        } else {
            template.disclaimer.trim().into()
        },
        company: template.company.trim().into(),
        accent: valid_hex(&template.accent).unwrap_or("#c2410c").into(),
        footer: template.footer.trim().into(),
        kind: template.kind,
        cover_rows,
        conditions,
        sections: template.sections,
        image_sections: template.image_sections.clone(),
        images_per_page: template.images_per_page.clamp(1, 2),
        overview,
        trends: if template.sections.trends { trend_charts(images, unit) } else { Vec::new() },
        images: prepared_images,
        logo,
    })
}

pub(super) fn valid_hex(s: &str) -> Option<&str> {
    let s = s.trim();
    (s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())).then_some(s)
}

/// A PNG or JPEG from a data URL or base64, re-encoded as PNG so the typesetter always accepts it.
fn decode_logo(s: &str) -> Result<Vec<u8>> {
    let b64 = s.trim().split_once("base64,").map(|(_, b)| b).unwrap_or(s.trim());
    let bytes = base64_decode(b64).ok_or_else(|| Error::Report("the logo is not valid base64".into()))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| Error::Report(format!("the logo is not a PNG or JPEG image ({e})")))?;
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

use image::ImageEncoder as _;

/// Standard alphabet without padding, as the old hand-written decoder behaved once it had
/// removed every `=`: unused trailing bits are accepted.
const BASE64_UNPADDED: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::RequireNone)
        .with_decode_allow_trailing_bits(true),
);

/// Decodes base64 as the previous hand-written decoder did, so templates saved or imported
/// earlier still work: standard or URL-safe alphabet, whitespace and every `=` ignored; `None`
/// if invalid.
pub(super) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '=')
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    BASE64_UNPADDED.decode(clean).ok()
}

pub fn base64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub(super) fn signed(unit: Unit, dk: f32) -> String {
    unit.format_signed(f64::from(unit.delta_from_kelvin(dk)))
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// Text must match src/panel.ts isothermText (not shared: a shared string needs a Rust-to-UI wire field, deferred).
fn isotherm_text(i: &Isotherm, unit: Unit) -> String {
    let t = |k: f32| unit.format(k);
    match i.kind {
        IsothermKind::Above { t: k } => format!("Above {}", t(k)),
        IsothermKind::Below { t: k } => format!("Below {}", t(k)),
        IsothermKind::Interval { lo, hi } => format!("{} to {}", t(lo.min(hi)), t(hi.max(lo))),
        IsothermKind::Condensation { air_k, rh, limit } => format!(
            "Condensation risk: surface humidity ≥ {:.0} % (air {} at {:.0} %; surfaces ≤ {})",
            limit * 100.0,
            t(air_k),
            rh * 100.0,
            t(IsothermKind::condensation_limit_k(air_k, rh, limit))
        ),
        IsothermKind::Insulation { inside_k, outside_k, index } => format!(
            "Insulation: thermal index below {:.0} % (inside {}, outside {})",
            index * 100.0,
            t(inside_k),
            t(outside_k)
        ),
    }
}

fn prepare_image(number: usize, item: &ReportImage, info: &ReportInfo, template: &Template) -> Result<PreparedImage> {
    let img = item.image;
    let (w, h) = (img.width(), img.height());
    let unit = info.unit;
    let fmt = |k: f32| unit.format(k);
    let kelvin = img.kelvin();

    let thermal_png = export::png(&render::rgb(kelvin, w, h, &item.look), w, h)?;
    let bar_png = export::png(&crate::palette::bar(&item.look.palette, 256), 256, 1)?;
    let photo_jpeg = match (info.include_photos, img.photo()) {
        (true, Some(p)) => Some(photo_jpeg(p)?),
        _ => None,
    };

    let any_isotherm = analysis::any_matcher(&item.look.isotherms);
    let stats: Vec<_> = item.measurements.iter().map(|m| measure::stats(kelvin, w, h, &m.shape)).collect();
    let measurements: Vec<MeasurementRow> = item
        .measurements
        .iter()
        .zip(&stats)
        .map(|(m, s)| {
            let cell = |f: fn(&measure::Stats) -> f32| s.as_ref().map(|s| fmt(f(s))).unwrap_or("–".into());
            let alarm = match m.alarm.and_then(|a| a.check(s.as_ref())) {
                Some(true) => "Alarm",
                Some(false) => "OK",
                None => "",
            };
            let coverage = match &any_isotherm {
                Some(any) if m.shape.is_area() => measure::coverage(kelvin, w, h, &m.shape, any)
                    .map(|c| format!("{:.1} %", c * 100.0))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let outline = measure::outline_of(&m.shape, kelvin, w, h);
            MeasurementRow {
                label: m.label.clone(),
                kind: m.shape.kind_name(),
                shape: m.shape.clone(),
                outline,
                min: if matches!(m.shape, Shape::Spot { .. }) { String::new() } else { cell(|s| s.min) },
                max: if matches!(m.shape, Shape::Spot { .. }) { String::new() } else { cell(|s| s.max) },
                avg: cell(|s| s.avg),
                alarm: alarm.into(),
                coverage,
            }
        })
        .collect();

    let results = formula::evaluate_measured(&item.formulas, &item.measurements, &stats, unit);
    let formulas = item
        .formulas
        .iter()
        .zip(results)
        .map(|(f, r)| {
            let value = match (r.value, r.error) {
                (Some(v), _) => match f.kind {
                    ResultKind::Temperature => unit.format_value(v),
                    ResultKind::Difference => unit.format_signed(v),
                    ResultKind::Number => format!("{v:.3}"),
                },
                (None, Some(e)) => format!("Error: {e}"),
                (None, None) => "–".into(),
            };
            [f.label.clone(), f.expression.clone(), value]
        })
        .collect();

    let findings = item
        .findings
        .iter()
        .map(|f| FindingRow {
            label: f.label.clone(),
            area: f.area.clone(),
            hot: f.hot,
            extreme: fmt(f.extreme_k),
            delta: signed(unit, f.delta_k),
            level: f.severity.level,
            severity: f.severity.label.clone(),
            action: f.severity.action.clone(),
        })
        .collect();

    let profiles = item
        .measurements
        .iter()
        .filter(|m| m.shape.is_path())
        .map(|m| {
            let p = measure::profile(kelvin, w, h, &m.shape);
            let pts: Vec<(f32, f32)> =
                p.iter().filter(|s| s.k.is_finite()).map(|s| (s.d, unit.from_kelvin(s.k))).collect();
            chart(
                format!("Profile along {}", m.label),
                "Distance (pixels)",
                unit.symbol(),
                vec![(m.label.clone(), pts)],
                |x| format!("{x:.0}"),
            )
        })
        .collect();

    let trade = trades::for_kind(template.kind)
        .map(|t| {
            t.rows(&trades::TradeCtx {
                measurements: &item.measurements,
                stats: &stats,
                conditions: info.conditions,
                unit,
                has_rows: !measurements.is_empty(),
            })
        })
        .unwrap_or_default();

    let fields = field_rows(item, template);

    let camera_rows = camera_rows(img, w, h);

    let crop = crop_of(item.presentation.crop, w, h);

    Ok(PreparedImage {
        number,
        name: img.file_name(),
        width: w,
        height: h,
        rotation: item.presentation.rotation.rem_euclid(360.0),
        crop,
        range_min: fmt(item.look.range[0]),
        range_max: fmt(item.look.range[1]),
        image_range: img.range().map(|[lo, hi]| format!("{} to {}", fmt(lo), fmt(hi))),
        palette: item.look.palette.name.clone(),
        distribution: match item.look.distribution {
            Distribution::TemperatureLinear => "Temperature linear",
            Distribution::SignalLinear => "Signal linear",
            Distribution::Histogram => "Histogram equalised",
            Distribution::Detail => "Detail enhanced",
        }
        .into(),
        isotherms: item
            .look
            .isotherms
            .iter()
            .map(|i| LegendRow { color: hex(i.color), text: isotherm_text(i, unit) })
            .collect(),
        measurements,
        formulas,
        findings,
        annotations: item.annotations.clone(),
        profiles,
        electrical: trade.electrical,
        building: trade.building,
        hvac: trade.hvac,
        fields,
        notes: item.notes.trim().into(),
        params: param_rows(&img.params(), &img.file_params(), &img.editable(), unit),
        camera: camera_rows,
        asset: item.asset.trim().into(),
        thermal_png,
        bar_png,
        photo_jpeg,
    })
}

fn field_rows(item: &ReportImage, template: &Template) -> Vec<[String; 2]> {
    let mut fields: Vec<[String; 2]> = Vec::new();
    if !item.asset.trim().is_empty() {
        fields.push(["Asset".into(), item.asset.trim().into()]);
    }
    for f in &template.image_fields {
        if let Some(v) = item.fields.get(f).filter(|v| !v.trim().is_empty()) {
            fields.push([f.clone(), v.trim().into()]);
        }
    }
    for (k, v) in &item.fields {
        if !template.image_fields.contains(k) && !v.trim().is_empty() {
            fields.push([k.clone(), v.trim().into()]);
        }
    }
    fields
}

fn camera_rows(img: &ThermalImage, w: usize, h: usize) -> Vec<[String; 2]> {
    let camera = img.camera();
    let gps = camera.latitude.zip(camera.longitude).map(|(la, lo)| format!("{la:.6}, {lo:.6}"));
    [
        ("Camera", camera.description.clone()),
        ("Serial number", camera.serial_number.clone()),
        ("Lens", camera.lens.clone()),
        ("Field of view", camera.field_of_view.map(|v| format!("{v:.1}°"))),
        ("Captured", camera.date_time.clone()),
        ("GPS", gps),
        ("Altitude", camera.altitude.map(|a| format!("{a:.1} m"))),
        ("Heading", camera.heading.map(|a| format!("{a:.0}°"))),
        ("Gimbal roll / pitch / yaw", camera.gimbal.map(|g| format!("{:.1}° / {:.1}° / {:.1}°", g[0], g[1], g[2]))),
        ("Format", Some(img.format().label().to_string())),
        ("Resolution", Some(format!("{w} × {h}"))),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| [k.to_string(), v]))
    .collect()
}

fn crop_of(crop: Option<[f32; 4]>, w: usize, h: usize) -> [f32; 4] {
    crop.filter(|c| c[2] >= 1.0 && c[3] >= 1.0)
        .map(|c| {
            let x = c[0].clamp(0.0, w as f32 - 1.0);
            let y = c[1].clamp(0.0, h as f32 - 1.0);
            [x, y, c[2].min(w as f32 - x), c[3].min(h as f32 - y)]
        })
        .unwrap_or([0.0, 0.0, w as f32, h as f32])
}

/// A chart from `(name, points)` series in data units.
pub(super) fn chart(
    title: String,
    x_label: &str,
    y_label: &str,
    series: Vec<(String, Vec<(f32, f32)>)>,
    x_fmt: impl Fn(f32) -> String,
) -> Chart {
    let all = series.iter().flat_map(|(_, p)| p.iter());
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in all {
        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
    }
    if x0 > x1 {
        (x0, x1, y0, y1) = (0.0, 1.0, 0.0, 1.0);
    }
    if x1 - x0 < 1e-6 {
        x1 = x0 + 1.0;
    }
    let pad = ((y1 - y0) * 0.08).max(0.25);
    let (y0, y1) = (y0 - pad, y1 + pad);
    let ticks = |lo: f32, hi: f32, f: &dyn Fn(f32) -> String| -> Vec<(f32, String)> {
        (0..=4).map(|i| (i as f32 / 4.0, f(lo + (hi - lo) * i as f32 / 4.0))).collect()
    };
    Chart {
        title,
        x_label: x_label.into(),
        y_label: y_label.into(),
        x_ticks: ticks(x0, x1, &x_fmt),
        y_ticks: ticks(y0, y1, &|v| format!("{v:.1}")),
        series: series
            .into_iter()
            .enumerate()
            .map(|(i, (name, pts))| Series {
                name,
                color: SERIES_COLORS[i % SERIES_COLORS.len()].into(),
                points: pts.iter().map(|&(x, y)| [(x - x0) / (x1 - x0), (y - y0) / (y1 - y0)]).collect(),
            })
            .collect(),
    }
}

/// For each asset with images on at least two dates: the maximum of each measurement label over
/// time.
fn trend_charts(images: &[ReportImage], unit: Unit) -> Vec<Chart> {
    let mut by_asset: BTreeMap<String, Vec<&ReportImage>> = BTreeMap::new();
    for item in images {
        if !item.asset.trim().is_empty() {
            by_asset.entry(item.asset.trim().to_string()).or_default().push(item);
        }
    }
    let mut charts = Vec::new();
    for (asset, items) in by_asset {
        // Order by capture time; images without one keep report order at the end.
        let mut dated: Vec<(String, &ReportImage)> =
            items.iter().map(|i| (i.image.camera().date_time.clone().unwrap_or_else(|| "~".into()), *i)).collect();
        dated.sort_by(|a, b| a.0.cmp(&b.0));
        if dated.len() < 2 {
            continue;
        }
        let mut labels: Vec<String> = Vec::new();
        for (_, it) in &dated {
            for m in &it.measurements {
                if !labels.contains(&m.label) {
                    labels.push(m.label.clone());
                }
            }
        }
        let series: Vec<(String, Vec<(f32, f32)>)> = labels
            .iter()
            .map(|l| {
                let pts = dated
                    .iter()
                    .enumerate()
                    .filter_map(|(x, (_, it))| {
                        let m = it.measurements.iter().find(|m| &m.label == l)?;
                        let s = measure::stats(it.image.kelvin(), it.image.width(), it.image.height(), &m.shape)?;
                        Some((x as f32, unit.from_kelvin(s.max)))
                    })
                    .collect();
                (format!("{l} max"), pts)
            })
            .filter(|(_, p): &(String, Vec<(f32, f32)>)| !p.is_empty())
            .collect();
        if series.is_empty() {
            continue;
        }
        let names: Vec<String> = dated
            .iter()
            .map(|(d, it)| if d == "~" { it.image.file_name() } else { d[..d.len().min(10)].to_string() })
            .collect();
        let n = names.len();
        let mut c = chart(format!("Trend: {asset}"), "Visit", unit.symbol(), series, |_| String::new());
        // One tick per visit, labelled with its date.
        c.x_ticks = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 }, name))
            .collect();
        charts.push(c);
    }
    charts
}

/// Rows of [name, value used, value in file] for the parameters the format records.
pub(super) fn param_rows(used: &Params, file: &Params, editable: &[Param], unit: Unit) -> Vec<[String; 3]> {
    let temp = |k: f32| unit.format(k);
    let show = |p: Param, v: f32| match p {
        Param::Emissivity | Param::WindowTransmission => format!("{v:.2}"),
        Param::ReflectedK | Param::AtmosphereK | Param::WindowK => temp(v),
        Param::DistanceM => format!("{v:.1} m"),
        Param::Humidity => format!("{:.0} %", v * 100.0),
    };
    let name = |p: Param| match p {
        Param::Emissivity => "Emissivity",
        Param::ReflectedK => "Reflected temperature",
        Param::AtmosphereK => "Atmospheric temperature",
        Param::DistanceM => "Distance",
        Param::Humidity => "Relative humidity",
        Param::WindowK => "IR window temperature",
        Param::WindowTransmission => "IR window transmission",
    };
    Param::ALL
        .iter()
        .filter_map(|&p| {
            let v = used.get(p)?;
            let original = file.get(p).map(|f| show(p, f)).unwrap_or_default();
            let changed = editable.contains(&p) && show(p, v) != original;
            Some([name(p).to_string(), show(p, v), if changed { original } else { String::new() }])
        })
        .collect()
}

fn photo_jpeg(photo: &crate::thermogram::Photo) -> Result<Vec<u8>> {
    let rgb = image::RgbImage::from_raw(photo.width as u32, photo.height as u32, photo.rgb.clone())
        .ok_or_else(|| Error::Encode("photo has an unexpected size".into()))?;
    let rgb = if rgb.width() > PHOTO_MAX_WIDTH {
        let height = (rgb.height() as u64 * PHOTO_MAX_WIDTH as u64 / rgb.width() as u64) as u32;
        image::imageops::resize(&rgb, PHOTO_MAX_WIDTH, height.max(1), image::imageops::FilterType::Triangle)
    } else {
        rgb
    };
    export::jpeg(rgb.as_raw(), rgb.width() as usize, rgb.height() as usize, 85)
}

#[cfg(test)]
mod tests {
    use super::crop_of;

    #[test]
    fn crop_is_clamped_to_the_image() {
        assert_eq!(crop_of(Some([150.0, 100.0, 50.0, 50.0]), 160, 120), [150.0, 100.0, 10.0, 20.0]);
        assert_eq!(crop_of(Some([0.0, 0.0, 0.5, 5.0]), 160, 120), [0.0, 0.0, 160.0, 120.0]);
        assert_eq!(crop_of(None, 160, 120), [0.0, 0.0, 160.0, 120.0]);
    }
}

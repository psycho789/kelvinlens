//! Inspection reports from templates, as PDF (typeset with Typst, entirely in memory) or as a
//! self-contained HTML page. No files are read and no network is used; fonts are bundled so the
//! PDF looks the same on every OS.
//!
//! A [`Template`] chooses the sections, their order, branding and the fields asked for. Built-in
//! templates cover general, electrical (NETA severity), building/moisture (thermal index, dew
//! point) and HVAC inspections; users can make unlimited custom ones.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use typst::foundations::{Array, Bytes, Dict, Value};
use typst_as_lib::TypstEngine;

use crate::analysis::{self, Basis, Finding, Isotherm, IsothermKind};
use crate::formula::{self, Formula, ResultKind};
use crate::measure::{self, Measurement, Shape};
use crate::params::{Param, Params};
use crate::render::{self, Distribution, Look};
use crate::sidecar::{Annotation, Presentation};
use crate::{Error, Result, ThermalImage, Unit, export};

const TEMPLATE: &str = include_str!("report.typ");
const FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/FiraSans-Regular.ttf"),
    include_bytes!("../assets/fonts/FiraSans-Bold.ttf"),
    include_bytes!("../assets/fonts/FiraSans-Italic.ttf"),
];
/// Photos are scaled down to this width; report pages don't need more.
const PHOTO_MAX_WIDTH: u32 = 1600;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Paper {
    #[default]
    A4,
    UsLetter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    #[default]
    Pdf,
    Html,
}

/// Which trade a template is for; it decides the extra analysis sections available.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateKind {
    #[default]
    General,
    Electrical,
    Building,
    Hvac,
}

/// Report-level sections that can be switched on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sections {
    pub summary: bool,
    pub contents: bool,
    pub findings_overview: bool,
    pub trends: bool,
    pub signature: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Sections { summary: true, contents: true, findings_overview: true, trends: true, signature: false }
    }
}

/// Sections shown for each image, in this order. Unknown names are ignored.
pub const IMAGE_SECTIONS: [&str; 12] = [
    "isotherms",
    "measurements",
    "formulas",
    "findings",
    "electrical",
    "building",
    "hvac",
    "profiles",
    "fields",
    "notes",
    "parameters",
    "details",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub kind: TemplateKind,
    /// Shown in the page header and on the cover.
    pub company: String,
    /// `#rrggbb`.
    pub accent: String,
    /// PNG or JPEG as a data URL or plain base64; shown on the cover and in the page header.
    pub logo: Option<String>,
    /// Extra fields asked for on the cover, such as "Customer" or "Job number".
    pub cover_fields: Vec<String>,
    /// Fields asked for per image, such as "Component" or "Load %".
    pub image_fields: Vec<String>,
    pub sections: Sections,
    pub image_sections: Vec<String>,
    /// 1 or 2.
    pub images_per_page: u8,
    /// Replaces the standard accuracy note when not empty.
    pub disclaimer: String,
    /// Text in the page footer next to the title.
    pub footer: String,
}

impl Default for Template {
    fn default() -> Self {
        builtin_templates().into_iter().next().expect("there are built-in templates")
    }
}

fn sections(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// The templates that ship with Kelvinlens.
pub fn builtin_templates() -> Vec<Template> {
    let base = |id: &str, name: &str, kind: TemplateKind, image_sections: Vec<String>| Template {
        id: id.into(),
        name: name.into(),
        kind,
        company: String::new(),
        accent: "#c2410c".into(),
        logo: None,
        cover_fields: Vec::new(),
        image_fields: Vec::new(),
        sections: Sections::default(),
        image_sections,
        images_per_page: 1,
        disclaimer: String::new(),
        footer: String::new(),
    };
    vec![
        base(
            "standard",
            "Standard",
            TemplateKind::General,
            sections(&[
                "isotherms",
                "measurements",
                "formulas",
                "findings",
                "profiles",
                "fields",
                "notes",
                "parameters",
                "details",
            ]),
        ),
        Template {
            cover_fields: vec!["Customer".into(), "Equipment".into()],
            image_fields: vec!["Component".into(), "Phase".into(), "Load %".into()],
            accent: "#b45309".into(),
            ..base(
                "electrical",
                "Electrical (NETA)",
                TemplateKind::Electrical,
                sections(&[
                    "fields",
                    "findings",
                    "electrical",
                    "measurements",
                    "formulas",
                    "profiles",
                    "notes",
                    "parameters",
                    "details",
                ]),
            )
        },
        Template {
            cover_fields: vec!["Customer".into(), "Building".into()],
            image_fields: vec!["Room".into(), "Wall / element".into()],
            accent: "#1d4ed8".into(),
            ..base(
                "building",
                "Building / moisture",
                TemplateKind::Building,
                sections(&[
                    "fields",
                    "isotherms",
                    "building",
                    "findings",
                    "measurements",
                    "profiles",
                    "notes",
                    "parameters",
                    "details",
                ]),
            )
        },
        Template {
            cover_fields: vec!["Customer".into(), "System".into()],
            image_fields: vec!["Zone".into(), "Mode (heating/cooling)".into()],
            accent: "#0f766e".into(),
            ..base(
                "hvac",
                "HVAC",
                TemplateKind::Hvac,
                sections(&[
                    "fields",
                    "hvac",
                    "measurements",
                    "formulas",
                    "findings",
                    "profiles",
                    "notes",
                    "parameters",
                    "details",
                ]),
            )
        },
        Template {
            sections: Sections {
                summary: true,
                contents: false,
                findings_overview: true,
                trends: false,
                signature: false,
            },
            images_per_page: 2,
            ..base(
                "compact",
                "Compact (2 per page)",
                TemplateKind::General,
                sections(&["measurements", "findings", "notes"]),
            )
        },
    ]
}

/// Site conditions some templates use: indoor/outdoor temperatures and humidity for building
/// work, ambient temperature and NETA basis for electrical work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Conditions {
    pub inside_k: Option<f32>,
    pub outside_k: Option<f32>,
    /// Indoor relative humidity, 0–1.
    pub humidity: Option<f32>,
    pub ambient_k: Option<f32>,
    pub basis: Basis,
}

/// Report-wide fields, as entered in the report dialog.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportInfo {
    pub title: String,
    pub site: String,
    pub inspector: String,
    pub date: String,
    pub summary: String,
    pub paper: Paper,
    pub unit: Unit,
    pub include_photos: bool,
    pub template: Option<Template>,
    /// Values for the template's cover fields.
    pub fields: BTreeMap<String, String>,
    pub conditions: Conditions,
    pub format: ReportFormat,
}

/// One image in the report, with the view and analysis the user set up for it.
pub struct ReportImage<'a> {
    pub image: &'a ThermalImage,
    pub look: Look,
    pub measurements: Vec<Measurement>,
    pub notes: String,
    pub formulas: Vec<Formula>,
    pub annotations: Vec<Annotation>,
    pub findings: Vec<Finding>,
    pub fields: BTreeMap<String, String>,
    pub asset: String,
    pub presentation: Presentation,
}

impl<'a> ReportImage<'a> {
    /// An image with just a look and measurements; the rest empty.
    pub fn basic(image: &'a ThermalImage, look: Look, measurements: Vec<Measurement>, notes: String) -> Self {
        ReportImage {
            image,
            look,
            measurements,
            notes,
            formulas: Vec::new(),
            annotations: Vec::new(),
            findings: Vec::new(),
            fields: BTreeMap::new(),
            asset: String::new(),
            presentation: Presentation::default(),
        }
    }
}

const DISCLAIMER: &str = "Temperatures are computed from the camera's radiometric data with the \
parameters listed for each image. Their accuracy depends on the camera's calibration and on \
those parameters matching the real conditions.";

/// Builds the report in `info.format`.
pub fn generate(info: &ReportInfo, images: &[ReportImage], generator: &str) -> Result<Vec<u8>> {
    let prepared = prepare(info, images, generator)?;
    match info.format {
        ReportFormat::Pdf => pdf_from(&prepared),
        ReportFormat::Html => Ok(crate::report_html::html(&prepared).into_bytes()),
    }
}

// ---------------------------------------------------------------------------- prepared data

/// Everything a report shows, already computed and formatted, for the PDF and HTML writers.
#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub title: String,
    pub site: String,
    pub inspector: String,
    pub date: String,
    pub summary: String,
    pub paper: &'static str,
    pub generator: String,
    pub disclaimer: String,
    pub company: String,
    pub accent: String,
    pub footer: String,
    pub kind: TemplateKind,
    pub cover_rows: Vec<[String; 2]>,
    pub conditions: Vec<[String; 2]>,
    pub sections: Sections,
    pub image_sections: Vec<String>,
    pub images_per_page: u8,
    pub overview: Vec<OverviewRow>,
    pub trends: Vec<Chart>,
    pub images: Vec<PreparedImage>,
    #[serde(skip)]
    pub logo: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OverviewRow {
    pub image: String,
    pub label: String,
    pub kind: String,
    pub delta: String,
    pub level: u8,
    pub severity: String,
    pub action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedImage {
    pub number: usize,
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub rotation: f32,
    /// Crop `[x, y, w, h]` in thermal pixels (the whole image when not cropped).
    pub crop: [f32; 4],
    pub range_min: String,
    pub range_max: String,
    pub image_range: Option<String>,
    pub palette: String,
    pub distribution: String,
    pub isotherms: Vec<LegendRow>,
    pub measurements: Vec<MeasurementRow>,
    pub formulas: Vec<[String; 3]>,
    pub findings: Vec<FindingRow>,
    pub annotations: Vec<Annotation>,
    pub profiles: Vec<Chart>,
    pub electrical: Vec<Vec<String>>,
    pub building: Vec<Vec<String>>,
    pub hvac: Vec<[String; 2]>,
    pub fields: Vec<[String; 2]>,
    pub notes: String,
    pub params: Vec<[String; 3]>,
    pub camera: Vec<[String; 2]>,
    pub asset: String,
    #[serde(skip)]
    pub thermal_png: Vec<u8>,
    #[serde(skip)]
    pub bar_png: Vec<u8>,
    #[serde(skip)]
    pub photo_jpeg: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegendRow {
    pub color: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeasurementRow {
    pub label: String,
    pub kind: &'static str,
    pub shape: Shape,
    /// Outline loops for the magic wand, in thermal pixels.
    pub outline: Vec<Vec<[f32; 2]>>,
    pub min: String,
    pub max: String,
    pub avg: String,
    /// "", "Alarm" or "OK".
    pub alarm: String,
    /// Share of the area matched by the image's isotherms, e.g. "12.5 %".
    pub coverage: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FindingRow {
    pub label: String,
    pub area: Shape,
    pub hot: bool,
    pub extreme: String,
    pub delta: String,
    pub level: u8,
    pub severity: String,
    pub action: String,
}

/// A line chart with coordinates normalised to 0–1 (y up), ready to draw.
#[derive(Clone, Debug, Serialize)]
pub struct Chart {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub x_ticks: Vec<(f32, String)>,
    pub y_ticks: Vec<(f32, String)>,
    pub series: Vec<Series>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Series {
    pub name: String,
    pub color: String,
    pub points: Vec<[f32; 2]>,
}

const SERIES_COLORS: [&str; 8] =
    ["#dc2626", "#2563eb", "#16a34a", "#9333ea", "#ea580c", "#0891b2", "#be185d", "#4d7c0f"];

fn prepare(info: &ReportInfo, images: &[ReportImage], generator: &str) -> Result<Prepared> {
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
        image_sections: template
            .image_sections
            .iter()
            .filter(|s| IMAGE_SECTIONS.contains(&s.as_str()))
            .cloned()
            .collect(),
        images_per_page: template.images_per_page.clamp(1, 2),
        overview,
        trends: if template.sections.trends { trend_charts(images, unit) } else { Vec::new() },
        images: prepared_images,
        logo,
    })
}

fn valid_hex(s: &str) -> Option<&str> {
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
fn base64_decode(s: &str) -> Option<Vec<u8>> {
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

fn signed(unit: Unit, dk: f32) -> String {
    unit.format_signed(f64::from(unit.delta_from_kelvin(dk)))
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

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
            let outline = match m.shape {
                Shape::Wand { .. } => measure::outline(&measure::pixels(&m.shape, kelvin, w, h), w, h),
                _ => Vec::new(),
            };
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

    let c = info.conditions;
    // Electrical: each measurement's maximum over the ambient (or the coldest measurement as the
    // similar-component reference), graded with NETA.
    let electrical = if template.kind == TemplateKind::Electrical && !measurements.is_empty() {
        let reference = match c.basis {
            Basis::Ambient => c.ambient_k.map(|k| (k, "ambient".to_string())),
            Basis::Similar => stats
                .iter()
                .zip(&item.measurements)
                .filter_map(|(s, m)| s.map(|s| (s.max, m.label.clone())))
                .min_by(|a, b| a.0.total_cmp(&b.0)),
        };
        match reference {
            Some((rk, rname)) => stats
                .iter()
                .zip(&item.measurements)
                .filter_map(|(s, m)| {
                    let s = s.as_ref()?;
                    let d = s.max - rk;
                    let sev = analysis::neta_severity(d, c.basis);
                    if c.basis == Basis::Similar && m.label == rname {
                        return Some(vec![
                            m.label.clone(),
                            fmt(s.max),
                            "Reference".into(),
                            "–".into(),
                            "Coolest similar component".into(),
                        ]);
                    }
                    let grade = if d < 1.0 {
                        ("–".to_string(), "No significant rise".to_string())
                    } else {
                        (sev.label, sev.action)
                    };
                    Some(vec![m.label.clone(), fmt(s.max), format!("{} ({rname})", signed(unit, d)), grade.0, grade.1])
                })
                .collect(),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Building: thermal index and surface humidity at each measurement's coldest point.
    let building = if template.kind == TemplateKind::Building {
        stats
            .iter()
            .zip(&item.measurements)
            .filter_map(|(s, m)| {
                let s = s.as_ref()?;
                let ti = match (c.inside_k, c.outside_k) {
                    (Some(i), Some(o)) if (i - o).abs() > 0.5 => {
                        format!("{:.0} %", analysis::thermal_index(s.min, i, o) * 100.0)
                    }
                    _ => "–".into(),
                };
                let (rh, risk) = match (c.inside_k, c.humidity) {
                    (Some(i), Some(rh)) => {
                        let srh = analysis::surface_humidity(i, rh, s.min);
                        let risk = if srh >= 1.0 {
                            "Condensation"
                        } else if srh >= 0.8 {
                            "Mould risk"
                        } else {
                            "OK"
                        };
                        (format!("{:.0} %", (srh * 100.0).min(999.0)), risk.to_string())
                    }
                    _ => ("–".into(), "–".into()),
                };
                Some(vec![m.label.clone(), fmt(s.min), ti, rh, risk])
            })
            .collect()
    } else {
        Vec::new()
    };

    // HVAC: the split between measurements labelled Supply… and Return….
    let hvac = if template.kind == TemplateKind::Hvac {
        let find = |p: &str| {
            item.measurements
                .iter()
                .zip(&stats)
                .find(|(m, s)| m.label.to_lowercase().starts_with(p) && s.is_some())
                .map(|(m, s)| (m.label.clone(), s.unwrap().avg))
        };
        let mut rows = Vec::new();
        if let Some((l, k)) = find("supply") {
            rows.push([format!("Supply ({l}, average)"), fmt(k)]);
        }
        if let Some((l, k)) = find("return") {
            rows.push([format!("Return ({l}, average)"), fmt(k)]);
        }
        if let (Some((_, s)), Some((_, r))) = (find("supply"), find("return")) {
            rows.push(["Split (return − supply)".into(), signed(unit, r - s)]);
        }
        if rows.is_empty() {
            rows.push(["Split".into(), "Label measurements \"Supply\" and \"Return\" to show the split".into()]);
        }
        rows
    } else {
        Vec::new()
    };

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

    let camera = img.camera();
    let gps = camera.latitude.zip(camera.longitude).map(|(la, lo)| format!("{la:.6}, {lo:.6}"));
    let camera_rows: Vec<[String; 2]> = [
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
    .collect();

    let crop = item
        .presentation
        .crop
        .filter(|c| c[2] >= 1.0 && c[3] >= 1.0)
        .map(|c| {
            let x = c[0].clamp(0.0, w as f32 - 1.0);
            let y = c[1].clamp(0.0, h as f32 - 1.0);
            [x, y, c[2].min(w as f32 - x), c[3].min(h as f32 - y)]
        })
        .unwrap_or([0.0, 0.0, w as f32, h as f32]);

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
        electrical,
        building,
        hvac,
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

/// A chart from `(name, points)` series in data units.
fn chart(
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
fn param_rows(used: &Params, file: &Params, editable: &[Param], unit: Unit) -> Vec<[String; 3]> {
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

// ---------------------------------------------------------------------------- PDF

fn pdf_from(p: &Prepared) -> Result<Vec<u8>> {
    let mut inputs = match to_typst(&serde_json::to_value(p).map_err(|e| Error::Report(e.to_string()))?) {
        Value::Dict(d) => d,
        _ => unreachable!("a struct serializes to an object"),
    };
    let images: Array = p
        .images
        .iter()
        .map(|img| {
            let mut v = to_typst(&serde_json::to_value(img).unwrap_or(Json::Null));
            if let Value::Dict(d) = &mut v {
                d.insert("thermal".into(), Value::Bytes(Bytes::new(img.thermal_png.clone())));
                d.insert("bar".into(), Value::Bytes(Bytes::new(img.bar_png.clone())));
                d.insert(
                    "photo".into(),
                    img.photo_jpeg.clone().map(|b| Value::Bytes(Bytes::new(b))).unwrap_or(Value::None),
                );
            }
            v
        })
        .collect();
    inputs.insert("images".into(), Value::Array(images));
    inputs.insert("logo".into(), p.logo.clone().map(|b| Value::Bytes(Bytes::new(b))).unwrap_or(Value::None));

    let engine = TypstEngine::builder().main_file(TEMPLATE).fonts(FONTS).build();
    let doc =
        engine.compile_with_input(inputs).output.map_err(|e| Error::Report(format!("typesetting failed: {e}")))?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|diags| Error::Report(format!("PDF export failed: {diags:?}")))
}

/// serde JSON to a Typst value. Integers stay integers so Typst can use them as counts.
fn to_typst(v: &Json) -> Value {
    match v {
        Json::Null => Value::None,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::Str(s.as_str().into()),
        Json::Array(a) => Value::Array(a.iter().map(to_typst).collect()),
        Json::Object(o) => {
            let mut d = Dict::new();
            for (k, v) in o {
                d.insert(k.as_str().into(), to_typst(v));
            }
            Value::Dict(d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn param_rows_flag_only_changed_values() {
        let file = Params { emissivity: Some(0.95), distance_m: Some(2.0), ..Default::default() };
        let used = Params { emissivity: Some(0.80), ..file };
        let rows = param_rows(&used, &file, &Param::ALL, Unit::Celsius);
        assert_eq!(rows[0], ["Emissivity".to_string(), "0.80".into(), "0.95".into()]);
        assert_eq!(rows[1], ["Distance".to_string(), "2.0 m".into(), String::new()]);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn json_numbers_keep_integer_type() {
        assert_eq!(to_typst(&json!(3)), Value::Int(3));
        assert_eq!(to_typst(&json!(2.5)), Value::Float(2.5));
    }

    #[test]
    fn base64_decoder_keeps_its_leniency() {
        assert_eq!(base64_decode("TWE").unwrap(), b"Ma");
        assert_eq!(base64_decode("TW\nE=\r\n").unwrap(), b"Ma");
        assert_eq!(base64_decode("").unwrap(), b"");
        // Kept from the previous decoder: URL-safe characters and stray `=` are accepted.
        assert_eq!(base64_decode("-_-_").unwrap(), base64_decode("+/+/").unwrap());
        assert_eq!(base64_decode("TWE==").unwrap(), b"Ma");
        assert_eq!(base64_decode("TWFu=").unwrap(), b"Man");
        assert!(base64_decode("TWFuT").is_none());
    }

    #[test]
    fn base64_round_trip() {
        for s in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10kelvin"] {
            assert_eq!(base64_decode(&base64_encode(s)).unwrap(), s);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_decode("data:image/png;base64,TWFu".split_once("base64,").unwrap().1).unwrap(), b"Man");
        assert!(base64_decode("@@@@").is_none());
    }

    #[test]
    fn builtin_templates_are_unique_and_valid() {
        let all = builtin_templates();
        for t in &all {
            assert_eq!(all.iter().filter(|o| o.id == t.id).count(), 1);
            assert!(t.image_sections.iter().all(|s| IMAGE_SECTIONS.contains(&s.as_str())), "{}", t.id);
            assert!(valid_hex(&t.accent).is_some());
        }
        // A template saved by an older version, with fields missing, still loads.
        let t: Template = serde_json::from_str(r#"{"id":"mine","name":"Mine"}"#).unwrap();
        assert_eq!(t.images_per_page, 1);
    }

    #[test]
    fn chart_normalises_and_labels() {
        let c =
            chart("t".into(), "x", "°C", vec![("a".into(), vec![(0.0, 10.0), (10.0, 20.0)])], |x| format!("{x:.0}"));
        let p = &c.series[0].points;
        assert_eq!(p[0][0], 0.0);
        assert_eq!(p[1][0], 1.0);
        assert!(p[0][1] > 0.0 && p[1][1] < 1.0);
        assert_eq!(c.x_ticks.last().unwrap().1, "10");
    }
}

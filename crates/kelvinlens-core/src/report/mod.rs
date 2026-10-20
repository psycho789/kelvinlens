//! Inspection reports from templates, as PDF (typeset with Typst, entirely in memory) or as a
//! self-contained HTML page. No files are read and no network is used; fonts are bundled so the
//! PDF looks the same on every OS.
//!
//! A [`Template`] chooses the sections, their order, branding and the fields asked for. Built-in
//! templates cover general, electrical (NETA severity), building/moisture (thermal index, dew
//! point) and HVAC inspections; users can make unlimited custom ones.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::analysis::{Basis, Finding, SeverityLevel};
use crate::formula::Formula;
use crate::measure::{Measurement, Shape};
#[cfg(test)]
use crate::params::{Param, Params};
use crate::render::Look;
use crate::sidecar::{Annotation, Presentation};
use crate::{Result, ThermalImage, Unit}; // for the unedited mod tests

mod prepare;
mod trades;
mod typst;
use self::typst::pdf_from;
#[cfg(test)]
use self::typst::to_typst;
#[cfg(test)]
use ::typst::foundations::Value;
pub use prepare::base64_encode;
use prepare::prepare;
#[cfg(test)]
use prepare::{base64_decode, chart, param_rows, valid_hex}; // for the unedited mod tests // for the unedited mod tests

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

/// Sections shown for each image, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageSection {
    Isotherms,
    Measurements,
    Formulas,
    Findings,
    Electrical,
    Building,
    Hvac,
    Profiles,
    Fields,
    Notes,
    Parameters,
    Details,
}

/// Reads section names and drops unknown ones, so a template saved by another version still loads.
fn known_sections<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<ImageSection>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Known(ImageSection),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(Vec::<Entry>::deserialize(d)?
        .into_iter()
        .filter_map(|e| match e {
            Entry::Known(s) => Some(s),
            Entry::Unknown(_) => None,
        })
        .collect())
}

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
    #[serde(deserialize_with = "known_sections")]
    pub image_sections: Vec<ImageSection>,
    /// 1 or 2.
    pub images_per_page: u8,
    /// Replaces the standard accuracy note when not empty.
    pub disclaimer: String,
    /// Text in the page footer next to the title.
    pub footer: String,
}

impl Default for Template {
    fn default() -> Self {
        standard()
    }
}

fn with_tail(head: &[ImageSection]) -> Vec<ImageSection> {
    use ImageSection::{Details, Notes, Parameters};
    head.iter().copied().chain([Notes, Parameters, Details]).collect()
}

fn base(id: &str, name: &str, kind: TemplateKind, image_sections: Vec<ImageSection>) -> Template {
    Template {
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
    }
}

fn standard() -> Template {
    use ImageSection::*;
    base(
        "standard",
        "Standard",
        TemplateKind::General,
        with_tail(&[Isotherms, Measurements, Formulas, Findings, Profiles, Fields]),
    )
}

fn electrical() -> Template {
    use ImageSection::*;
    Template {
        cover_fields: vec!["Customer".into(), "Equipment".into()],
        image_fields: vec!["Component".into(), "Phase".into(), "Load %".into()],
        accent: "#b45309".into(),
        ..base(
            "electrical",
            "Electrical (NETA)",
            TemplateKind::Electrical,
            with_tail(&[Fields, Findings, Electrical, Measurements, Formulas, Profiles]),
        )
    }
}

fn building() -> Template {
    use ImageSection::*;
    Template {
        cover_fields: vec!["Customer".into(), "Building".into()],
        image_fields: vec!["Room".into(), "Wall / element".into()],
        accent: "#1d4ed8".into(),
        ..base(
            "building",
            "Building / moisture",
            TemplateKind::Building,
            with_tail(&[Fields, Isotherms, Building, Findings, Measurements, Profiles]),
        )
    }
}

fn hvac() -> Template {
    use ImageSection::*;
    Template {
        cover_fields: vec!["Customer".into(), "System".into()],
        image_fields: vec!["Zone".into(), "Mode (heating/cooling)".into()],
        accent: "#0f766e".into(),
        ..base(
            "hvac",
            "HVAC",
            TemplateKind::Hvac,
            with_tail(&[Fields, Hvac, Measurements, Formulas, Findings, Profiles]),
        )
    }
}

fn compact() -> Template {
    use ImageSection::*;
    Template {
        sections: Sections { summary: true, contents: false, findings_overview: true, trends: false, signature: false },
        images_per_page: 2,
        ..base("compact", "Compact (2 per page)", TemplateKind::General, vec![Measurements, Findings, Notes])
    }
}

/// The templates that ship with Kelvinlens.
pub fn builtin_templates() -> Vec<Template> {
    vec![standard(), electrical(), building(), hvac(), compact()]
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
    pub image_sections: Vec<ImageSection>,
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
    pub level: SeverityLevel,
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
    pub level: SeverityLevel,
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
    fn image_sections_serialise_as_the_template_names() {
        use ImageSection::*;
        let all = [
            Isotherms,
            Measurements,
            Formulas,
            Findings,
            Electrical,
            Building,
            Hvac,
            Profiles,
            Fields,
            Notes,
            Parameters,
            Details,
        ];
        let names: Vec<String> =
            all.iter().map(|s| serde_json::to_value(s).unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(
            names,
            [
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
                "details"
            ]
        );
    }

    #[test]
    fn template_ignores_unknown_image_sections() {
        let t: Template = serde_json::from_str(r#"{"imageSections":["notes","bogus",7,"details"]}"#).unwrap();
        assert_eq!(t.image_sections, [ImageSection::Notes, ImageSection::Details]);
        let t: Template = serde_json::from_str(r#"{"id":"x"}"#).unwrap();
        assert_eq!(t.image_sections, Template::default().image_sections);
    }

    #[test]
    fn builtin_templates_are_unique_and_valid() {
        let all = builtin_templates();
        for t in &all {
            assert_eq!(all.iter().filter(|o| o.id == t.id).count(), 1);
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

#[cfg(test)]
mod template_tests {
    use super::*;

    #[test]
    fn default_template_is_the_first_builtin() {
        assert_eq!(Template::default(), builtin_templates().remove(0));
    }
}

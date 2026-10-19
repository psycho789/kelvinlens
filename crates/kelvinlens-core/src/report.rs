//! PDF inspection reports, typeset with Typst entirely in memory: no files are read and no
//! network is used. Fonts are bundled so the PDF looks the same on every OS.

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use typst::foundations::{Array, Bytes, Dict, Value};
use typst_as_lib::TypstEngine;

use crate::measure::{self, Measurement};
use crate::palette::{self, Palette};
use crate::params::{Param, Params};
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
}

/// One image in the report, with the view and measurements the user set up for it.
pub struct ReportImage<'a> {
    pub image: &'a ThermalImage,
    pub palette: Palette,
    /// Display range in kelvin.
    pub range: [f32; 2],
    pub measurements: Vec<Measurement>,
    pub notes: String,
}

pub fn pdf(info: &ReportInfo, images: &[ReportImage], generator: &str) -> Result<Vec<u8>> {
    let mut items = Vec::with_capacity(images.len());
    for item in images {
        items.push(image_value(item, info)?);
    }

    let mut inputs = match to_typst(&json!({
        "title": if info.title.trim().is_empty() { "Thermal inspection report" } else { info.title.trim() },
        "site": info.site.trim(),
        "inspector": info.inspector.trim(),
        "date": info.date.trim(),
        "summary": info.summary.trim(),
        "paper": match info.paper { Paper::A4 => "a4", Paper::UsLetter => "us-letter" },
        "generator": generator,
        "disclaimer": DISCLAIMER,
    })) {
        Value::Dict(d) => d,
        _ => unreachable!("object converts to a dict"),
    };
    inputs.insert("images".into(), Value::Array(items.into_iter().collect::<Array>()));

    let engine = TypstEngine::builder().main_file(TEMPLATE).fonts(FONTS).build();
    let doc =
        engine.compile_with_input(inputs).output.map_err(|e| Error::Report(format!("typesetting failed: {e}")))?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|diags| Error::Report(format!("PDF export failed: {diags:?}")))
}

const DISCLAIMER: &str = "Temperatures are computed from the camera's radiometric data with the \
parameters listed for each image. Their accuracy depends on the camera's calibration and on \
those parameters matching the real conditions.";

fn image_value(item: &ReportImage, info: &ReportInfo) -> Result<Value> {
    let img = item.image;
    let (w, h) = (img.width(), img.height());
    let unit = info.unit;
    let fmt = |k: f32| format!("{:.1} {}", unit.from_kelvin(k), unit.symbol());

    let thermal = export::png(&palette::render(img.kelvin(), item.range, &item.palette), w, h)?;
    let bar = export::png(&palette::bar(&item.palette, 256), 256, 1)?;
    let photo = match (info.include_photos, img.photo()) {
        (true, Some(p)) => Some(photo_jpeg(p)?),
        _ => None,
    };

    let measurements: Vec<Json> = item
        .measurements
        .iter()
        .map(|m| {
            let stats = measure::stats(img.kelvin(), w, h, &m.shape);
            let cell = |f: fn(&measure::Stats) -> f32| stats.as_ref().map(|s| fmt(f(s))).unwrap_or("–".into());
            json!({
                "label": m.label,
                "shape": m.shape,
                "kind": kind_name(&m.shape),
                "min": cell(|s| s.min),
                "max": cell(|s| s.max),
                "avg": cell(|s| s.avg),
            })
        })
        .collect();

    let camera = img.camera();
    let gps = camera.latitude.zip(camera.longitude).map(|(la, lo)| format!("{la:.6}, {lo:.6}"));
    let camera_rows: Vec<[String; 2]> = [
        ("Camera", camera.description.clone()),
        ("Serial number", camera.serial_number.clone()),
        ("Captured", camera.date_time.clone()),
        ("GPS", gps),
        ("Format", Some(img.format().label().to_string())),
        ("Resolution", Some(format!("{w} × {h}"))),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| [k.to_string(), v]))
    .collect();

    let range = img.range().map(|[lo, hi]| format!("{} to {}", fmt(lo), fmt(hi)));
    let mut value = to_typst(&json!({
        "name": img.file_name(),
        "width": w,
        "height": h,
        "range_min": fmt(item.range[0]),
        "range_max": fmt(item.range[1]),
        "image_range": range,
        "palette": item.palette.name,
        "measurements": measurements,
        "params": param_rows(&img.params(), &img.file_params(), &img.editable(), unit),
        "camera": camera_rows,
        "notes": item.notes.trim(),
    }));
    if let Value::Dict(d) = &mut value {
        d.insert("thermal".into(), Value::Bytes(Bytes::new(thermal)));
        d.insert("bar".into(), Value::Bytes(Bytes::new(bar)));
        d.insert("photo".into(), photo.map(|p| Value::Bytes(Bytes::new(p))).unwrap_or(Value::None));
    }
    Ok(value)
}

fn kind_name(shape: &measure::Shape) -> &'static str {
    match shape {
        measure::Shape::Spot { .. } => "Spot",
        measure::Shape::Box { .. } => "Box",
        measure::Shape::Ellipse { .. } => "Ellipse",
        measure::Shape::Line { .. } => "Line",
    }
}

/// Rows of [name, value used, value in file] for the parameters the format records.
fn param_rows(used: &Params, file: &Params, editable: &[Param], unit: Unit) -> Vec<[String; 3]> {
    let temp = |k: f32| format!("{:.1} {}", unit.from_kelvin(k), unit.symbol());
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
}

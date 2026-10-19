//! An opened thermogram: temperatures, the photo, metadata, and recomputation with new
//! capture parameters.

use std::path::{Path, PathBuf};

use libblackbody::{FlirThermogram, Measurement as FileMeasurement, Thermogram, ThermogramTrait};
use serde::Serialize;
use uom::si::thermodynamic_temperature::kelvin as K;

use crate::measure::{Measurement, Shape};
use crate::params::{Param, Params};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Flir,
    Fluke,
    Hti,
    Irg,
    Png,
    Tiff,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Flir => "FLIR radiometric JPEG",
            Format::Fluke => "Fluke IS2",
            Format::Hti => "HTI / ToolTop",
            Format::Irg => "InfiRay IRG",
            Format::Png => "16-bit PNG (centikelvin)",
            Format::Tiff => "TIFF",
        }
    }
}

/// Camera details recorded in the file.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub description: Option<String>,
    pub serial_number: Option<String>,
    pub date_time: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// An axis-aligned rectangle in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: u32,
    pub h: u32,
}

/// Picture-in-picture geometry: `source` (thermal pixels) lands on `destination` (photo pixels).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Pip {
    pub source: Rect,
    pub destination: Rect,
}

/// RGB photo, 3 bytes per pixel, row-major.
#[derive(Clone, Debug)]
pub struct Photo {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// How temperatures can be recomputed for a format.
#[derive(Clone, Debug)]
enum Model {
    /// FLIR: flyr's full model (atmosphere, IR window, reflections) re-run on the raw counts.
    Flir(Box<FlirThermogram>),
    /// Fluke: serendip's correction `T⁴ = (Tapp⁴ − (1 − ε)·Tbg⁴) / (τ·ε)`, kept as the apparent
    /// `Tapp⁴` per pixel so new parameters can be applied exactly.
    Fluke { apparent4: Vec<f32> },
    /// Formats that only store final temperatures.
    Fixed,
}

#[derive(Clone, Debug)]
pub struct ThermalImage {
    path: PathBuf,
    format: Format,
    width: usize,
    height: usize,
    kelvin: Vec<f32>,
    model: Model,
    file_params: Params,
    params: Params,
    camera: Camera,
    embedded_measurements: Vec<Measurement>,
    embedded_range: Option<[f32; 2]>,
    photo: Option<Photo>,
    pip: Option<Pip>,
}

impl ThermalImage {
    pub fn open(path: &Path) -> Result<ThermalImage> {
        let thermogram = Thermogram::from_file(path).map_err(|e| Error::Open(e.to_string()))?;
        Ok(Self::from_thermogram(path, thermogram))
    }

    fn from_thermogram(path: &Path, t: Thermogram) -> ThermalImage {
        let thermal = t.thermal();
        let (width, height) = (thermal.width(), thermal.height());
        let kelvin: Vec<f32> = thermal.pixels().map(|v| v.get::<K>()).collect();
        let file_params = Params::from(t.capture_parameters());

        let metadata = t.camera_metadata();
        let camera = Camera {
            description: metadata.description(),
            serial_number: metadata.serial_number.clone(),
            date_time: metadata.date_time.as_deref().map(exif_date),
            latitude: metadata.latitude,
            longitude: metadata.longitude,
        };
        let embedded_measurements = t.measurements().iter().filter_map(convert_measurement).collect();
        let embedded_range = t.embedded_render_range().map(|r| [r[0].get::<K>(), r[1].get::<K>()]);
        let photo = t.visual().map(|v| Photo {
            width: v.width(),
            height: v.height(),
            rgb: v.pixels().flat_map(|p| [p.r, p.g, p.b]).collect(),
        });
        let rect = |r: libblackbody::PipRect| Rect { x: r.x, y: r.y, w: r.width, h: r.height };
        let pip = t.pip_geometry().map(|g| Pip { source: rect(g.source), destination: rect(g.destination) });

        let (format, model) = match t {
            Thermogram::Flir(f) => (Format::Flir, Model::Flir(Box::new(f))),
            Thermogram::Fluke(_) => {
                let apparent4 = fluke_apparent4(&kelvin, &file_params);
                (Format::Fluke, Model::Fluke { apparent4 })
            }
            Thermogram::Hti(_) => (Format::Hti, Model::Fixed),
            Thermogram::Irg(_) => (Format::Irg, Model::Fixed),
            Thermogram::Png(_) => (Format::Png, Model::Fixed),
            Thermogram::Tiff(_) => (Format::Tiff, Model::Fixed),
        };

        ThermalImage {
            path: path.to_path_buf(),
            format,
            width,
            height,
            kelvin,
            model,
            file_params,
            params: file_params,
            camera,
            embedded_measurements,
            embedded_range,
            photo,
            pip,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Temperatures in kelvin, row-major. NaN where the parameters have no physical solution.
    pub fn kelvin(&self) -> &[f32] {
        &self.kelvin
    }

    pub fn kelvin_at(&self, x: usize, y: usize) -> Option<f32> {
        (x < self.width && y < self.height).then(|| self.kelvin[y * self.width + x])
    }

    /// Lowest and highest finite temperature, or `None` if there is none.
    pub fn range(&self) -> Option<[f32; 2]> {
        let finite = self.kelvin.iter().copied().filter(|v| v.is_finite());
        finite.fold(None, |acc, v| match acc {
            None => Some([v, v]),
            Some([lo, hi]) => Some([lo.min(v), hi.max(v)]),
        })
    }

    pub fn file_params(&self) -> Params {
        self.file_params
    }

    pub fn params(&self) -> Params {
        self.params
    }

    /// The parameters that can be changed for this format.
    pub fn editable(&self) -> Vec<Param> {
        match self.model {
            Model::Flir(_) => Param::ALL.to_vec(),
            Model::Fluke { .. } => vec![Param::Emissivity, Param::ReflectedK, Param::WindowTransmission],
            Model::Fixed => Vec::new(),
        }
    }

    /// Recomputes every temperature with `params`. Fields this format cannot use are ignored;
    /// fields left `None` keep the file's value.
    pub fn set_params(&mut self, params: &Params) -> Result<()> {
        let editable = self.editable();
        if editable.is_empty() {
            return Err(Error::Params(format!(
                "{} files store final temperatures only; their parameters cannot be changed",
                self.format.label()
            )));
        }
        let merged = params.overlay(&self.file_params, &editable);
        merged.validate()?;

        match &mut self.model {
            Model::Flir(f) => {
                // `merged` holds the file's values for anything not edited; keep flyr's own
                // value only where the file reported none.
                let info = &mut f.thermogram.camera_info;
                let or = |new: Option<f32>, current: f32| new.unwrap_or(current);
                info.emissivity = or(merged.emissivity, info.emissivity);
                info.reflected_apparent_temperature = or(merged.reflected_k, info.reflected_apparent_temperature);
                info.atmospheric_temperature = or(merged.atmosphere_k, info.atmospheric_temperature);
                info.object_distance = or(merged.distance_m, info.object_distance);
                info.relative_humidity = or(merged.humidity, info.relative_humidity);
                info.ir_window_temperature = or(merged.window_k, info.ir_window_temperature);
                info.ir_window_transmission = or(merged.window_transmission, info.ir_window_transmission);
                let kelvin = f.thermogram.kelvin();
                if kelvin.len() != self.width * self.height {
                    return Err(Error::Params("recomputed image has an unexpected size".into()));
                }
                // Non-physical results (negative radiance) come out as NaN or ±inf; keep NaN only.
                self.kelvin = kelvin.into_iter().map(|v| if v.is_finite() { v } else { f32::NAN }).collect();
            }
            Model::Fluke { apparent4 } => {
                self.kelvin = fluke_object_kelvin(apparent4, &merged);
            }
            Model::Fixed => unreachable!("checked above"),
        }
        self.params = merged;
        Ok(())
    }

    /// Restores the temperatures the file was saved with.
    pub fn reset_params(&mut self) -> Result<()> {
        if self.editable().is_empty() {
            return Ok(());
        }
        let file = self.file_params;
        self.set_params(&file)
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Measurements stored in the file by the camera (spots, boxes, lines, axis-aligned ellipses).
    pub fn embedded_measurements(&self) -> &[Measurement] {
        &self.embedded_measurements
    }

    /// The temperature range the camera displayed, if recorded.
    pub fn embedded_range(&self) -> Option<[f32; 2]> {
        self.embedded_range
    }

    pub fn photo(&self) -> Option<&Photo> {
        self.photo.as_ref()
    }

    pub fn pip(&self) -> Option<Pip> {
        self.pip
    }
}

/// EXIF writes dates as `YYYY:MM:DD HH:MM:SS`; show them as `YYYY-MM-DD HH:MM:SS`.
fn exif_date(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 10 && b[4] == b':' && b[7] == b':' {
        format!("{}-{}-{}{}", &s[0..4], &s[5..7], &s[8..10], &s[10..])
    } else {
        s.to_string()
    }
}

/// Fluke files keep transmission implicit at 1.0 when they store none (serendip does the same).
fn fluke_terms(p: &Params) -> (f32, f32, f32) {
    let emissivity = p.emissivity.unwrap_or(1.0);
    let background = p.reflected_k.unwrap_or(293.15);
    let transmission = p.window_transmission.unwrap_or(1.0);
    (emissivity, background, transmission)
}

/// Inverts serendip's correction: `Tapp⁴ = τ·ε·T⁴ + (1 − ε)·Tbg⁴`.
fn fluke_apparent4(kelvin: &[f32], p: &Params) -> Vec<f32> {
    let (e, bg, tau) = fluke_terms(p);
    kelvin.iter().map(|t| tau * e * t.powi(4) + (1.0 - e) * bg.powi(4)).collect()
}

fn fluke_object_kelvin(apparent4: &[f32], p: &Params) -> Vec<f32> {
    let (e, bg, tau) = fluke_terms(p);
    apparent4
        .iter()
        .map(|a| {
            let t4 = (a - (1.0 - e) * bg.powi(4)) / (tau * e);
            if t4 > 0.0 { t4.powf(0.25) } else { f32::NAN }
        })
        .collect()
}

/// File measurements use pixel indices; ours use continuous coordinates where pixel `i`
/// spans `i..i+1`, so points move to pixel centres.
fn convert_measurement(m: &FileMeasurement) -> Option<Measurement> {
    let c = |v: u32| v as f32 + 0.5;
    let (label, shape) = match m {
        FileMeasurement::Spot { label, x, y } => (label, Shape::Spot { x: c(*x), y: c(*y) }),
        FileMeasurement::Area { label, x, y, width, height } => {
            (label, Shape::Box { x: *x as f32, y: *y as f32, w: *width as f32, h: *height as f32 })
        }
        FileMeasurement::Line { label, x1, y1, x2, y2 } => {
            (label, Shape::Line { x1: c(*x1), y1: c(*y1), x2: c(*x2), y2: c(*y2) })
        }
        // Centre, then the end points of the two semi-axes. Only axis-aligned ones map to our shape.
        FileMeasurement::Ellipse { label, params } if params.len() >= 6 => {
            let (xc, yc) = (params[0] as f32, params[1] as f32);
            let (ux, uy, vx, vy) =
                (params[2] as f32 - xc, params[3] as f32 - yc, params[4] as f32 - xc, params[5] as f32 - yc);
            let (rx, ry) = match (uy == 0.0 && vx == 0.0, ux == 0.0 && vy == 0.0) {
                (true, _) => (ux.abs(), vy.abs()),
                (_, true) => (vx.abs(), uy.abs()),
                _ => return None,
            };
            (label, Shape::Ellipse { cx: xc + 0.5, cy: yc + 0.5, rx, ry })
        }
        _ => return None,
    };
    Some(Measurement { label: label.trim().to_string(), shape })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluke_inversion_round_trips_and_matches_forward_formula() {
        let file = Params { emissivity: Some(0.95), reflected_k: Some(293.15), ..Default::default() };
        let kelvin = vec![250.0, 300.0, 350.0];
        let apparent4 = fluke_apparent4(&kelvin, &file);
        let same = fluke_object_kelvin(&apparent4, &file);
        for (a, b) in kelvin.iter().zip(&same) {
            assert!((a - b).abs() < 0.01, "{a} vs {b}");
        }

        // serendip's forward formula applied to the apparent temperature with new parameters.
        let new = Params { emissivity: Some(0.7), reflected_k: Some(310.0), window_transmission: Some(0.9), ..file };
        let changed = fluke_object_kelvin(&apparent4, &new);
        for (a4, t) in apparent4.iter().zip(&changed) {
            let forward = ((a4 - (1.0 - 0.7) * 310f32.powi(4)) / (0.9 * 0.7)).powf(0.25);
            assert!((forward - t).abs() < 0.01);
        }
    }

    #[test]
    fn fluke_non_physical_is_nan() {
        let p = Params { emissivity: Some(0.1), reflected_k: Some(400.0), ..Default::default() };
        let out = fluke_object_kelvin(&[200f32.powi(4)], &p);
        assert!(out[0].is_nan());
    }

    #[test]
    fn exif_dates_become_iso_like() {
        assert_eq!(exif_date("2013:04:12 09:24:01"), "2013-04-12 09:24:01");
        assert_eq!(exif_date("2013:04:12 09:24:01.123+02:00"), "2013-04-12 09:24:01.123+02:00");
        assert_eq!(exif_date("unknown"), "unknown");
    }

    #[test]
    fn converts_file_measurements_to_pixel_centres() {
        let spot = FileMeasurement::Spot { label: " Sp1 ".into(), x: 80, y: 60 };
        let m = convert_measurement(&spot).unwrap();
        assert_eq!(m.label, "Sp1");
        assert_eq!(m.shape, Shape::Spot { x: 80.5, y: 60.5 });

        let ellipse = FileMeasurement::Ellipse { label: "El1".into(), params: vec![50, 40, 60, 40, 50, 45] };
        assert_eq!(
            convert_measurement(&ellipse).unwrap().shape,
            Shape::Ellipse { cx: 50.5, cy: 40.5, rx: 10.0, ry: 5.0 }
        );
        let rotated = FileMeasurement::Ellipse { label: "El2".into(), params: vec![50, 40, 57, 47, 46, 44] };
        assert!(convert_measurement(&rotated).is_none());
    }
}

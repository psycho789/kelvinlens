//! An opened thermogram: temperatures, the photo, metadata, and recomputation with new
//! capture parameters.

use std::path::{Path, PathBuf};

use libblackbody::{FlirThermogram, Measurement as FileMeasurement, Thermogram, ThermogramTrait};
use serde::Serialize;
use uom::si::thermodynamic_temperature::kelvin as K;

use crate::fff;
use crate::infiray;
use crate::measure::{Measurement, Shape};
use crate::palette;
use crate::params::{Param, Params};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Flir,
    Fluke,
    Hti,
    Irg,
    /// InfiRay IJPEG, as saved by the P2 Pro app.
    InfiRay,
    Png,
    Tiff,
    /// A frame of a FLIR SEQ or CSQ radiometric video.
    Video,
    /// Stitched from several images in Kelvinlens.
    Panorama,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Flir => "FLIR radiometric JPEG",
            Format::Fluke => "Fluke IS2",
            Format::Hti => "HTI / ToolTop",
            Format::Irg => "InfiRay IRG",
            Format::InfiRay => "InfiRay P2 Pro JPEG",
            Format::Png => "16-bit PNG (centikelvin)",
            Format::Tiff => "TIFF",
            Format::Video => "FLIR radiometric video",
            Format::Panorama => "Panorama",
        }
    }
}

/// Camera details recorded in the file.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub description: Option<String>,
    pub serial_number: Option<String>,
    pub date_time: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Metres above sea level.
    pub altitude: Option<f32>,
    /// Compass direction the camera faced, in degrees.
    pub heading: Option<f32>,
    pub lens: Option<String>,
    /// Horizontal field of view in degrees.
    pub field_of_view: Option<f32>,
    pub focal_length: Option<f32>,
    /// Drone gimbal roll, pitch and yaw in degrees (DJI XMP).
    pub gimbal: Option<[f32; 3]>,
    /// Drone height above the take-off point in metres (DJI XMP).
    pub relative_altitude: Option<f32>,
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
    /// Fluke and InfiRay P2 Pro: serendip's correction `T⁴ = (Tapp⁴ − (1 − ε)·Tbg⁴) / (τ·ε)`, kept
    /// as the apparent `Tapp⁴` per pixel so new parameters can be applied exactly. Fluke stores
    /// the window transmission; for InfiRay the camera's own model is unknown, so the correction
    /// is an approximation without it. `editable` lists what each format lets the user change.
    T4Corrected { apparent4: Vec<f32>, editable: &'static [Param] },
    /// Formats that only store final temperatures.
    Fixed,
}

/// Parameters a Fluke file lets the user change (it stores the window transmission).
const FLUKE_EDITABLE: &[Param] = &[Param::Emissivity, Param::ReflectedK, Param::WindowTransmission];
/// Parameters an InfiRay file lets the user change.
const INFIRAY_EDITABLE: &[Param] = &[Param::Emissivity, Param::ReflectedK];

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
    embedded_palette: Option<Vec<[u8; 3]>>,
    photo: Option<Photo>,
    pip: Option<Pip>,
}

impl ThermalImage {
    pub fn open(path: &Path) -> Result<ThermalImage> {
        Self::open_with(path, true)
    }

    /// Temperatures and parameters only, without decoding the photo: for scanning many files.
    pub fn open_temperatures(path: &Path) -> Result<ThermalImage> {
        Self::open_with(path, false)
    }

    /// Reads `path`; `full` also decodes the photo and the FLIR lens, serial and drone details.
    fn open_with(path: &Path, full: bool) -> Result<ThermalImage> {
        let bytes = std::fs::read(path).map_err(|e| Error::Open(e.to_string()))?;
        if infiray::is_ijpeg(&bytes) {
            return Self::from_infiray(path, &bytes);
        }
        let thermogram = Thermogram::from_bytes(&bytes).map_err(|e| Error::Open(e.to_string()))?;
        let mut image = Self::build(path, thermogram, full);
        if full && image.format == Format::Flir {
            image.add_flir_details(&bytes);
        }
        Ok(image)
    }

    /// An image with only temperatures: no camera details, extras or photo, and parameters that
    /// cannot change. Each constructor overrides the fields its format provides.
    fn bare(path: &Path, format: Format, width: usize, height: usize, kelvin: Vec<f32>) -> ThermalImage {
        ThermalImage {
            path: path.to_path_buf(),
            format,
            width,
            height,
            kelvin,
            model: Model::Fixed,
            file_params: Params::default(),
            params: Params::default(),
            camera: Camera::default(),
            embedded_measurements: Vec::new(),
            embedded_range: None,
            embedded_palette: None,
            photo: None,
            pip: None,
        }
    }

    /// An InfiRay IJPEG (P2 Pro). It has no visible-light photo: the saved picture is the
    /// camera's own palette rendering, so it is used only to orient the temperatures.
    fn from_infiray(path: &Path, bytes: &[u8]) -> Result<ThermalImage> {
        let j = infiray::decode(bytes).ok_or_else(|| Error::Open("unreadable InfiRay thermal data".into()))?;
        let description = match (&j.make, &j.model) {
            (Some(make), Some(model)) => Some(format!("{make} {model}")),
            (make, model) => make.clone().or_else(|| model.clone()),
        };
        let camera = Camera {
            description,
            serial_number: j.serial_number.clone(),
            date_time: j.date_time.as_deref().map(exif_date),
            focal_length: j.focal_length,
            ..Default::default()
        };
        let mut embedded_measurements = Vec::new();
        if j.show_centre {
            // The camera reads the pixel just up and left of the middle (matches both samples).
            let shape = Shape::Spot { x: j.width as f32 / 2.0 - 0.5, y: j.height as f32 / 2.0 - 0.5 };
            embedded_measurements.push(Measurement::new("Centre", shape));
        }
        let embedded_palette = (j.palette == Some(3)).then(|| palette::by_id("iron")).flatten().map(|p| p.colors);
        let apparent4 = t4_apparent4(&j.kelvin, &j.params);
        Ok(ThermalImage {
            model: Model::T4Corrected { apparent4, editable: INFIRAY_EDITABLE },
            file_params: j.params,
            params: j.params,
            camera,
            embedded_measurements,
            embedded_palette,
            ..Self::bare(path, Format::InfiRay, j.width, j.height, j.kelvin)
        })
    }

    /// One frame of a FLIR video: an FFF block with uncompressed raw data.
    pub fn from_fff_frame(path: &Path, fff_block: &[u8]) -> Result<ThermalImage> {
        let flir = FlirThermogram::from_bytes(fff_block).ok_or_else(|| Error::Open("unreadable video frame".into()))?;
        let mut image = Self::build(path, Thermogram::Flir(flir), true);
        image.format = Format::Video;
        if let Some(x) = fff::camera_extras(fff_block) {
            image.apply_extras(&x);
        }
        Ok(image)
    }

    /// An image made from temperatures alone (a panorama), whose parameters cannot change.
    pub fn from_kelvin(
        path: &Path,
        format: Format,
        kelvin: Vec<f32>,
        width: usize,
        height: usize,
        camera: Camera,
    ) -> Result<ThermalImage> {
        if kelvin.len() != width * height || width == 0 || height == 0 {
            return Err(Error::Open("temperature data does not match the image size".into()));
        }
        Ok(ThermalImage { camera, ..Self::bare(path, format, width, height, kelvin) })
    }

    /// Lens, field of view and serial from the FLIR record, and drone attitude from DJI XMP.
    fn add_flir_details(&mut self, bytes: &[u8]) {
        if let Some((block, _)) = fff::from_jpeg(bytes)
            && let Some(x) = fff::camera_extras(&block)
        {
            self.apply_extras(&x);
        }
        let xmp = |name: &str| xmp_number(bytes, name);
        if let (Some(r), Some(p), Some(y)) = (xmp("GimbalRollDegree"), xmp("GimbalPitchDegree"), xmp("GimbalYawDegree"))
        {
            self.camera.gimbal = Some([r, p, y]);
        }
        self.camera.relative_altitude = xmp("RelativeAltitude");
    }

    fn apply_extras(&mut self, x: &fff::CameraExtras) {
        let c = &mut self.camera;
        if c.description.is_none() {
            c.description = x.model.clone();
        }
        c.serial_number = c.serial_number.clone().or_else(|| x.serial.clone());
        c.lens = c.lens.clone().or_else(|| x.lens.clone());
        c.field_of_view = c.field_of_view.or(x.field_of_view);
        if c.date_time.is_none()
            && let Some((secs, tz)) = x.time
        {
            c.date_time = Some(format_time(secs, tz));
        }
    }

    fn build(path: &Path, t: Thermogram, with_photo: bool) -> ThermalImage {
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
            altitude: metadata.altitude,
            heading: metadata.heading,
            focal_length: metadata.focal_length,
            ..Default::default()
        };
        let embedded_palette = t.palette().filter(|p| p.len() >= 2).map(|p| {
            palette::resample(
                &p.iter().map(|c| c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)).collect::<Vec<_>>(),
            )
        });
        let embedded_measurements = t.measurements().iter().filter_map(convert_measurement).collect();
        let embedded_range = t.embedded_render_range().map(|r| [r[0].get::<K>(), r[1].get::<K>()]);
        let visual = if with_photo { t.visual() } else { None };
        let photo = visual.map(|v| Photo {
            width: v.width(),
            height: v.height(),
            rgb: v.pixels().flat_map(|p| [p.r, p.g, p.b]).collect(),
        });
        let rect = |r: libblackbody::PipRect| Rect { x: r.x, y: r.y, w: r.width, h: r.height };
        let pip = t.pip_geometry().map(|g| Pip { source: rect(g.source), destination: rect(g.destination) });

        let (format, model) = match t {
            Thermogram::Flir(f) => (Format::Flir, Model::Flir(Box::new(f))),
            Thermogram::Fluke(_) => {
                let apparent4 = t4_apparent4(&kelvin, &file_params);
                (Format::Fluke, Model::T4Corrected { apparent4, editable: FLUKE_EDITABLE })
            }
            Thermogram::Hti(_) => (Format::Hti, Model::Fixed),
            Thermogram::Irg(_) => (Format::Irg, Model::Fixed),
            Thermogram::Png(_) => (Format::Png, Model::Fixed),
            Thermogram::Tiff(_) => (Format::Tiff, Model::Fixed),
        };

        ThermalImage {
            model,
            file_params,
            params: file_params,
            camera,
            embedded_measurements,
            embedded_range,
            embedded_palette,
            photo,
            pip,
            ..Self::bare(path, format, width, height, kelvin)
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
            Model::T4Corrected { editable, .. } => editable.to_vec(),
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
            Model::T4Corrected { apparent4, .. } => {
                self.kelvin = t4_object_kelvin(apparent4, &merged);
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

    /// The palette the camera rendered the image with, as 256 colours.
    pub fn embedded_palette(&self) -> Option<&[[u8; 3]]> {
        self.embedded_palette.as_deref()
    }

    /// Whether the temperatures come from a FLIR raw-data model (and can be saved back into a
    /// FLIR file with new parameters).
    pub fn is_flir_jpeg(&self) -> bool {
        self.format == Format::Flir
    }
}

/// A number attribute or element from DJI XMP, such as `drone-dji:GimbalYawDegree="+82.40"`.
fn xmp_number(bytes: &[u8], name: &str) -> Option<f32> {
    let key = format!("drone-dji:{name}");
    let at = bytes.windows(key.len()).position(|w| w == key.as_bytes())? + key.len();
    let rest = &bytes[at..(at + 40).min(bytes.len())];
    let start = rest.iter().position(|&c| c == b'"' || c == b'>')? + 1;
    let end = rest[start..].iter().position(|&c| c == b'"' || c == b'<')? + start;
    std::str::from_utf8(&rest[start..end]).ok()?.trim().parse().ok()
}

/// `YYYY-MM-DD HH:MM:SS.mmm±hh:mm` local time from Unix seconds and a FLIR time-zone field
/// (minutes west of UTC, as ExifTool reads it).
pub fn format_time(secs: f64, tz_minutes: i16) -> String {
    let local = secs - tz_minutes as f64 * 60.0;
    let days = (local / 86_400.0).floor() as i64;
    let rem = local - days as f64 * 86_400.0;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let ms = (rem * 1000.0).round() as i64;
    let (h, mi, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let off = -(tz_minutes as i32);
    let sign = if off < 0 { '-' } else { '+' };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}.{ms:03}{sign}{:02}:{:02}", off.abs() / 60, off.abs() % 60)
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

/// Fluke and InfiRay files keep transmission implicit at 1.0 when they store none (serendip does
/// the same).
fn t4_terms(p: &Params) -> (f32, f32, f32) {
    let emissivity = p.emissivity.unwrap_or(1.0);
    let background = p.reflected_k.unwrap_or(293.15);
    let transmission = p.window_transmission.unwrap_or(1.0);
    (emissivity, background, transmission)
}

/// Inverts serendip's correction: `Tapp⁴ = τ·ε·T⁴ + (1 − ε)·Tbg⁴`.
fn t4_apparent4(kelvin: &[f32], p: &Params) -> Vec<f32> {
    let (e, bg, tau) = t4_terms(p);
    kelvin.iter().map(|t| tau * e * t.powi(4) + (1.0 - e) * bg.powi(4)).collect()
}

fn t4_object_kelvin(apparent4: &[f32], p: &Params) -> Vec<f32> {
    let (e, bg, tau) = t4_terms(p);
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
    Some(Measurement::new(label.trim(), shape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t4_inversion_round_trips_and_matches_forward_formula() {
        let file = Params { emissivity: Some(0.95), reflected_k: Some(293.15), ..Default::default() };
        let kelvin = vec![250.0, 300.0, 350.0];
        let apparent4 = t4_apparent4(&kelvin, &file);
        let same = t4_object_kelvin(&apparent4, &file);
        for (a, b) in kelvin.iter().zip(&same) {
            assert!((a - b).abs() < 0.01, "{a} vs {b}");
        }

        // serendip's forward formula applied to the apparent temperature with new parameters.
        let new = Params { emissivity: Some(0.7), reflected_k: Some(310.0), window_transmission: Some(0.9), ..file };
        let changed = t4_object_kelvin(&apparent4, &new);
        for (a4, t) in apparent4.iter().zip(&changed) {
            let forward = ((a4 - (1.0 - 0.7) * 310f32.powi(4)) / (0.9 * 0.7)).powf(0.25);
            assert!((forward - t).abs() < 0.01);
        }
    }

    #[test]
    fn t4_non_physical_is_nan() {
        let p = Params { emissivity: Some(0.1), reflected_k: Some(400.0), ..Default::default() };
        let out = t4_object_kelvin(&[200f32.powi(4)], &p);
        assert!(out[0].is_nan());
    }

    #[test]
    fn t4_models_expose_their_own_editable_params() {
        let params = Params { emissivity: Some(0.95), reflected_k: Some(293.15), ..Default::default() };
        let kelvin = vec![300.0f32; 4];
        let make = |format, editable| ThermalImage {
            model: Model::T4Corrected { apparent4: t4_apparent4(&kelvin, &params), editable },
            file_params: params,
            params,
            ..ThermalImage::bare(Path::new("x.jpg"), format, 2, 2, kelvin.clone())
        };
        let mut fluke = make(Format::Fluke, FLUKE_EDITABLE);
        let mut infiray = make(Format::InfiRay, INFIRAY_EDITABLE);
        assert_eq!(fluke.editable(), vec![Param::Emissivity, Param::ReflectedK, Param::WindowTransmission]);
        assert_eq!(infiray.editable(), vec![Param::Emissivity, Param::ReflectedK]);
        // Same data and the same correction: an emissivity change gives identical temperatures.
        let new = Params { emissivity: Some(0.7), ..Default::default() };
        fluke.set_params(&new).unwrap();
        infiray.set_params(&new).unwrap();
        assert_eq!(fluke.kelvin(), infiray.kelvin());
        assert!(fluke.kelvin()[0] > 300.0);
    }

    #[test]
    fn flir_time_matches_exiftool() {
        // ExifTool shows the SampleSEQ frame time as 2012:06:13 14:52:08.699-05:00.
        let secs = 1_339_617_128.699;
        assert_eq!(format_time(secs, 300), "2012-06-13 14:52:08.699-05:00");
        assert_eq!(format_time(0.0, 0), "1970-01-01 00:00:00.000+00:00");
        assert_eq!(format_time(951_782_400.0, -60), "2000-02-29 01:00:00.000+01:00");
    }

    #[test]
    fn reads_dji_xmp_numbers() {
        let xmp = br#"<x drone-dji:GimbalYawDegree="+82.40" drone-dji:GimbalRollDegree="-0.5"><drone-dji:RelativeAltitude>1.90</drone-dji:RelativeAltitude>"#;
        assert_eq!(xmp_number(xmp, "GimbalYawDegree"), Some(82.4));
        assert_eq!(xmp_number(xmp, "GimbalRollDegree"), Some(-0.5));
        assert_eq!(xmp_number(xmp, "RelativeAltitude"), Some(1.9));
        assert_eq!(xmp_number(xmp, "GimbalPitchDegree"), None);
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

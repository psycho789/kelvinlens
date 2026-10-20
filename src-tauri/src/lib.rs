//! Tauri shell: keeps opened images and videos in memory and exposes the core library to the UI.
//! The app has no network access; files are only read or written at paths the user picked.

mod cmd_error;
#[cfg(debug_assertions)]
mod dev;
#[cfg(test)]
mod wire_tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kelvinlens_core::analysis::{self, Finding, InspectSettings, Isotherm};
use kelvinlens_core::formula::{self, Formula, FormulaResult};
use kelvinlens_core::measure::ProfilePoint;
use kelvinlens_core::render::{self, FuseRequest, Fusion, Look};
use kelvinlens_core::report::{self, ReportImage, ReportInfo, Template};
use kelvinlens_core::sidecar::{self, Annotation, Presentation, Sidecar};
use kelvinlens_core::thermogram::{Camera, Pip};
use kelvinlens_core::video::{self, Video, VideoInfo};
use kelvinlens_core::{
    Format, Measurement, Param, Params, Shape, Stats, ThermalImage, Unit, avi, export, fff, folder, geo, measure,
    palette, panorama,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Response;

use crate::cmd_error::CmdError;

#[derive(Default)]
struct Store {
    next_id: u32,
    images: HashMap<u32, ThermalImage>,
    videos: HashMap<u32, Video>,
    /// Bytes sent by the UI ahead of `write_staged` (exported images).
    staged: Option<Vec<u8>>,
    /// JPEG frames of a video export in progress.
    avi_frames: Vec<Vec<u8>>,
    /// Recently decoded video frames (video id, frame, parameters) for stepping and the
    /// high-sensitivity view, which needs the frames before the current one.
    frames: FrameCache,
}

#[derive(Default)]
struct FrameCache {
    entries: std::collections::VecDeque<(u32, usize, Option<Params>, std::sync::Arc<ThermalImage>)>,
    bytes: usize,
}

impl FrameCache {
    /// About 256 MB of temperatures.
    const LIMIT: usize = 256 * 1024 * 1024;

    fn get(&self, id: u32, index: usize, params: &Option<Params>) -> Option<std::sync::Arc<ThermalImage>> {
        self.entries.iter().find(|(i, f, p, _)| *i == id && *f == index && p == params).map(|e| e.3.clone())
    }

    fn put(&mut self, id: u32, index: usize, params: Option<Params>, image: std::sync::Arc<ThermalImage>) {
        self.bytes += image.kelvin().len() * 4;
        self.entries.push_back((id, index, params, image));
        while self.bytes > Self::LIMIT && self.entries.len() > 1 {
            if let Some((_, _, _, old)) = self.entries.pop_front() {
                self.bytes -= old.kelvin().len() * 4;
            }
        }
    }

    fn forget(&mut self, id: u32) {
        self.entries.retain(|e| e.0 != id);
        self.bytes = self.entries.iter().map(|e| e.3.kelvin().len() * 4).sum();
    }
}

/// Frame `index` of video `id` with `params`, from the cache or decoded (and cached).
fn cached_frame(
    state: &AppState,
    id: u32,
    video: &Video,
    index: usize,
    params: &Option<Params>,
) -> CmdResult<std::sync::Arc<ThermalImage>> {
    if let Some(f) = state.lock()?.frames.get(id, index, params) {
        return Ok(f);
    }
    let frame = std::sync::Arc::new(video.frame(index, params.as_ref())?);
    state.lock()?.frames.put(id, index, *params, frame.clone());
    Ok(frame)
}

type AppState = Mutex<Store>;
type CmdResult<T> = Result<T, CmdError>;

fn with_image<T>(state: &AppState, id: u32, f: impl FnOnce(&mut ThermalImage) -> CmdResult<T>) -> CmdResult<T> {
    let mut store = state.lock()?;
    let image = store.images.get_mut(&id).ok_or_else(|| CmdError::not_found("This image is no longer open"))?;
    f(image)
}

/// Everything the UI needs about an image except the pixel data.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageInfo {
    id: u32,
    path: String,
    name: String,
    format: Format,
    format_label: &'static str,
    width: usize,
    height: usize,
    params: Params,
    file_params: Params,
    editable: Vec<Param>,
    camera: Camera,
    embedded_measurements: Vec<Measurement>,
    embedded_range: Option<[f32; 2]>,
    embedded_palette: Option<Vec<[u8; 3]>>,
    range: Option<[f32; 2]>,
    photo_size: Option<[usize; 2]>,
    pip: Option<Pip>,
    /// Whether "Save FLIR copy with these parameters" works for this file.
    flir_jpeg: bool,
    video: Option<VideoInfo>,
    sidecar: Option<Sidecar>,
    sidecar_error: Option<String>,
}

fn info(id: u32, img: &ThermalImage, video: Option<&Video>, unsaved: bool) -> ImageInfo {
    let (sidecar, sidecar_error) = if unsaved {
        (None, None)
    } else {
        match sidecar::load(img.path()) {
            Ok(s) => (s, None),
            Err(e) => (None, Some(e.to_string())),
        }
    };
    ImageInfo {
        id,
        path: img.path().display().to_string(),
        name: img.file_name(),
        format: img.format(),
        format_label: img.format().label(),
        width: img.width(),
        height: img.height(),
        params: img.params(),
        file_params: img.file_params(),
        editable: img.editable(),
        camera: img.camera().clone(),
        embedded_measurements: img.embedded_measurements().to_vec(),
        embedded_range: img.embedded_range(),
        embedded_palette: img.embedded_palette().map(<[_]>::to_vec),
        range: img.range(),
        photo_size: img.photo().map(|p| [p.width, p.height]),
        pip: img.pip(),
        flir_jpeg: img.is_flir_jpeg(),
        video: video.map(Video::info),
        sidecar,
        sidecar_error,
    }
}

#[tauri::command]
async fn open_image(path: String, state: State<'_, AppState>) -> CmdResult<ImageInfo> {
    let p = Path::new(&path);
    let (image, video) = if video::is_video(p) {
        let v = Video::open(p)?;
        (v.frame(0, None)?, Some(v))
    } else {
        (ThermalImage::open(p)?, None)
    };
    let mut store = state.lock()?;
    store.next_id += 1;
    let id = store.next_id;
    let out = info(id, &image, video.as_ref(), false);
    store.images.insert(id, image);
    if let Some(v) = video {
        store.videos.insert(id, v);
    }
    Ok(out)
}

#[tauri::command]
async fn close_image(id: u32, state: State<'_, AppState>) -> CmdResult<()> {
    let mut store = state.lock()?;
    store.images.remove(&id);
    store.videos.remove(&id);
    store.frames.forget(id);
    Ok(())
}

/// Temperatures in kelvin as little-endian f32, row-major.
#[tauri::command]
async fn thermal_data(id: u32, state: State<'_, AppState>) -> CmdResult<Response> {
    with_image(&state, id, |img| {
        let bytes: Vec<u8> = img.kelvin().iter().flat_map(|v| v.to_le_bytes()).collect();
        Ok(Response::new(bytes))
    })
}

#[tauri::command]
async fn photo_jpeg(id: u32, state: State<'_, AppState>) -> CmdResult<Response> {
    let photo = with_image(&state, id, |img| img.photo().cloned().ok_or_else(|| CmdError::not_found("No photo")))?;
    let jpeg = export::jpeg(&photo.rgb, photo.width, photo.height, 92)?;
    Ok(Response::new(jpeg))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Recomputed {
    params: Params,
    range: Option<[f32; 2]>,
}

#[tauri::command]
async fn set_params(id: u32, params: Params, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    with_image(&state, id, |img| {
        img.set_params(&params)?;
        Ok(Recomputed { params: img.params(), range: img.range() })
    })
}

#[tauri::command]
async fn reset_params(id: u32, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    with_image(&state, id, |img| {
        img.reset_params()?;
        Ok(Recomputed { params: img.params(), range: img.range() })
    })
}

#[tauri::command]
async fn measure(id: u32, shapes: Vec<Shape>, state: State<'_, AppState>) -> CmdResult<Vec<Option<Stats>>> {
    with_image(&state, id, |img| {
        Ok(shapes.iter().map(|s| measure::stats(img.kelvin(), img.width(), img.height(), s)).collect())
    })
}

/// Outline loops of magic-wand areas (empty for other tools), in thermal pixels.
#[tauri::command]
async fn outlines(id: u32, shapes: Vec<Shape>, state: State<'_, AppState>) -> CmdResult<Vec<Vec<Vec<[f32; 2]>>>> {
    with_image(&state, id, |img| {
        let (k, w, h) = (img.kelvin(), img.width(), img.height());
        Ok(shapes
            .iter()
            .map(|s| match s {
                Shape::Wand { .. } => measure::outline(&measure::pixels(s, k, w, h), w, h),
                _ => Vec::new(),
            })
            .collect())
    })
}

#[tauri::command]
async fn profile(id: u32, shape: Shape, state: State<'_, AppState>) -> CmdResult<Vec<ProfilePoint>> {
    with_image(&state, id, |img| Ok(measure::profile(img.kelvin(), img.width(), img.height(), &shape)))
}

/// Share of each area shape's pixels matched by any of the isotherms (`None` for non-areas).
#[tauri::command]
async fn coverage(
    id: u32,
    shapes: Vec<Shape>,
    isotherms: Vec<Isotherm>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<Option<f32>>> {
    with_image(&state, id, |img| {
        let any = analysis::any_matcher(&isotherms);
        Ok(shapes
            .iter()
            .map(|s| {
                let any = any.as_ref().filter(|_| s.is_area())?;
                measure::coverage(img.kelvin(), img.width(), img.height(), s, any)
            })
            .collect())
    })
}

#[tauri::command]
async fn evaluate_formulas(
    id: u32,
    measurements: Vec<Measurement>,
    formulas: Vec<Formula>,
    unit: Unit,
    state: State<'_, AppState>,
) -> CmdResult<Vec<FormulaResult>> {
    with_image(&state, id, |img| {
        let stats: Vec<Option<Stats>> =
            measurements.iter().map(|m| measure::stats(img.kelvin(), img.width(), img.height(), &m.shape)).collect();
        Ok(formula::evaluate_measured(&formulas, &measurements, &stats, unit))
    })
}

/// The thermal image coloured with `look`, as RGBA (4 bytes per pixel).
#[tauri::command]
async fn render(id: u32, look: Look, state: State<'_, AppState>) -> CmdResult<Response> {
    let look = look.normalized();
    with_image(&state, id, |img| {
        Ok(Response::new(render::rgba(&render::rgb(img.kelvin(), img.width(), img.height(), &look))))
    })
}

/// A thermal/photo composite in the wire layout of `render::Composite::to_wire`.
#[tauri::command]
async fn fuse(
    id: u32,
    look: Look,
    fusion: Fusion,
    max_width: usize,
    align: Option<[f32; 3]>,
    state: State<'_, AppState>,
) -> CmdResult<Response> {
    let look = look.normalized();
    with_image(&state, id, |img| {
        let photo = img.photo().ok_or_else(|| CmdError::not_found("This image has no photo"))?;
        let c = render::fuse(&FuseRequest {
            kelvin: img.kelvin(),
            width: img.width(),
            height: img.height(),
            look: &look,
            photo,
            pip: img.pip(),
            fusion,
            max_width: max_width.clamp(64, 4096),
            align: align.unwrap_or(render::NO_ALIGNMENT),
        });
        Ok(Response::new(c.to_wire()))
    })
}

#[tauri::command]
async fn inspect(id: u32, settings: InspectSettings, state: State<'_, AppState>) -> CmdResult<Vec<Finding>> {
    with_image(&state, id, |img| Ok(analysis::inspect(img.kelvin(), img.width(), img.height(), &settings)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TriageSettings {
    /// Flag images hotter than this (kelvin) …
    above_k: Option<f32>,
    /// … or colder than this.
    below_k: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TriageRow {
    path: String,
    name: String,
    error: Option<String>,
    min_k: Option<f32>,
    max_k: Option<f32>,
    flagged: bool,
    date_time: Option<String>,
}

/// Opens each file (videos: the first frame) and reports its hottest and coldest temperature.
#[tauri::command]
async fn triage(paths: Vec<String>, settings: TriageSettings) -> CmdResult<Vec<TriageRow>> {
    let mut files = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if path.is_dir() {
            files.extend(folder::candidates(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files
        .into_iter()
        .map(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // As the image would look when opened: the saved parameter edits applied.
            let saved = sidecar::load(&path).ok().flatten().and_then(|s| s.params);
            let opened = if video::is_video(&path) {
                Video::open(&path).and_then(|v| v.frame(0, saved.as_ref()))
            } else {
                ThermalImage::open_temperatures(&path).and_then(|mut img| {
                    if let Some(p) = &saved
                        && !img.editable().is_empty()
                    {
                        img.set_params(p)?;
                    }
                    Ok(img)
                })
            };
            match opened {
                Ok(img) => {
                    let r = img.range();
                    let flagged = r.is_some_and(|[lo, hi]| {
                        settings.above_k.is_some_and(|a| hi > a) || settings.below_k.is_some_and(|b| lo < b)
                    });
                    TriageRow {
                        path: path.display().to_string(),
                        name,
                        error: None,
                        min_k: r.map(|r| r[0]),
                        max_k: r.map(|r| r[1]),
                        flagged,
                        date_time: img.camera().date_time.clone(),
                    }
                }
                Err(e) => TriageRow {
                    path: path.display().to_string(),
                    name,
                    error: Some(e.to_string().replace("Could not open thermogram: ", "")),
                    min_k: None,
                    max_k: None,
                    flagged: false,
                    date_time: None,
                },
            }
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StitchResult {
    info: ImageInfo,
    offsets: Vec<[i64; 2]>,
    scores: Vec<f32>,
}

/// Stitches open images (in order) into a new panorama image, kept in memory until saved.
#[tauri::command]
async fn stitch(ids: Vec<u32>, offsets: Vec<Option<[i64; 2]>>, state: State<'_, AppState>) -> CmdResult<StitchResult> {
    let mut store = state.lock()?;
    let (s, pano) = {
        let images: Vec<&ThermalImage> = ids
            .iter()
            .map(|id| store.images.get(id).ok_or_else(|| CmdError::not_found("An image is no longer open")))
            .collect::<Result<_, _>>()?;
        let tiles: Vec<panorama::Tile> = images
            .iter()
            .map(|i| panorama::Tile { kelvin: i.kelvin(), width: i.width(), height: i.height() })
            .collect();
        let s = panorama::stitch(&tiles, &offsets)?;
        let first = images[0];
        let dir = first.path().parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = first.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
        let camera = Camera {
            date_time: first.camera().date_time.clone(),
            description: first.camera().description.clone(),
            ..Default::default()
        };
        let pano = ThermalImage::from_kelvin(
            &dir.join(format!("{stem}-panorama.tiff")),
            Format::Panorama,
            s.kelvin.clone(),
            s.width,
            s.height,
            camera,
        )?;
        (s, pano)
    };
    store.next_id += 1;
    let id = store.next_id;
    let out = info(id, &pano, None, true);
    store.images.insert(id, pano);
    Ok(StitchResult { info: out, offsets: s.offsets, scores: s.scores })
}

fn not_a_source(path: &Path, source: &Path) -> CmdResult<()> {
    let same = path.canonicalize().ok().zip(source.canonicalize().ok()).is_some_and(|(a, b)| a == b) || path == source;
    if same {
        Err(CmdError::invalid_input("Choose a new file name: the original image is never overwritten"))
    } else {
        Ok(())
    }
}

/// The file types each command that creates a file may write, as lower-case extensions.
mod write_types {
    pub const IMAGE: &[&str] = &["png", "jpg", "jpeg"]; // write_staged
    pub const TEXT: &[&str] = &["json", "csv", "kml", "geojson", "txt"]; // write_text
    pub const AVI: &[&str] = &["avi"]; // avi_finish
    pub const SEQ: &[&str] = &["seq"]; // export_segment
    pub const CSV: &[&str] = &["csv"]; // export_csv
    pub const TIFF: &[&str] = &["tiff", "tif"]; // export_thermal_tiff
    pub const FLIR_JPEG: &[&str] = &["jpg", "jpeg"]; // save_flir_copy
    pub const MAP: &[&str] = &["kml", "geojson", "json"]; // export_map
    pub const PDF: &[&str] = &["pdf"]; // make_report
    pub const HTML: &[&str] = &["html", "htm"]; // make_report
}

fn checked_write_path(path: &str, allowed: &[&str]) -> CmdResult<()> {
    if folder::has_extension(Path::new(path), allowed) {
        return Ok(());
    }
    let list: Vec<String> = allowed.iter().map(|e| format!(".{e}")).collect();
    Err(CmdError::unsupported(format!("This file must be saved as {}", list.join(", "))))
}

/// Saves the temperatures as a 32-bit float TIFF in kelvin.
#[tauri::command]
async fn export_thermal_tiff(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::TIFF)?;
    let bytes = with_image(&state, id, |img| {
        not_a_source(Path::new(&path), img.path())?;
        Ok(export::thermal_tiff(img.kelvin(), img.width(), img.height())?)
    })?;
    Ok(std::fs::write(&path, bytes)?)
}

/// Saves a copy of a FLIR radiometric JPEG with the current parameters written into it.
#[tauri::command]
async fn save_flir_copy(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::FLIR_JPEG)?;
    let (source, params) = with_image(&state, id, |img| {
        if !img.is_flir_jpeg() {
            return Err(CmdError::unsupported("Only FLIR radiometric JPEGs can be saved with new parameters"));
        }
        Ok((img.path().to_path_buf(), img.params()))
    })?;
    not_a_source(Path::new(&path), &source)?;
    let original = std::fs::read(&source)?;
    let copy = fff::rewrite_jpeg_params(&original, &params)?;
    Ok(std::fs::write(&path, copy)?)
}

#[tauri::command]
async fn select_frame(id: u32, index: usize, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    let store = state.lock()?;
    let video = store.videos.get(&id).ok_or_else(|| CmdError::unsupported("This image is not a video"))?.clone();
    let current = store.images.get(&id).ok_or_else(|| CmdError::not_found("This video is no longer open"))?;
    let params = (current.params() != current.file_params()).then(|| current.params());
    drop(store);
    let frame = cached_frame(&state, id, &video, index, &params)?;
    let out = Recomputed { params: frame.params(), range: frame.range() };
    state.lock()?.images.insert(id, (*frame).clone());
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Series {
    rows: Vec<Vec<Option<Stats>>>,
    times: Vec<f64>,
}

/// Each measurement's statistics in frames `first..=last`, with the current parameters.
#[tauri::command]
async fn video_series(
    id: u32,
    first: usize,
    last: usize,
    measurements: Vec<Measurement>,
    state: State<'_, AppState>,
) -> CmdResult<Series> {
    let (video, params) = {
        let store = state.lock()?;
        let v = store.videos.get(&id).ok_or_else(|| CmdError::unsupported("This image is not a video"))?.clone();
        let img = store.images.get(&id).ok_or_else(|| CmdError::not_found("This video is no longer open"))?;
        (v, (img.params() != img.file_params()).then(|| img.params()))
    };
    let rows = video.series(first, last, params.as_ref(), &measurements)?;
    let times = video.info().times.into_iter().skip(first).take(rows.len()).collect();
    Ok(Series { rows, times })
}

/// High-sensitivity view of the current frame: its difference from the average of the previous
/// `reference` frames, optionally 2×2 binned. Returns the difference in kelvin (f32 LE).
#[tauri::command]
async fn hsm(
    id: u32,
    index: usize,
    reference: usize,
    binning: bool,
    state: State<'_, AppState>,
) -> CmdResult<Response> {
    let (video, params) = {
        let store = state.lock()?;
        let v = store.videos.get(&id).ok_or_else(|| CmdError::unsupported("This image is not a video"))?.clone();
        let img = store.images.get(&id).ok_or_else(|| CmdError::not_found("This video is no longer open"))?;
        (v, (img.params() != img.file_params()).then(|| img.params()))
    };
    let current = cached_frame(&state, id, &video, index, &params)?;
    let (w, h) = (current.width(), current.height());
    let first = index.saturating_sub(reference.clamp(1, 60));
    let mut mean = vec![0f64; w * h];
    let mut n = 0usize;
    for i in first..index {
        let f = cached_frame(&state, id, &video, i, &params)?;
        for (m, k) in mean.iter_mut().zip(f.kelvin()) {
            *m += *k as f64;
        }
        n += 1;
    }
    let mut diff: Vec<f32> = if n == 0 {
        vec![0.0; w * h]
    } else {
        current.kelvin().iter().zip(&mean).map(|(k, m)| (*k as f64 - m / n as f64) as f32).collect()
    };
    if binning {
        diff = video::bin2x2(&diff, w, h);
    }
    Ok(Response::new(diff.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()))
}

#[tauri::command]
async fn export_segment(
    id: u32,
    first: usize,
    last: usize,
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<usize> {
    checked_write_path(&path, write_types::SEQ)?;
    let video =
        state.lock()?.videos.get(&id).ok_or_else(|| CmdError::unsupported("This image is not a video"))?.clone();
    not_a_source(Path::new(&path), video.path())?;
    Ok(video.write_segment(first, last, Path::new(&path))?)
}

/// Adds one JPEG frame (raw request body) to the video export in progress.
#[tauri::command]
fn avi_add_frame(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CmdError::invalid_input("expected raw bytes"));
    };
    let mut store = state.lock()?;
    store.avi_frames.push(bytes.clone());
    Ok(store.avi_frames.len())
}

#[tauri::command]
fn avi_reset(state: State<'_, AppState>) -> CmdResult<()> {
    state.lock()?.avi_frames.clear();
    Ok(())
}

/// Writes the collected frames as a Motion-JPEG AVI.
#[tauri::command]
async fn avi_finish(
    path: String,
    width: usize,
    height: usize,
    fps: f64,
    state: State<'_, AppState>,
) -> CmdResult<usize> {
    checked_write_path(&path, write_types::AVI)?;
    let frames = std::mem::take(&mut state.lock()?.avi_frames);
    if frames.is_empty() {
        return Err(CmdError::invalid_input("No frames to save"));
    }
    let n = frames.len();
    std::fs::write(&path, avi::container(&frames, width, height, fps))?;
    Ok(n)
}

#[tauri::command]
async fn list_folder(path: String) -> CmdResult<Vec<String>> {
    let files = folder::candidates(Path::new(&path))?;
    Ok(files.into_iter().map(|p| p.display().to_string()).collect())
}

#[tauri::command]
fn is_folder(path: String) -> bool {
    Path::new(&path).is_dir()
}

#[tauri::command]
fn palettes() -> Vec<palette::Palette> {
    palette::all().to_vec()
}

#[tauri::command]
fn templates() -> Vec<Template> {
    report::builtin_templates()
}

#[tauri::command]
async fn export_csv(id: u32, path: String, unit: Unit, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::CSV)?;
    let csv = with_image(&state, id, |img| Ok(export::csv(img.kelvin(), img.width(), unit)))?;
    Ok(std::fs::write(&path, csv)?)
}

/// Receives raw bytes from the UI (sent as the request body), kept until `write_staged`.
#[tauri::command]
fn stage_bytes(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CmdError::invalid_input("expected raw bytes"));
    };
    let len = bytes.len();
    state.lock()?.staged = Some(bytes.clone());
    Ok(len)
}

fn extension(path: &str) -> Option<String> {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Writes the staged bytes to an image file the user chose in a save dialog.
#[tauri::command]
async fn write_staged(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::IMAGE)?;
    let bytes = state.lock()?.staged.take().ok_or_else(|| CmdError::invalid_input("Nothing to save"))?;
    Ok(std::fs::write(&path, bytes)?)
}

/// Writes text the UI produced (settings, tables, maps) to a file the user chose.
#[tauri::command]
async fn write_text(path: String, text: String) -> CmdResult<()> {
    checked_write_path(&path, write_types::TEXT)?;
    Ok(std::fs::write(&path, text)?)
}

/// Reads a settings or template file the user picked.
#[tauri::command]
async fn read_text(path: String) -> CmdResult<String> {
    if !folder::has_extension(Path::new(&path), &["json"]) {
        return Err(CmdError::unsupported("Only .json settings files can be imported"));
    }
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 5_000_000 {
        return Err(CmdError::invalid_file("That file is too large to be a settings file"));
    }
    Ok(std::fs::read_to_string(&path)?)
}

/// A PNG or JPEG the user picked (a logo), as a data URL.
#[tauri::command]
async fn read_image_data_url(path: String) -> CmdResult<String> {
    let mime = match extension(&path).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => return Err(CmdError::unsupported("Logos must be PNG or JPEG images")),
    };
    let bytes = std::fs::read(&path)?;
    if bytes.len() > 4_000_000 {
        return Err(CmdError::invalid_file("The logo is too large (over 4 MB)"));
    }
    image_check(&bytes)?;
    Ok(format!("data:{mime};base64,{}", report::base64_encode(&bytes)))
}

fn image_check(bytes: &[u8]) -> CmdResult<()> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8]) {
        Ok(())
    } else {
        Err(CmdError::invalid_file("Not a PNG or JPEG image"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MapItem {
    id: u32,
    description: String,
}

/// Saves where the images were taken as KML or GeoJSON (by the file extension).
#[tauri::command]
async fn export_map(items: Vec<MapItem>, title: String, path: String, state: State<'_, AppState>) -> CmdResult<usize> {
    checked_write_path(&path, write_types::MAP)?;
    let marks: Vec<geo::Placemark> = {
        let store = state.lock()?;
        items
            .iter()
            .filter_map(|it| {
                let img = store.images.get(&it.id)?;
                let c = img.camera();
                Some(geo::Placemark {
                    name: img.file_name(),
                    latitude: c.latitude?,
                    longitude: c.longitude?,
                    altitude: c.altitude,
                    description: it.description.clone(),
                    time: c.date_time.clone(),
                })
            })
            .collect()
    };
    if marks.is_empty() {
        return Err(CmdError::invalid_input("None of these images has a GPS position"));
    }
    let text = if extension(&path).as_deref() == Some("kml") { geo::kml(&title, &marks) } else { geo::geojson(&marks) };
    std::fs::write(&path, text)?;
    Ok(marks.len())
}

#[tauri::command]
async fn save_sidecar(id: u32, edits: Sidecar, state: State<'_, AppState>) -> CmdResult<String> {
    let (path, format) = with_image(&state, id, |img| Ok((img.path().to_path_buf(), img.format())))?;
    if format == Format::Panorama {
        return Err(CmdError::unsupported("Save the panorama as a radiometric TIFF first (Export ▸ Radiometric TIFF)"));
    }
    Ok(sidecar::save(&path, &edits).map(|p| p.display().to_string())?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportItem {
    id: u32,
    look: Look,
    measurements: Vec<Measurement>,
    notes: String,
    #[serde(default)]
    formulas: Vec<Formula>,
    #[serde(default)]
    annotations: Vec<Annotation>,
    #[serde(default)]
    findings: Vec<Finding>,
    #[serde(default)]
    fields: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    asset: String,
    #[serde(default)]
    presentation: Presentation,
}

#[tauri::command]
async fn make_report(
    info: ReportInfo,
    items: Vec<ReportItem>,
    path: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    checked_write_path(
        &path,
        match info.format {
            report::ReportFormat::Pdf => write_types::PDF,
            report::ReportFormat::Html => write_types::HTML,
        },
    )?;
    let generator = format!("Kelvinlens {}", app.package_info().version);
    let bytes = {
        let store = state.lock()?;
        let mut images = Vec::with_capacity(items.len());
        for item in items {
            let image = store
                .images
                .get(&item.id)
                .ok_or_else(|| CmdError::not_found("An image in the report is no longer open"))?;
            images.push(ReportImage {
                image,
                look: item.look.normalized(),
                measurements: item.measurements,
                notes: item.notes,
                formulas: item.formulas,
                annotations: item.annotations,
                findings: item.findings,
                fields: item.fields,
                asset: item.asset,
                presentation: item.presentation,
            });
        }
        report::generate(&info, &images, &generator)?
    };
    Ok(std::fs::write(PathBuf::from(path), bytes)?)
}

/// Files passed on the command line (also how "Open with" works on Linux and Windows).
#[tauri::command]
fn initial_paths() -> Vec<String> {
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
}

/// `--self-test <file>` opens a thermogram, prints its size and centre temperature, and exits
/// without creating a window. CI runs it on the packaged binary of every OS.
fn self_test(path: &str) -> i32 {
    let p = Path::new(path);
    let opened = if video::is_video(p) { Video::open(p).and_then(|v| v.frame(0, None)) } else { ThermalImage::open(p) };
    match opened {
        Ok(img) => {
            let centre = img.kelvin_at(img.width() / 2, img.height() / 2).unwrap_or(f32::NAN);
            println!("{} {}x{} centre={:.2}C", img.file_name(), img.width(), img.height(), centre - 273.15);
            0
        }
        Err(e) => {
            eprintln!("self-test failed: {e}");
            1
        }
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--self-test") {
        std::process::exit(self_test(args.get(i + 1).map(String::as_str).unwrap_or("")));
    }

    macro_rules! handlers {
        ($($extra:path),*) => {
            tauri::generate_handler![
                open_image,
                close_image,
                thermal_data,
                photo_jpeg,
                set_params,
                reset_params,
                measure,
                outlines,
                profile,
                coverage,
                evaluate_formulas,
                render,
                fuse,
                inspect,
                triage,
                stitch,
                export_thermal_tiff,
                save_flir_copy,
                select_frame,
                video_series,
                hsm,
                export_segment,
                avi_add_frame,
                avi_reset,
                avi_finish,
                list_folder,
                is_folder,
                palettes,
                templates,
                export_csv,
                stage_bytes,
                write_staged,
                write_text,
                read_text,
                read_image_data_url,
                export_map,
                save_sidecar,
                make_report,
                initial_paths,
                $($extra),*
            ]
        };
    }

    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init()).manage(AppState::default());
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(handlers![dev::dev_log]).setup(|app| {
        dev::start(app.handle());
        Ok(())
    });
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(handlers![]);
    builder.run(tauri::generate_context!()).expect("error while running Kelvinlens");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_error::ErrorKind;

    #[test]
    fn write_policy_accepts_listed_extensions_case_insensitively() {
        assert!(checked_write_path("a.CSV", write_types::CSV).is_ok());
        assert!(checked_write_path("a.tif", write_types::TIFF).is_ok());
        assert!(checked_write_path("a.htm", write_types::HTML).is_ok());
    }

    #[test]
    fn write_policy_rejects_other_or_missing_extensions() {
        for path in ["a.exe", "csv", "a.csv.exe"] {
            assert!(checked_write_path(path, write_types::CSV).is_err(), "{path}");
        }
        let e = checked_write_path("a.exe", write_types::CSV).unwrap_err();
        assert_eq!((e.kind, e.message.as_str()), (ErrorKind::Unsupported, "This file must be saved as .csv"));
    }
}

//! Tauri shell: keeps opened images and videos in memory and exposes the core library to the UI.
//! The app has no network access; files are only read or written at paths the user picked.

#[cfg(debug_assertions)]
mod dev;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kelvinlens_core::analysis::{self, Finding, InspectSettings, Isotherm};
use kelvinlens_core::formula::{self, Formula, FormulaResult};
use kelvinlens_core::measure::ProfilePoint;
use kelvinlens_core::render::{self, Fusion, Look};
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
    if let Some(f) = state.lock().map_err(err)?.frames.get(id, index, params) {
        return Ok(f);
    }
    let frame = std::sync::Arc::new(video.frame(index, params.as_ref()).map_err(err)?);
    state.lock().map_err(err)?.frames.put(id, index, *params, frame.clone());
    Ok(frame)
}

type AppState = Mutex<Store>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn with_image<T>(state: &AppState, id: u32, f: impl FnOnce(&mut ThermalImage) -> CmdResult<T>) -> CmdResult<T> {
    let mut store = state.lock().map_err(err)?;
    let image = store.images.get_mut(&id).ok_or_else(|| "This image is no longer open".to_string())?;
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
        let v = Video::open(p).map_err(err)?;
        (v.frame(0, None).map_err(err)?, Some(v))
    } else {
        (ThermalImage::open(p).map_err(err)?, None)
    };
    let mut store = state.lock().map_err(err)?;
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
    let mut store = state.lock().map_err(err)?;
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
    let photo = with_image(&state, id, |img| img.photo().cloned().ok_or_else(|| "No photo".to_string()))?;
    let jpeg = export::jpeg(&photo.rgb, photo.width, photo.height, 92).map_err(err)?;
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
        img.set_params(&params).map_err(err)?;
        Ok(Recomputed { params: img.params(), range: img.range() })
    })
}

#[tauri::command]
async fn reset_params(id: u32, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    with_image(&state, id, |img| {
        img.reset_params().map_err(err)?;
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
        let matchers: Vec<_> = isotherms.iter().map(|i| i.kind.matcher()).collect();
        Ok(shapes
            .iter()
            .map(|s| {
                if !s.is_area() || matchers.is_empty() {
                    return None;
                }
                measure::coverage(img.kelvin(), img.width(), img.height(), s, |k| matchers.iter().any(|m| m(k)))
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
        Ok(formula::evaluate_all(
            &formulas,
            |name| {
                let i = measurements
                    .iter()
                    .position(|m| m.label == name)
                    .or_else(|| measurements.iter().position(|m| m.label.eq_ignore_ascii_case(name)))?;
                Some(formula::Value::Measurement {
                    stats: stats[i].as_ref(),
                    spot: matches!(measurements[i].shape, Shape::Spot { .. }),
                })
            },
            unit,
        ))
    })
}

/// The thermal image coloured with `look`, as RGBA (4 bytes per pixel).
#[tauri::command]
async fn render(id: u32, look: Look, state: State<'_, AppState>) -> CmdResult<Response> {
    let look = Look { palette: look.palette.normalized(), ..look };
    with_image(&state, id, |img| {
        Ok(Response::new(render::rgba(&render::rgb(img.kelvin(), img.width(), img.height(), &look))))
    })
}

/// A thermal/photo composite, little-endian: width and height (u32), the thermal source rectangle
/// and where it lands in the composite (8 × f32: x, y, w, h twice), then RGBA pixels.
#[tauri::command]
async fn fuse(
    id: u32,
    look: Look,
    fusion: Fusion,
    max_width: usize,
    state: State<'_, AppState>,
) -> CmdResult<Response> {
    let look = Look { palette: look.palette.normalized(), ..look };
    with_image(&state, id, |img| {
        let photo = img.photo().ok_or("This image has no photo")?;
        let c = render::fuse(
            img.kelvin(),
            img.width(),
            img.height(),
            &look,
            photo,
            img.pip(),
            fusion,
            max_width.clamp(64, 4096),
        );
        let mut out = Vec::with_capacity(40 + c.rgb.len() / 3 * 4);
        out.extend_from_slice(&(c.width as u32).to_le_bytes());
        out.extend_from_slice(&(c.height as u32).to_le_bytes());
        for v in c.source.iter().chain(&c.destination) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend(render::rgba(&c.rgb));
        Ok(Response::new(out))
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
            files.extend(folder::candidates(&path).map_err(err)?);
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
    let mut store = state.lock().map_err(err)?;
    let (s, pano) = {
        let images: Vec<&ThermalImage> =
            ids.iter().map(|id| store.images.get(id).ok_or("An image is no longer open")).collect::<Result<_, _>>()?;
        let tiles: Vec<panorama::Tile> = images
            .iter()
            .map(|i| panorama::Tile { kelvin: i.kelvin(), width: i.width(), height: i.height() })
            .collect();
        let s = panorama::stitch(&tiles, &offsets, &Default::default())?;
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
        )
        .map_err(err)?;
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
    if same { Err("Choose a new file name: the original image is never overwritten".into()) } else { Ok(()) }
}

/// Saves the temperatures as a 32-bit float TIFF in kelvin.
#[tauri::command]
async fn export_thermal_tiff(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let bytes = with_image(&state, id, |img| {
        not_a_source(Path::new(&path), img.path())?;
        export::thermal_tiff(img.kelvin(), img.width(), img.height()).map_err(err)
    })?;
    std::fs::write(&path, bytes).map_err(err)
}

/// Saves a copy of a FLIR radiometric JPEG with the current parameters written into it.
#[tauri::command]
async fn save_flir_copy(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let (source, params) = with_image(&state, id, |img| {
        if !img.is_flir_jpeg() {
            return Err("Only FLIR radiometric JPEGs can be saved with new parameters".into());
        }
        Ok((img.path().to_path_buf(), img.params()))
    })?;
    not_a_source(Path::new(&path), &source)?;
    let original = std::fs::read(&source).map_err(err)?;
    let copy = fff::rewrite_jpeg_params(&original, &params).map_err(err)?;
    std::fs::write(&path, copy).map_err(err)
}

#[tauri::command]
async fn select_frame(id: u32, index: usize, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    let store = state.lock().map_err(err)?;
    let video = store.videos.get(&id).ok_or("This image is not a video")?.clone();
    let current = store.images.get(&id).ok_or("This video is no longer open")?;
    let params = (current.params() != current.file_params()).then(|| current.params());
    drop(store);
    let frame = cached_frame(&state, id, &video, index, &params)?;
    let out = Recomputed { params: frame.params(), range: frame.range() };
    state.lock().map_err(err)?.images.insert(id, (*frame).clone());
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
        let store = state.lock().map_err(err)?;
        let v = store.videos.get(&id).ok_or("This image is not a video")?.clone();
        let img = store.images.get(&id).ok_or("This video is no longer open")?;
        (v, (img.params() != img.file_params()).then(|| img.params()))
    };
    let rows = video.series(first, last, params.as_ref(), &measurements).map_err(err)?;
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
        let store = state.lock().map_err(err)?;
        let v = store.videos.get(&id).ok_or("This image is not a video")?.clone();
        let img = store.images.get(&id).ok_or("This video is no longer open")?;
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
    let video = state.lock().map_err(err)?.videos.get(&id).ok_or("This image is not a video")?.clone();
    not_a_source(Path::new(&path), video.path())?;
    video.write_segment(first, last, Path::new(&path)).map_err(err)
}

/// Adds one JPEG frame (raw request body) to the video export in progress.
#[tauri::command]
fn avi_add_frame(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw bytes".into());
    };
    let mut store = state.lock().map_err(err)?;
    store.avi_frames.push(bytes.clone());
    Ok(store.avi_frames.len())
}

#[tauri::command]
fn avi_reset(state: State<'_, AppState>) -> CmdResult<()> {
    state.lock().map_err(err)?.avi_frames.clear();
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
    if !path.to_lowercase().ends_with(".avi") {
        return Err("Videos are saved as .avi".into());
    }
    let frames = std::mem::take(&mut state.lock().map_err(err)?.avi_frames);
    if frames.is_empty() {
        return Err("No frames to save".into());
    }
    let n = frames.len();
    std::fs::write(&path, avi::container(&frames, width, height, fps)).map_err(err)?;
    Ok(n)
}

#[tauri::command]
async fn list_folder(path: String) -> CmdResult<Vec<String>> {
    let files = folder::candidates(Path::new(&path)).map_err(err)?;
    Ok(files.into_iter().map(|p| p.display().to_string()).collect())
}

#[tauri::command]
fn is_folder(path: String) -> bool {
    Path::new(&path).is_dir()
}

#[tauri::command]
fn palettes() -> Vec<palette::Palette> {
    palette::all()
}

#[tauri::command]
fn templates() -> Vec<Template> {
    report::builtin_templates()
}

#[tauri::command]
async fn export_csv(id: u32, path: String, unit: Unit, state: State<'_, AppState>) -> CmdResult<()> {
    let csv = with_image(&state, id, |img| Ok(export::csv(img.kelvin(), img.width(), unit)))?;
    std::fs::write(&path, csv).map_err(err)
}

/// Receives raw bytes from the UI (sent as the request body), kept until `write_staged`.
#[tauri::command]
fn stage_bytes(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw bytes".into());
    };
    let len = bytes.len();
    state.lock().map_err(err)?.staged = Some(bytes.clone());
    Ok(len)
}

fn extension(path: &str) -> Option<String> {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Writes the staged bytes to an image file the user chose in a save dialog.
#[tauri::command]
async fn write_staged(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    if !matches!(extension(&path).as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err("Exported images must be saved as .png or .jpg".into());
    }
    let bytes = state.lock().map_err(err)?.staged.take().ok_or("Nothing to save")?;
    std::fs::write(&path, bytes).map_err(err)
}

/// Writes text the UI produced (settings, tables, maps) to a file the user chose.
#[tauri::command]
async fn write_text(path: String, text: String) -> CmdResult<()> {
    if !matches!(extension(&path).as_deref(), Some("json" | "csv" | "kml" | "geojson" | "txt")) {
        return Err("This kind of file cannot be written here".into());
    }
    std::fs::write(&path, text).map_err(err)
}

/// Reads a settings or template file the user picked.
#[tauri::command]
async fn read_text(path: String) -> CmdResult<String> {
    if extension(&path).as_deref() != Some("json") {
        return Err("Only .json settings files can be imported".into());
    }
    let meta = std::fs::metadata(&path).map_err(err)?;
    if meta.len() > 5_000_000 {
        return Err("That file is too large to be a settings file".into());
    }
    std::fs::read_to_string(&path).map_err(err)
}

/// A PNG or JPEG the user picked (a logo), as a data URL.
#[tauri::command]
async fn read_image_data_url(path: String) -> CmdResult<String> {
    let mime = match extension(&path).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => return Err("Logos must be PNG or JPEG images".into()),
    };
    let bytes = std::fs::read(&path).map_err(err)?;
    if bytes.len() > 4_000_000 {
        return Err("The logo is too large (over 4 MB)".into());
    }
    image_check(&bytes)?;
    Ok(format!("data:{mime};base64,{}", report::base64_encode(&bytes)))
}

fn image_check(bytes: &[u8]) -> CmdResult<()> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8]) {
        Ok(())
    } else {
        Err("Not a PNG or JPEG image".into())
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
    let marks: Vec<geo::Placemark> = {
        let store = state.lock().map_err(err)?;
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
        return Err("None of these images has a GPS position".into());
    }
    let text = match extension(&path).as_deref() {
        Some("kml") => geo::kml(&title, &marks),
        Some("geojson" | "json") => geo::geojson(&marks),
        _ => return Err("Maps are saved as .kml or .geojson".into()),
    };
    std::fs::write(&path, text).map_err(err)?;
    Ok(marks.len())
}

#[tauri::command]
async fn save_sidecar(id: u32, edits: Sidecar, state: State<'_, AppState>) -> CmdResult<String> {
    let (path, format) = with_image(&state, id, |img| Ok((img.path().to_path_buf(), img.format())))?;
    if format == Format::Panorama {
        return Err("Save the panorama as a radiometric TIFF first (Export ▸ Radiometric TIFF)".into());
    }
    sidecar::save(&path, &edits).map(|p| p.display().to_string()).map_err(err)
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
    let ext = extension(&path);
    let expected = match info.format {
        report::ReportFormat::Pdf => "pdf",
        report::ReportFormat::Html => "html",
    };
    if ext.as_deref() != Some(expected) && !(expected == "html" && ext.as_deref() == Some("htm")) {
        return Err(format!("Reports in this format are saved as .{expected}"));
    }
    let generator = format!("Kelvinlens {}", app.package_info().version);
    let bytes = {
        let store = state.lock().map_err(err)?;
        let mut images = Vec::with_capacity(items.len());
        for item in items {
            let image = store.images.get(&item.id).ok_or("An image in the report is no longer open")?;
            images.push(ReportImage {
                image,
                look: Look { palette: item.look.palette.normalized(), ..item.look },
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
        report::generate(&info, &images, &generator).map_err(err)?
    };
    std::fs::write(PathBuf::from(path), bytes).map_err(err)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
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

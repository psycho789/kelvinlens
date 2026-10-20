//! Opening and closing images, parameters, triage, panoramas, folders and static lists.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kelvinlens_core::{
    Format, Params, ThermalImage, export, folder, palette, panorama,
    report::{self, Template},
    thermogram::Camera,
    triage,
    video::{self, Video},
};
use tauri::State;
use tauri::ipc::Response;

use crate::cmd_error::CmdError;
use crate::dto::{ImageInfo, Recomputed, StitchResult, TriageRow, TriageSettings, info};
use crate::state::{AppState, CmdResult, image, with_image_mut};

#[tauri::command]
pub(crate) async fn open_image(path: String, state: State<'_, AppState>) -> CmdResult<ImageInfo> {
    let p = Path::new(&path);
    let (image, video) = if video::is_video(p) {
        let v = Video::open(p)?;
        (v.frame(0, None)?, Some(v))
    } else {
        (ThermalImage::open(p)?, None)
    };
    let image = Arc::new(image);
    let video = video.map(Arc::new);
    let id = {
        let mut store = state.lock()?;
        let id = store.insert_image(image.clone());
        if let Some(v) = &video {
            store.videos.insert(id, v.clone());
        }
        id
    };
    Ok(info(id, &image, video.as_deref(), false))
}

#[tauri::command]
pub(crate) async fn close_image(id: u32, state: State<'_, AppState>) -> CmdResult<()> {
    let mut store = state.lock()?;
    store.images.remove(&id);
    store.videos.remove(&id);
    store.frames.forget(id);
    Ok(())
}

/// Temperatures in kelvin as little-endian f32, row-major.
#[tauri::command]
pub(crate) async fn thermal_data(id: u32, state: State<'_, AppState>) -> CmdResult<Response> {
    let img = image(&state, id)?;
    let bytes: Vec<u8> = img.kelvin().iter().flat_map(|v| v.to_le_bytes()).collect();
    Ok(Response::new(bytes))
}

#[tauri::command]
pub(crate) async fn photo_jpeg(id: u32, state: State<'_, AppState>) -> CmdResult<Response> {
    let img = image(&state, id)?;
    let photo = img.photo().ok_or_else(|| CmdError::not_found("No photo"))?;
    let jpeg = export::jpeg(&photo.rgb, photo.width, photo.height, 92)?;
    Ok(Response::new(jpeg))
}

#[tauri::command]
pub(crate) async fn set_params(id: u32, params: Params, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    with_image_mut(&state, id, |img| {
        img.set_params(&params)?;
        Ok(Recomputed { params: img.params(), range: img.range() })
    })
}

#[tauri::command]
pub(crate) async fn reset_params(id: u32, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    with_image_mut(&state, id, |img| {
        img.reset_params()?;
        Ok(Recomputed { params: img.params(), range: img.range() })
    })
}

/// Opens each file (videos: the first frame) and reports its hottest and coldest temperature.
#[tauri::command]
pub(crate) async fn triage(paths: Vec<String>, settings: TriageSettings) -> CmdResult<Vec<TriageRow>> {
    let files = triage::expand(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())?;
    Ok(files.iter().map(|p| triage::summarize(p, settings.above_k, settings.below_k)).map(TriageRow::from).collect())
}

/// Stitches open images (in order) into a new panorama image, kept in memory until saved.
#[tauri::command]
pub(crate) async fn stitch(
    ids: Vec<u32>,
    offsets: Vec<Option<[i64; 2]>>,
    state: State<'_, AppState>,
) -> CmdResult<StitchResult> {
    let held = {
        let store = state.lock()?;
        ids.iter().map(|id| store.image_or(*id, "An image is no longer open")).collect::<CmdResult<Vec<_>>>()?
    };
    let tiles: Vec<panorama::Tile> =
        held.iter().map(|i| panorama::Tile { kelvin: i.kelvin(), width: i.width(), height: i.height() }).collect();
    let s = panorama::stitch(&tiles, &offsets)?;
    let first = &held[0];
    let camera = Camera {
        date_time: first.camera().date_time.clone(),
        description: first.camera().description.clone(),
        ..Default::default()
    };
    let pano = ThermalImage::from_kelvin(
        &panorama::output_path(first.path()),
        Format::Panorama,
        s.kelvin.clone(),
        s.width,
        s.height,
        camera,
    )?;
    let pano = Arc::new(pano);
    let id = state.lock()?.insert_image(pano.clone());
    let out = info(id, &pano, None, true);
    Ok(StitchResult { info: out, offsets: s.offsets, scores: s.scores })
}

#[tauri::command]
pub(crate) async fn list_folder(path: String) -> CmdResult<Vec<String>> {
    let files = folder::candidates(Path::new(&path))?;
    Ok(files.into_iter().map(|p| p.display().to_string()).collect())
}

#[tauri::command]
pub(crate) fn is_folder(path: String) -> bool {
    Path::new(&path).is_dir()
}

#[tauri::command]
pub(crate) fn palettes() -> Vec<palette::Palette> {
    palette::all().to_vec()
}

#[tauri::command]
pub(crate) fn templates() -> Vec<Template> {
    report::builtin_templates()
}

/// Files passed on the command line (also how "Open with" works on Linux and Windows).
#[tauri::command]
pub(crate) fn initial_paths() -> Vec<String> {
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
}

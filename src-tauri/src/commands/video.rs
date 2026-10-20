//! Radiometric video: frames, series, high-sensitivity view and exports.

use std::path::Path;

use kelvinlens_core::{Measurement, avi, video};
use tauri::State;
use tauri::ipc::Response;

use crate::cmd_error::CmdError;
use crate::commands::files::{checked_write_path, not_a_source, write_types};
use crate::dto::{Recomputed, Series};
use crate::state::{AppState, CmdResult, cached_frame, video_and_params};

#[tauri::command]
pub(crate) async fn select_frame(id: u32, index: usize, state: State<'_, AppState>) -> CmdResult<Recomputed> {
    let (video, params) = video_and_params(&state, id)?;
    let frame = cached_frame(&state, id, &video, index, &params)?;
    let out = Recomputed { params: frame.params(), range: frame.range() };
    state.lock()?.images.insert(id, frame);
    Ok(out)
}

/// Each measurement's statistics in frames `first..=last`, with the current parameters.
#[tauri::command]
pub(crate) async fn video_series(
    id: u32,
    first: usize,
    last: usize,
    measurements: Vec<Measurement>,
    state: State<'_, AppState>,
) -> CmdResult<Series> {
    let (video, params) = video_and_params(&state, id)?;
    let rows = video.series(first, last, params.as_ref(), &measurements)?;
    let times = video.info().times.into_iter().skip(first).take(rows.len()).collect();
    Ok(Series { rows, times })
}

/// High-sensitivity view of the current frame: its difference from the average of the previous
/// `reference` frames, optionally 2×2 binned. Returns the difference in kelvin (f32 LE).
#[tauri::command]
pub(crate) async fn hsm(
    id: u32,
    index: usize,
    reference: usize,
    binning: bool,
    state: State<'_, AppState>,
) -> CmdResult<Response> {
    let (video, params) = video_and_params(&state, id)?;
    let current = cached_frame(&state, id, &video, index, &params)?;
    let (w, h) = (current.width(), current.height());
    let first = index.saturating_sub(reference.clamp(1, 60));
    let frames = (first..index).map(|i| cached_frame(&state, id, &video, i, &params)).collect::<CmdResult<Vec<_>>>()?;
    let prev: Vec<&[f32]> = frames.iter().map(|f| f.kelvin()).collect();
    let diff = video::difference_from_mean(current.kelvin(), &prev, w, h, binning);
    Ok(Response::new(diff.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()))
}

#[tauri::command]
pub(crate) async fn export_segment(
    id: u32,
    first: usize,
    last: usize,
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<usize> {
    checked_write_path(&path, write_types::SEQ)?;
    let video = state.lock()?.video(id)?;
    not_a_source(Path::new(&path), video.path())?;
    Ok(video.write_segment(first, last, Path::new(&path))?)
}

/// Adds one JPEG frame (raw request body) to the video export in progress.
#[tauri::command]
pub(crate) fn avi_add_frame(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CmdError::invalid_input("expected raw bytes"));
    };
    let mut store = state.lock()?;
    store.avi_frames.push(bytes.clone());
    Ok(store.avi_frames.len())
}

#[tauri::command]
pub(crate) fn avi_reset(state: State<'_, AppState>) -> CmdResult<()> {
    state.lock()?.avi_frames.clear();
    Ok(())
}

/// Writes the collected frames as a Motion-JPEG AVI.
#[tauri::command]
pub(crate) async fn avi_finish(
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

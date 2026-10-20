//! Commands that read or write files the user picked, and the write policy.

use std::path::Path;

use kelvinlens_core::{
    Format, Unit, export, fff, folder, geo, report,
    sidecar::{self, Sidecar},
};
use tauri::State;

use crate::cmd_error::CmdError;
use crate::dto::MapItem;
use crate::state::{AppState, CmdResult, image};

pub(crate) fn not_a_source(path: &Path, source: &Path) -> CmdResult<()> {
    let same = path.canonicalize().ok().zip(source.canonicalize().ok()).is_some_and(|(a, b)| a == b) || path == source;
    if same {
        Err(CmdError::invalid_input("Choose a new file name: the original image is never overwritten"))
    } else {
        Ok(())
    }
}

/// The file types each command that creates a file may write, as lower-case extensions.
pub(crate) mod write_types {
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

pub(crate) fn checked_write_path(path: &str, allowed: &[&str]) -> CmdResult<()> {
    if folder::has_extension(Path::new(path), allowed) {
        return Ok(());
    }
    let list: Vec<String> = allowed.iter().map(|e| format!(".{e}")).collect();
    Err(CmdError::unsupported(format!("This file must be saved as {}", list.join(", "))))
}

/// Saves the temperatures as a 32-bit float TIFF in kelvin.
#[tauri::command]
pub(crate) async fn export_thermal_tiff(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::TIFF)?;
    let img = image(&state, id)?;
    not_a_source(Path::new(&path), img.path())?;
    let bytes = export::thermal_tiff(img.kelvin(), img.width(), img.height())?;
    Ok(std::fs::write(&path, bytes)?)
}

/// Saves a copy of a FLIR radiometric JPEG with the current parameters written into it.
#[tauri::command]
pub(crate) async fn save_flir_copy(id: u32, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::FLIR_JPEG)?;
    let img = image(&state, id)?;
    if !img.is_flir_jpeg() {
        return Err(CmdError::unsupported("Only FLIR radiometric JPEGs can be saved with new parameters"));
    }
    let (source, params) = (img.path().to_path_buf(), img.params());
    not_a_source(Path::new(&path), &source)?;
    let original = std::fs::read(&source)?;
    let copy = fff::rewrite_jpeg_params(&original, &params)?;
    Ok(std::fs::write(&path, copy)?)
}

#[tauri::command]
pub(crate) async fn export_csv(id: u32, path: String, unit: Unit, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::CSV)?;
    let img = image(&state, id)?;
    let csv = export::csv(img.kelvin(), img.width(), unit);
    Ok(std::fs::write(&path, csv)?)
}

/// Receives raw bytes from the UI (sent as the request body), kept until `write_staged`.
#[tauri::command]
pub(crate) fn stage_bytes(request: tauri::ipc::Request<'_>, state: State<'_, AppState>) -> CmdResult<usize> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CmdError::invalid_input("expected raw bytes"));
    };
    let len = bytes.len();
    state.lock()?.staged = Some(bytes.clone());
    Ok(len)
}

pub(crate) fn extension(path: &str) -> Option<String> {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Writes the staged bytes to an image file the user chose in a save dialog.
#[tauri::command]
pub(crate) async fn write_staged(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    checked_write_path(&path, write_types::IMAGE)?;
    let bytes = state.lock()?.staged.take().ok_or_else(|| CmdError::invalid_input("Nothing to save"))?;
    Ok(std::fs::write(&path, bytes)?)
}

/// Writes text the UI produced (settings, tables, maps) to a file the user chose.
#[tauri::command]
pub(crate) async fn write_text(path: String, text: String) -> CmdResult<()> {
    checked_write_path(&path, write_types::TEXT)?;
    Ok(std::fs::write(&path, text)?)
}

/// Reads a settings or template file the user picked.
#[tauri::command]
pub(crate) async fn read_text(path: String) -> CmdResult<String> {
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
pub(crate) async fn read_image_data_url(path: String) -> CmdResult<String> {
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

pub(crate) fn image_check(bytes: &[u8]) -> CmdResult<()> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8]) {
        Ok(())
    } else {
        Err(CmdError::invalid_file("Not a PNG or JPEG image"))
    }
}

/// Saves where the images were taken as KML or GeoJSON (by the file extension).
#[tauri::command]
pub(crate) async fn export_map(
    items: Vec<MapItem>,
    title: String,
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<usize> {
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
pub(crate) async fn save_sidecar(id: u32, edits: Sidecar, state: State<'_, AppState>) -> CmdResult<String> {
    let img = image(&state, id)?;
    let (path, format) = (img.path().to_path_buf(), img.format());
    if format == Format::Panorama {
        return Err(CmdError::unsupported("Save the panorama as a radiometric TIFF first (Export ▸ Radiometric TIFF)"));
    }
    Ok(sidecar::save(&path, &edits).map(|p| p.display().to_string())?)
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

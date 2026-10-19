//! Tauri shell: keeps opened images in memory and exposes the core library to the UI.
//! The app has no network access; files are only read or written at paths the user picked.

#[cfg(debug_assertions)]
mod dev;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kelvinlens_core::report::{self, ReportImage, ReportInfo};
use kelvinlens_core::sidecar::{self, Sidecar};
use kelvinlens_core::thermogram::{Camera, Pip};
use kelvinlens_core::{
    Format, Measurement, Param, Params, Shape, Stats, ThermalImage, Unit, export, folder, measure, palette,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Response;

#[derive(Default)]
struct Store {
    next_id: u32,
    images: HashMap<u32, ThermalImage>,
    /// Bytes sent by the UI ahead of `write_staged` (exported PNGs).
    staged: Option<Vec<u8>>,
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
    range: Option<[f32; 2]>,
    photo_size: Option<[usize; 2]>,
    pip: Option<Pip>,
    sidecar: Option<Sidecar>,
    sidecar_error: Option<String>,
}

fn info(id: u32, img: &ThermalImage) -> ImageInfo {
    let (sidecar, sidecar_error) = match sidecar::load(img.path()) {
        Ok(s) => (s, None),
        Err(e) => (None, Some(e.to_string())),
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
        range: img.range(),
        photo_size: img.photo().map(|p| [p.width, p.height]),
        pip: img.pip(),
        sidecar,
        sidecar_error,
    }
}

#[tauri::command]
async fn open_image(path: String, state: State<'_, AppState>) -> CmdResult<ImageInfo> {
    let image = ThermalImage::open(Path::new(&path)).map_err(err)?;
    let mut store = state.lock().map_err(err)?;
    store.next_id += 1;
    let id = store.next_id;
    let out = info(id, &image);
    store.images.insert(id, image);
    Ok(out)
}

#[tauri::command]
async fn close_image(id: u32, state: State<'_, AppState>) -> CmdResult<()> {
    state.lock().map_err(err)?.images.remove(&id);
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

/// Writes the staged bytes to an image file the user chose in a save dialog.
#[tauri::command]
async fn write_staged(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let ext = Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase());
    if !matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err("Exported images must be saved as .png or .jpg".into());
    }
    let bytes = state.lock().map_err(err)?.staged.take().ok_or("Nothing to save")?;
    std::fs::write(&path, bytes).map_err(err)
}

#[tauri::command]
async fn save_sidecar(id: u32, edits: Sidecar, state: State<'_, AppState>) -> CmdResult<String> {
    let path = with_image(&state, id, |img| Ok(img.path().to_path_buf()))?;
    sidecar::save(&path, &edits).map(|p| p.display().to_string()).map_err(err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportItem {
    id: u32,
    palette: String,
    range: [f32; 2],
    measurements: Vec<Measurement>,
    notes: String,
}

#[tauri::command]
async fn make_report(
    info: ReportInfo,
    items: Vec<ReportItem>,
    path: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let generator = format!("Kelvinlens {}", app.package_info().version);
    let pdf = {
        let store = state.lock().map_err(err)?;
        let mut images = Vec::with_capacity(items.len());
        for item in items {
            let image = store.images.get(&item.id).ok_or("An image in the report is no longer open")?;
            let palette = palette::by_id(&item.palette).ok_or_else(|| format!("Unknown palette {}", item.palette))?;
            images.push(ReportImage {
                image,
                palette,
                range: item.range,
                measurements: item.measurements,
                notes: item.notes,
            });
        }
        report::pdf(&info, &images, &generator).map_err(err)?
    };
    std::fs::write(PathBuf::from(path), pdf).map_err(err)
}

/// Files passed on the command line (also how "Open with" works on Linux and Windows).
#[tauri::command]
fn initial_paths() -> Vec<String> {
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
}

/// `--self-test <file>` opens a thermogram, prints its size and centre temperature, and exits
/// without creating a window. CI runs it on the packaged binary of every OS.
fn self_test(path: &str) -> i32 {
    match ThermalImage::open(Path::new(path)) {
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
                list_folder,
                is_folder,
                palettes,
                export_csv,
                stage_bytes,
                write_staged,
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

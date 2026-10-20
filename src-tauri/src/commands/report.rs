//! Report generation.

use std::path::PathBuf;
use std::sync::Arc;

use kelvinlens_core::{
    ThermalImage,
    report::{self, ReportImage, ReportInfo},
};
use tauri::State;

use crate::commands::files::{checked_write_path, write_types};
use crate::dto::ReportItem;
use crate::state::{AppState, CmdResult};

#[tauri::command]
pub(crate) async fn make_report(
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
    // Resolve the images under the lock, then drop it before generating.
    let held: Vec<Arc<ThermalImage>> = {
        let store = state.lock()?;
        items
            .iter()
            .map(|it| store.image_or(it.id, "An image in the report is no longer open"))
            .collect::<CmdResult<_>>()?
    };
    let images: Vec<ReportImage> = items
        .into_iter()
        .zip(&held)
        .map(|(item, image)| ReportImage {
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
        })
        .collect();
    let bytes = report::generate(&info, &images, &generator)?;
    Ok(std::fs::write(PathBuf::from(path), bytes)?)
}

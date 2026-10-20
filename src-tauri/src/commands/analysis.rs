//! Measurements, outlines, profiles, coverage, formulas and automatic inspection.

use kelvinlens_core::{
    Measurement, Shape, Stats, Unit,
    analysis::{self, Finding, InspectSettings, Isotherm},
    formula::{self, Formula, FormulaResult},
    measure::{self, ProfilePoint},
};
use tauri::State;

use crate::state::{AppState, CmdResult, image};

#[tauri::command]
pub(crate) async fn measure(id: u32, shapes: Vec<Shape>, state: State<'_, AppState>) -> CmdResult<Vec<Option<Stats>>> {
    let img = image(&state, id)?;
    Ok(measure::stats_each(img.kelvin(), img.width(), img.height(), &shapes))
}

/// Outline loops of magic-wand areas (empty for other tools), in thermal pixels.
#[tauri::command]
pub(crate) async fn outlines(
    id: u32,
    shapes: Vec<Shape>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<Vec<Vec<[f32; 2]>>>> {
    let img = image(&state, id)?;
    let (k, w, h) = (img.kelvin(), img.width(), img.height());
    Ok(shapes.iter().map(|s| measure::outline_of(s, k, w, h)).collect())
}

#[tauri::command]
pub(crate) async fn profile(id: u32, shape: Shape, state: State<'_, AppState>) -> CmdResult<Vec<ProfilePoint>> {
    let img = image(&state, id)?;
    Ok(measure::profile(img.kelvin(), img.width(), img.height(), &shape))
}

/// Share of each area shape's pixels matched by any of the isotherms (`None` for non-areas).
#[tauri::command]
pub(crate) async fn coverage(
    id: u32,
    shapes: Vec<Shape>,
    isotherms: Vec<Isotherm>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<Option<f32>>> {
    let img = image(&state, id)?;
    Ok(analysis::coverage_by_shape(img.kelvin(), img.width(), img.height(), &shapes, &isotherms))
}

#[tauri::command]
pub(crate) async fn evaluate_formulas(
    id: u32,
    measurements: Vec<Measurement>,
    formulas: Vec<Formula>,
    unit: Unit,
    state: State<'_, AppState>,
) -> CmdResult<Vec<FormulaResult>> {
    let img = image(&state, id)?;
    Ok(formula::evaluate_for_image(img.kelvin(), img.width(), img.height(), &measurements, &formulas, unit))
}

#[tauri::command]
pub(crate) async fn inspect(id: u32, settings: InspectSettings, state: State<'_, AppState>) -> CmdResult<Vec<Finding>> {
    let img = image(&state, id)?;
    Ok(analysis::inspect(img.kelvin(), img.width(), img.height(), &settings))
}

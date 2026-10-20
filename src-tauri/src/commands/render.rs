//! Colouring and fusing a thermal image.

use kelvinlens_core::render::{self, FuseRequest, Fusion, Look};
use tauri::State;
use tauri::ipc::Response;

use crate::cmd_error::CmdError;
use crate::state::{AppState, CmdResult, image};

/// The thermal image coloured with `look`, as RGBA (4 bytes per pixel).
#[tauri::command]
pub(crate) async fn render(id: u32, look: Look, state: State<'_, AppState>) -> CmdResult<Response> {
    let look = look.normalized();
    let img = image(&state, id)?;
    Ok(Response::new(render::rgba(&render::rgb(img.kelvin(), img.width(), img.height(), &look))))
}

/// A thermal/photo composite in the wire layout of `render::Composite::to_wire`.
#[tauri::command]
pub(crate) async fn fuse(
    id: u32,
    look: Look,
    fusion: Fusion,
    max_width: usize,
    align: Option<[f32; 3]>,
    state: State<'_, AppState>,
) -> CmdResult<Response> {
    let look = look.normalized();
    let img = image(&state, id)?;
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
}

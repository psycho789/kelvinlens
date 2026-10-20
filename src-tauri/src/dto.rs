//! Types that cross the IPC boundary (serialised to or from the UI).

use kelvinlens_core::{
    Format, Measurement, Param, Params, Stats, ThermalImage,
    analysis::Finding,
    formula::Formula,
    render::Look,
    sidecar::{self, Annotation, Presentation, Sidecar},
    thermogram::{Camera, Pip},
    triage,
    video::{Video, VideoInfo},
};
use serde::{Deserialize, Serialize};

/// Everything the UI needs about an image except the pixel data.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageInfo {
    pub(crate) id: u32,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) format: Format,
    pub(crate) format_label: &'static str,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) params: Params,
    pub(crate) file_params: Params,
    pub(crate) editable: Vec<Param>,
    pub(crate) camera: Camera,
    pub(crate) embedded_measurements: Vec<Measurement>,
    pub(crate) embedded_range: Option<[f32; 2]>,
    pub(crate) embedded_palette: Option<Vec<[u8; 3]>>,
    pub(crate) range: Option<[f32; 2]>,
    pub(crate) photo_size: Option<[usize; 2]>,
    pub(crate) pip: Option<Pip>,
    /// Whether "Save FLIR copy with these parameters" works for this file.
    pub(crate) flir_jpeg: bool,
    pub(crate) video: Option<VideoInfo>,
    pub(crate) sidecar: Option<Sidecar>,
    pub(crate) sidecar_error: Option<String>,
}

pub(crate) fn info(id: u32, img: &ThermalImage, video: Option<&Video>, unsaved: bool) -> ImageInfo {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recomputed {
    pub(crate) params: Params,
    pub(crate) range: Option<[f32; 2]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TriageSettings {
    /// Flag images hotter than this (kelvin) …
    pub(crate) above_k: Option<f32>,
    /// … or colder than this.
    pub(crate) below_k: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TriageRow {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) error: Option<String>,
    pub(crate) min_k: Option<f32>,
    pub(crate) max_k: Option<f32>,
    pub(crate) flagged: bool,
    pub(crate) date_time: Option<String>,
}

impl From<triage::Row> for TriageRow {
    fn from(r: triage::Row) -> Self {
        TriageRow {
            path: r.path.display().to_string(),
            name: r.name,
            error: r.error,
            min_k: r.min_k,
            max_k: r.max_k,
            flagged: r.flagged,
            date_time: r.date_time,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StitchResult {
    pub(crate) info: ImageInfo,
    pub(crate) offsets: Vec<[i64; 2]>,
    pub(crate) scores: Vec<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Series {
    pub(crate) rows: Vec<Vec<Option<Stats>>>,
    pub(crate) times: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapItem {
    pub(crate) id: u32,
    pub(crate) description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportItem {
    pub(crate) id: u32,
    pub(crate) look: Look,
    pub(crate) measurements: Vec<Measurement>,
    pub(crate) notes: String,
    #[serde(default)]
    pub(crate) formulas: Vec<Formula>,
    #[serde(default)]
    pub(crate) annotations: Vec<Annotation>,
    #[serde(default)]
    pub(crate) findings: Vec<Finding>,
    #[serde(default)]
    pub(crate) fields: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) asset: String,
    #[serde(default)]
    pub(crate) presentation: Presentation,
}

//! Per-image edits saved next to the image as `<file name>.kelvinlens.json`, so they travel
//! with the photo and the original file is never modified.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::analysis::{Finding, Isotherm};
use crate::formula::Formula;
use crate::render::Distribution;
use crate::{Error, Measurement, Params, Result};

pub const SUFFIX: &str = ".kelvinlens.json";
/// 1: params, measurements, notes, palette, range. 2 adds everything else below; 1 files load
/// unchanged.
pub const VERSION: u32 = 2;

/// A note or drawing on the image, in thermal-pixel coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Annotation {
    /// A text label pinned at a point.
    Text { x: f32, y: f32, text: String },
    /// A freehand stroke.
    Sketch { points: Vec<[f32; 2]>, color: [u8; 3] },
}

/// How the image is presented: rotation (degrees clockwise, any angle) and a crop rectangle in
/// thermal pixels `[x, y, w, h]`. Temperatures and measurement coordinates are unaffected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Presentation {
    pub rotation: f32,
    pub crop: Option<[f32; 4]>,
    /// Manual thermal-on-photo alignment `[dx, dy, scale]` (see `render::Alignment`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<[f32; 3]>,
}

impl Presentation {
    pub fn is_identity(&self) -> bool {
        self.rotation.rem_euclid(360.0) == 0.0 && self.crop.is_none() && self.align.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sidecar {
    pub version: u32,
    /// Only the parameters the user changed; absent ones keep the file's values.
    pub params: Option<Params>,
    pub measurements: Vec<Measurement>,
    pub notes: String,
    pub palette: Option<String>,
    /// Display range in kelvin; absent means automatic.
    pub range: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<Distribution>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub isotherms: Vec<Isotherm>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub formulas: Vec<Formula>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    /// Equipment or location tag that groups images of the same thing across visits (trends).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub asset: String,
    /// Inspection fields a report template asks for (e.g. "Load %", "Component").
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Presentation::is_identity")]
    pub presentation: Presentation,
}

pub fn path_for(image: &Path) -> PathBuf {
    let mut name = image.file_name().unwrap_or_default().to_os_string();
    name.push(SUFFIX);
    image.with_file_name(name)
}

/// The saved edits for `image`, or `None` if there are none.
pub fn load(image: &Path) -> Result<Option<Sidecar>> {
    let path = path_for(image);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let sidecar: Sidecar = serde_json::from_str(&text)
        .map_err(|e| Error::Open(format!("{} is not a valid edits file: {e}", path.display())))?;
    Ok(Some(sidecar))
}

pub fn save(image: &Path, sidecar: &Sidecar) -> Result<PathBuf> {
    let path = path_for(image);
    let mut sidecar = sidecar.clone();
    sidecar.version = VERSION;
    let json = serde_json::to_string_pretty(&sidecar).map_err(|e| Error::Encode(e.to_string()))?;
    std::fs::write(&path, json + "\n")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shape;
    use crate::analysis::IsothermKind;

    #[test]
    fn path_appends_suffix() {
        assert_eq!(path_for(Path::new("/x/FLIR1.jpg")), PathBuf::from("/x/FLIR1.jpg.kelvinlens.json"));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("kelvinlens-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("a.jpg");
        assert_eq!(load(&image).unwrap(), None);

        let edits = Sidecar {
            params: Some(Params { emissivity: Some(0.9), ..Default::default() }),
            measurements: vec![Measurement::new("Sp1", Shape::Spot { x: 1.5, y: 2.5 })],
            notes: "Breaker 3 hot".into(),
            palette: Some("iron".into()),
            range: Some([290.0, 310.0]),
            distribution: Some(Distribution::Histogram),
            isotherms: vec![Isotherm { kind: IsothermKind::Above { t: 330.0 }, color: [255, 0, 0], opacity: 1.0 }],
            formulas: vec![Formula { label: "Rise".into(), expression: "Sp1 - 20".into(), ..Default::default() }],
            annotations: vec![
                Annotation::Text { x: 3.0, y: 4.0, text: "Loose lug".into() },
                Annotation::Sketch { points: vec![[0.0, 0.0], [5.0, 5.0]], color: [255, 255, 0] },
            ],
            asset: "Panel B / breaker 3".into(),
            fields: BTreeMap::from([("Load %".to_string(), "80".to_string())]),
            presentation: Presentation {
                rotation: 90.0,
                crop: Some([1.0, 2.0, 30.0, 20.0]),
                align: Some([2.0, -1.0, 1.02]),
            },
            ..Default::default()
        };
        save(&image, &edits).unwrap();
        let loaded = load(&image).unwrap().unwrap();
        assert_eq!(loaded, Sidecar { version: VERSION, ..edits });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn version_1_files_still_load() {
        let v1 = r#"{"version":1,"params":null,"measurements":[{"label":"Sp1","shape":{"kind":"spot","x":1.5,"y":2.5}}],"notes":"x","palette":"iron","range":null}"#;
        let s: Sidecar = serde_json::from_str(v1).unwrap();
        assert_eq!(s.notes, "x");
        assert_eq!(s.measurements.len(), 1);
        assert!(s.isotherms.is_empty() && s.presentation.is_identity());
    }

    #[test]
    fn missing_fields_default_and_empty_extras_are_not_written() {
        let s: Sidecar = serde_json::from_str(r#"{"notes":"x"}"#).unwrap();
        assert_eq!(s.notes, "x");
        assert!(s.measurements.is_empty() && s.params.is_none());
        let json = serde_json::to_string(&Sidecar::default()).unwrap();
        assert!(!json.contains("isotherms") && !json.contains("presentation") && !json.contains("asset"), "{json}");
    }

    #[test]
    fn findings_saved_by_earlier_versions_still_load() {
        use crate::analysis::SeverityLevel;
        let finding = |level: u8| {
            format!(
                r#"{{"label":"A","hot":true,"area":{{"kind":"spot","x":1.0,"y":1.0}},"pixels":1,"extremeK":300.0,"at":[1,1],"referenceK":290.0,"deltaK":10.0,"thermalIndex":null,"severity":{{"level":{level},"label":"l","action":"a"}}}}"#
            )
        };
        let json = format!(r#"{{"version":2,"findings":[{},{}]}}"#, finding(3), finding(0));
        let s: Sidecar = serde_json::from_str(&json).unwrap();
        let levels: Vec<_> = s.findings.iter().map(|f| f.severity.level).collect();
        assert_eq!(levels, [SeverityLevel::High, SeverityLevel::Low]);
        assert!(serde_json::to_string(&s).unwrap().contains(r#""level":3"#));
    }
}

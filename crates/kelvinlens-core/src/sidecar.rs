//! Per-image edits saved next to the image as `<file name>.kelvinlens.json`, so they travel
//! with the photo and the original file is never modified.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Measurement, Params, Result};

pub const SUFFIX: &str = ".kelvinlens.json";
pub const VERSION: u32 = 1;

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
            measurements: vec![Measurement { label: "Sp1".into(), shape: Shape::Spot { x: 1.5, y: 2.5 } }],
            notes: "Breaker 3 hot".into(),
            palette: Some("iron".into()),
            range: Some([290.0, 310.0]),
            ..Default::default()
        };
        save(&image, &edits).unwrap();
        let loaded = load(&image).unwrap().unwrap();
        assert_eq!(loaded, Sidecar { version: VERSION, ..edits });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_fields_default() {
        let s: Sidecar = serde_json::from_str(r#"{"notes":"x"}"#).unwrap();
        assert_eq!(s.notes, "x");
        assert!(s.measurements.is_empty() && s.params.is_none());
    }
}

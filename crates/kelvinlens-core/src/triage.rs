//! Triage: a quick pass over many files, reporting each one's hottest and coldest temperature.

use std::path::{Path, PathBuf};

use crate::video::{self, Video};
use crate::{ThermalImage, folder, sidecar};

/// What triage found out about one file.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub error: Option<String>,
    pub min_k: Option<f32>,
    pub max_k: Option<f32>,
    pub flagged: bool,
    pub date_time: Option<String>,
}

/// Files of `paths`, with folders expanded to their supported files.
pub fn expand(paths: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            files.extend(folder::candidates(path)?);
        } else {
            files.push(path.clone());
        }
    }
    Ok(files)
}

/// `true` when the range is hotter than `above_k` or colder than `below_k`.
pub fn is_flagged(range: Option<[f32; 2]>, above_k: Option<f32>, below_k: Option<f32>) -> bool {
    range.is_some_and(|[lo, hi]| above_k.is_some_and(|a| hi > a) || below_k.is_some_and(|b| lo < b))
}

/// Opens `path` as it would look in the app (saved parameter edits applied) and summarises it.
pub fn summarize(path: &Path, above_k: Option<f32>, below_k: Option<f32>) -> Row {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // As the image would look when opened: the saved parameter edits applied.
    let saved = sidecar::load(path).ok().flatten().and_then(|s| s.params);
    let opened = if video::is_video(path) {
        Video::open(path).and_then(|v| v.frame(0, saved.as_ref()))
    } else {
        ThermalImage::open_temperatures(path).and_then(|mut img| {
            if let Some(p) = &saved
                && !img.editable().is_empty()
            {
                img.set_params(p)?;
            }
            Ok(img)
        })
    };
    match opened {
        Ok(img) => {
            let r = img.range();
            Row {
                path: path.to_path_buf(),
                name,
                error: None,
                min_k: r.map(|r| r[0]),
                max_k: r.map(|r| r[1]),
                flagged: is_flagged(r, above_k, below_k),
                date_time: img.camera().date_time.clone(),
            }
        }
        Err(e) => Row {
            path: path.to_path_buf(),
            name,
            error: Some(e.to_string().replace("Could not open thermogram: ", "")),
            min_k: None,
            max_k: None,
            flagged: false,
            date_time: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_flagged_above_below_and_none() {
        assert!(is_flagged(Some([280.0, 320.0]), Some(310.0), None));
        assert!(!is_flagged(Some([280.0, 300.0]), Some(310.0), None));
        assert!(is_flagged(Some([270.0, 300.0]), None, Some(275.0)));
        assert!(!is_flagged(Some([280.0, 300.0]), None, Some(275.0)));
        assert!(!is_flagged(Some([280.0, 300.0]), None, None));
        assert!(!is_flagged(None, Some(0.0), Some(1000.0)));
    }

    #[test]
    fn summarize_reports_error_for_missing_file() {
        let row = summarize(Path::new("/nonexistent/x.jpg"), Some(300.0), None);
        assert!(row.error.is_some());
        assert!(!row.flagged);
        assert!(row.min_k.is_none());
        assert_eq!(row.name, "x.jpg");
    }

    #[test]
    fn expand_lists_supported_files_in_a_folder() {
        let dir = std::env::temp_dir().join(format!("kelvinlens-triage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"").unwrap();
        std::fs::write(dir.join("b.txt"), b"").unwrap();
        let single = PathBuf::from("/some/file.png");
        let out = expand(&[dir.clone(), single.clone()]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(out, vec![dir.join("a.jpg"), single]);
    }
}

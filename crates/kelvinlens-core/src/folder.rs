//! Finding thermograms in a folder.

use std::path::{Path, PathBuf};

/// File extensions that may hold a supported thermogram.
const EXTENSIONS: [&str; 9] = ["jpg", "jpeg", "is2", "irg", "tif", "tiff", "png", "seq", "csq"];

/// Candidate files directly inside `dir`, sorted by name. Whether each one really is a
/// thermogram is only known after opening it.
pub fn candidates(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && has_supported_extension(p))
        .collect();
    out.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    Ok(out)
}

/// Whether `path` ends in one of `extensions` (lower case, without the dot), ignoring case.
pub fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension().is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn has_supported_extension(path: &Path) -> bool {
    let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
    !hidden && has_extension(path, &EXTENSIONS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_extension_ignores_case_and_needs_a_whole_extension() {
        let exts = ["png", "jpg"];
        assert!(has_extension(Path::new("a/logo.PNG"), &exts));
        assert!(has_extension(Path::new("photo.Jpg"), &exts));
        assert!(!has_extension(Path::new("photo.jpeg"), &exts));
        assert!(!has_extension(Path::new("png"), &exts));
        assert!(!has_extension(Path::new("noextension"), &exts));
    }

    #[test]
    fn filters_by_extension_and_skips_hidden() {
        assert!(has_supported_extension(Path::new("a/FLIR0001.JPG")));
        assert!(has_supported_extension(Path::new("IR_0001.is2")));
        assert!(!has_supported_extension(Path::new("notes.txt")));
        assert!(!has_supported_extension(Path::new("._FLIR0001.jpg")));
        assert!(!has_supported_extension(Path::new("FLIR0001.jpg.kelvinlens.json")));
    }
}

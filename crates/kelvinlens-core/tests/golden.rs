//! Golden tests on real camera files (fetched by `scripts/fetch-testdata.sh`, MIT-licensed
//! samples from SanNianYiSi/thermal_parser).
//!
//! "Camera" values are what the camera itself displayed or stored. "Reference" values come from
//! an independent Python implementation of the published FLIR model (exiftool + numpy), run with
//! the parameters shown; it agreed with libblackbody to 0.01 °C on all three files.

use std::path::PathBuf;

use kelvinlens_core::report::{self, ReportImage, ReportInfo};
use kelvinlens_core::{Format, Measurement, Params, Shape, ThermalImage, measure, palette, sidecar};

fn sample(name: &str) -> ThermalImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata").join(name);
    assert!(path.exists(), "{} is missing: run scripts/fetch-testdata.sh first", path.display());
    ThermalImage::open(&path).expect("sample opens")
}

fn c(img: &ThermalImage, x: usize, y: usize) -> f32 {
    img.kelvin_at(x, y).unwrap() - 273.15
}

fn whole(img: &ThermalImage) -> (f32, f32, f32) {
    let s = measure::stats(
        img.kelvin(),
        img.width(),
        img.height(),
        &Shape::Box { x: 0.0, y: 0.0, w: img.width() as f32, h: img.height() as f32 },
    )
    .unwrap();
    (s.min - 273.15, s.max - 273.15, s.avg - 273.15)
}

fn close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!((actual - expected).abs() <= tolerance, "{what}: got {actual:.3}, expected {expected} ± {tolerance}");
}

/// The test changes these; the reference model was run with exactly the same values.
fn changed() -> Params {
    Params {
        emissivity: Some(0.80),
        reflected_k: Some(30.0 + 273.15),
        distance_m: Some(10.0),
        humidity: Some(0.80),
        atmosphere_k: Some(5.0 + 273.15),
        ..Default::default()
    }
}

#[test]
fn e40_matches_camera_spot() {
    let img = sample("FLIR_E40.jpg");
    assert_eq!((img.format(), img.width(), img.height()), (Format::Flir, 160, 120));
    // Camera shows 20.9 °C at its stored spot (80, 60).
    close(c(&img, 80, 60), 20.9, 0.05, "E40 spot");
    let spot = img.embedded_measurements().iter().find(|m| matches!(m.shape, Shape::Spot { .. }));
    assert_eq!(spot.map(|m| &m.shape), Some(&Shape::Spot { x: 80.5, y: 60.5 }));
}

#[test]
fn ax8_matches_camera_box_values() {
    let img = sample("FLIR_AX8.jpg");
    let (min, max, avg) = whole(&img);
    // Camera: max 25.5, min 24.3, avg 25.1 (displayed to 0.1 °C).
    close(max, 25.5, 0.1, "AX8 max");
    close(min, 24.3, 0.1, "AX8 min");
    close(avg, 25.1, 0.1, "AX8 avg");
}

#[test]
fn b60_matches_camera_spot() {
    let img = sample("FLIR_B60.jpg");
    // Camera shows 19.0 °F = −7.22 °C. The centre pixel reads −7.34; the 3×3 mean −7.26.
    close(c(&img, 90, 90), -7.34, 0.02, "B60 centre (reference)");
    let s = measure::stats(img.kelvin(), 180, 180, &Shape::Box { x: 89.0, y: 89.0, w: 3.0, h: 3.0 }).unwrap();
    close(s.avg - 273.15, -7.22, 0.1, "B60 3×3 vs camera");
}

#[test]
fn recompute_matches_independent_model() {
    let mut e40 = sample("FLIR_E40.jpg");
    e40.set_params(&changed()).unwrap();
    close(c(&e40, 80, 60), 18.86, 0.02, "E40 centre");
    close(c(&e40, 10, 10), 19.96, 0.02, "E40 (10,10)");
    let (min, max, avg) = whole(&e40);
    close(min, 15.12, 0.02, "E40 min");
    close(max, 23.48, 0.02, "E40 max");
    close(avg, 19.07, 0.02, "E40 avg");

    let mut ax8 = sample("FLIR_AX8.jpg");
    ax8.set_params(&changed()).unwrap();
    close(c(&ax8, 40, 30), 24.37, 0.02, "AX8 centre");
    let (min, max, avg) = whole(&ax8);
    close(min, 23.09, 0.02, "AX8 min");
    close(max, 24.44, 0.02, "AX8 max");
    close(avg, 23.90, 0.02, "AX8 avg");
}

#[test]
fn recompute_non_physical_pixels_become_nan_and_reset_restores() {
    let mut b60 = sample("FLIR_B60.jpg");
    let original = b60.kelvin().to_vec();
    b60.set_params(&changed()).unwrap();
    close(c(&b60, 90, 90), -19.77, 0.02, "B60 centre");
    // The reference model has no solution for the cold sky at these settings either.
    assert!(c(&b60, 10, 10).is_nan());
    assert!(b60.range().is_some(), "finite pixels remain");

    b60.reset_params().unwrap();
    assert_eq!(b60.kelvin(), original.as_slice());
    assert_eq!(b60.params(), b60.file_params());
}

#[test]
fn file_parameters_are_read() {
    let img = sample("FLIR_E40.jpg");
    let p = img.file_params();
    close(p.emissivity.unwrap(), 0.95, 1e-6, "emissivity");
    close(p.distance_m.unwrap(), 2.0, 1e-6, "distance");
    close(p.reflected_k.unwrap() - 273.15, 21.0, 0.01, "reflected");
    close(p.atmosphere_k.unwrap() - 273.15, 14.0, 0.01, "atmosphere");
    close(p.humidity.unwrap(), 0.49, 1e-3, "humidity");
    assert_eq!(img.editable().len(), 7);
}

#[test]
fn photos_are_decoded_where_the_file_has_one() {
    // Checked with exiftool: E40 has no embedded image, AX8 a 640×480 JPEG, B60 a 480×480 PNG.
    assert!(sample("FLIR_E40.jpg").photo().is_none());
    let ax8 = sample("FLIR_AX8.jpg");
    let p = ax8.photo().expect("AX8 photo");
    assert_eq!((p.width, p.height, p.rgb.len()), (640, 480, 640 * 480 * 3));
    let b60 = sample("FLIR_B60.jpg");
    let p = b60.photo().expect("B60 photo");
    assert_eq!((p.width, p.height), (480, 480));
}

#[test]
fn invalid_parameters_are_rejected_and_leave_image_unchanged() {
    let mut img = sample("FLIR_E40.jpg");
    let before = img.kelvin().to_vec();
    let bad = Params { emissivity: Some(1.5), ..Default::default() };
    assert!(img.set_params(&bad).is_err());
    assert_eq!(img.kelvin(), before.as_slice());
}

#[test]
fn report_pdf_is_produced() {
    let mut img = sample("FLIR_E40.jpg");
    img.set_params(&changed()).unwrap();
    let range = img.range().unwrap();
    let measurements = vec![
        Measurement { label: "Sp1".into(), shape: Shape::Spot { x: 80.5, y: 60.5 } },
        Measurement { label: "Bx1".into(), shape: Shape::Box { x: 20.0, y: 20.0, w: 40.0, h: 30.0 } },
        Measurement { label: "El1".into(), shape: Shape::Ellipse { cx: 120.0, cy: 80.0, rx: 15.0, ry: 10.0 } },
        Measurement { label: "Li1".into(), shape: Shape::Line { x1: 10.0, y1: 110.0, x2: 150.0, y2: 100.0 } },
    ];
    let info = ReportInfo {
        title: "Golden test report".into(),
        site: "Test bench".into(),
        inspector: "CI".into(),
        date: "2026-10-19".into(),
        summary: "Automated check that report generation works.".into(),
        include_photos: true,
        ..Default::default()
    };
    let ax8 = sample("FLIR_AX8.jpg");
    let items = [
        ReportImage {
            image: &img,
            palette: palette::by_id("iron").unwrap(),
            range,
            measurements,
            notes: "Recomputed with emissivity 0.80.".into(),
        },
        ReportImage {
            image: &ax8,
            palette: palette::by_id("rainbow").unwrap(),
            range: ax8.range().unwrap(),
            measurements: ax8.embedded_measurements().to_vec(),
            notes: String::new(),
        },
    ];
    let pdf = report::pdf(&info, &items, "Kelvinlens test").expect("report renders");
    assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
    assert!(pdf.len() > 20_000, "suspiciously small PDF: {} bytes", pdf.len());
    if let Ok(dir) = std::env::var("KELVINLENS_REPORT_OUT") {
        std::fs::write(PathBuf::from(dir).join("golden-report.pdf"), &pdf).unwrap();
    }
}

#[test]
fn sidecar_edits_apply_to_a_real_image() {
    let mut img = sample("FLIR_E40.jpg");
    let edits = sidecar::Sidecar { params: Some(changed()), ..Default::default() };
    img.set_params(edits.params.as_ref().unwrap()).unwrap();
    close(c(&img, 80, 60), 18.86, 0.02, "E40 centre after sidecar params");
}

//! Golden tests on real camera files (fetched by `scripts/fetch-testdata.sh`, MIT-licensed
//! samples from SanNianYiSi/thermal_parser).
//!
//! "Camera" values are what the camera itself displayed or stored. "Reference" values come from
//! an independent Python implementation of the published FLIR model (exiftool + numpy), run with
//! the parameters shown; it agreed with libblackbody to 0.01 °C on all files. Video references
//! decode the raw frame independently (exiftool, and CharLS through imagecodecs for JPEG-LS).

use std::collections::BTreeMap;
use std::path::PathBuf;

use kelvinlens_core::analysis::{self, InspectSettings, Isotherm, IsothermKind};
use kelvinlens_core::formula::{Formula, ResultKind};
use kelvinlens_core::measure::Alarm;
use kelvinlens_core::render::{Distribution, Look};
use kelvinlens_core::report::{self, ReportFormat, ReportImage, ReportInfo, TemplateKind};
use kelvinlens_core::sidecar::{Annotation, Presentation};
use kelvinlens_core::video::Video;
use kelvinlens_core::{
    Format, Measurement, Params, Shape, ThermalImage, export, fff, measure, palette, panorama, sidecar,
};

fn path(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata").join(name);
    assert!(path.exists(), "{} is missing: run scripts/fetch-testdata.sh first", path.display());
    path
}

fn sample(name: &str) -> ThermalImage {
    ThermalImage::open(&path(name)).expect("sample opens")
}

fn look(img: &ThermalImage, palette_id: &str) -> Look {
    Look {
        palette: palette::by_id(palette_id).unwrap(),
        range: img.range().unwrap(),
        distribution: Distribution::TemperatureLinear,
        isotherms: vec![],
    }
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
    let mut measurements = vec![
        Measurement::new("Sp1", Shape::Spot { x: 80.5, y: 60.5 }),
        Measurement::new("Bx1", Shape::Box { x: 20.0, y: 20.0, w: 40.0, h: 30.0 }),
        Measurement::new("El1", Shape::Ellipse { cx: 120.0, cy: 80.0, rx: 15.0, ry: 10.0 }),
        Measurement::new("Li1", Shape::Line { x1: 10.0, y1: 110.0, x2: 150.0, y2: 100.0 }),
        Measurement::new(
            "Po1",
            Shape::Polygon { points: vec![[70.0, 10.0], [100.0, 15.0], [95.0, 40.0], [72.0, 35.0]] },
        ),
        Measurement::new("Pl1", Shape::Polyline { points: vec![[5.0, 60.0], [60.0, 70.0], [100.0, 55.0]] }),
        Measurement::new("Wa1", Shape::Wand { x: 80.5, y: 60.5, tolerance: 0.5 }),
    ];
    measurements[1].alarm = Some(Alarm { stat: measure::Stat::Max, above: Some(range[0] + 1.0), below: None });
    let isotherms =
        vec![Isotherm { kind: IsothermKind::Above { t: range[1] - 1.5 }, color: [255, 40, 40], opacity: 1.0 }];
    let findings = analysis::inspect(
        img.kelvin(),
        img.width(),
        img.height(),
        &InspectSettings { min_delta: 2.0, ..Default::default() },
    );
    assert!(!findings.is_empty(), "the E40 scene has a warm object");
    let ax8 = sample("FLIR_AX8.jpg");
    let t640 = sample("FLIR_T640.jpg");
    let t640_look = Look {
        palette: palette::Palette {
            id: "camera".into(),
            name: "Camera palette".into(),
            colors: t640.embedded_palette().unwrap().to_vec(),
        },
        distribution: Distribution::Detail,
        ..look(&t640, "iron")
    };

    for template in report::builtin_templates() {
        for format in [ReportFormat::Pdf, ReportFormat::Html] {
            let info = ReportInfo {
                title: format!("Golden test report ({})", template.name),
                site: "Test bench".into(),
                inspector: "CI".into(),
                date: "2026-10-19".into(),
                summary: "Automated check that report generation works.".into(),
                include_photos: true,
                fields: BTreeMap::from([("Customer".to_string(), "ACME".to_string())]),
                conditions: report::Conditions {
                    inside_k: Some(293.15),
                    outside_k: Some(273.15),
                    humidity: Some(0.5),
                    ambient_k: Some(288.15),
                    ..Default::default()
                },
                template: Some(template.clone()),
                format,
                ..Default::default()
            };
            let items = [
                ReportImage {
                    formulas: vec![
                        Formula {
                            label: "Rise".into(),
                            expression: "Bx1.max - Sp1".into(),
                            kind: ResultKind::Difference,
                        },
                        Formula { label: "Broken".into(), expression: "Nope + 1".into(), kind: ResultKind::Number },
                    ],
                    annotations: vec![
                        Annotation::Text { x: 30.0, y: 90.0, text: "Check fixing".into() },
                        Annotation::Sketch {
                            points: vec![[10.0, 10.0], [30.0, 20.0], [50.0, 12.0]],
                            color: [255, 255, 0],
                        },
                    ],
                    findings: findings.clone(),
                    fields: BTreeMap::from([("Component".to_string(), "Mug".to_string())]),
                    asset: "Kitchen".into(),
                    presentation: Presentation { rotation: 90.0, crop: Some([10.0, 5.0, 140.0, 110.0]), align: None },
                    ..ReportImage::basic(
                        &img,
                        Look { isotherms: isotherms.clone(), ..look(&img, "iron") },
                        measurements.clone(),
                        "Recomputed with emissivity 0.80.".into(),
                    )
                },
                ReportImage {
                    asset: "Kitchen".into(),
                    ..ReportImage::basic(
                        &ax8,
                        look(&ax8, "rainbow"),
                        ax8.embedded_measurements().to_vec(),
                        String::new(),
                    )
                },
                ReportImage {
                    presentation: Presentation { rotation: 17.0, crop: None, align: None },
                    ..ReportImage::basic(
                        &t640,
                        t640_look.clone(),
                        vec![
                            Measurement::new("Supply 1", Shape::Spot { x: 320.5, y: 240.5 }),
                            Measurement::new("Return 1", Shape::Box { x: 10.0, y: 10.0, w: 50.0, h: 40.0 }),
                        ],
                        String::new(),
                    )
                },
            ];
            let out = report::generate(&info, &items, "Kelvinlens test")
                .unwrap_or_else(|e| panic!("{} {format:?}: {e}", template.id));
            match format {
                ReportFormat::Pdf => {
                    assert!(out.starts_with(b"%PDF-"), "not a PDF");
                    assert!(out.len() > 20_000, "suspiciously small PDF: {} bytes", out.len());
                }
                ReportFormat::Html => {
                    let html = String::from_utf8(out.clone()).unwrap();
                    assert!(html.starts_with("<!doctype html>") && html.ends_with("</html>\n"));
                    assert!(html.contains("Golden test report") && html.contains("data:image/png;base64,"));
                    assert!(!html.contains("<script"), "reports must not contain scripts");
                    if template.kind == TemplateKind::Electrical {
                        assert!(html.contains("Temperature rise (NETA)"));
                    }
                    if template.kind == TemplateKind::Building {
                        assert!(html.contains("Thermal index") && html.contains("Dew point"));
                    }
                    if template.kind == TemplateKind::Hvac {
                        assert!(html.contains("Split (return − supply)"));
                    }
                    if template.sections.trends {
                        assert!(html.contains("Trend: Kitchen"));
                    }
                }
            }
            if let Ok(dir) = std::env::var("KELVINLENS_REPORT_OUT") {
                let ext = if format == ReportFormat::Pdf { "pdf" } else { "html" };
                std::fs::write(PathBuf::from(dir).join(format!("golden-{}.{ext}", template.id)), &out).unwrap();
            }
        }
    }
}

#[test]
fn sidecar_edits_apply_to_a_real_image() {
    let mut img = sample("FLIR_E40.jpg");
    let edits = sidecar::Sidecar { params: Some(changed()), ..Default::default() };
    img.set_params(edits.params.as_ref().unwrap()).unwrap();
    close(c(&img, 80, 60), 18.86, 0.02, "E40 centre after sidecar params");
}

#[test]
fn more_cameras_match_independent_model() {
    // DJI Zenmuse XT2 and XTR record FLIR data; the T640 and the portrait sample have palettes.
    for (name, w, h, centre, min, max) in [
        ("DJI_XT2.jpg", 640, 512, 34.47, 21.46, 82.92),
        ("DJI_XTR.jpg", 640, 512, 25.80, 15.93, 59.73),
        ("FLIR_T640.jpg", 640, 480, 34.45, 25.46, 97.85),
        ("FLIR.jpg", 240, 320, 30.50, 25.95, 62.32),
    ] {
        let img = sample(name);
        assert_eq!((img.width(), img.height()), (w, h), "{name}");
        close(c(&img, w / 2, h / 2), centre, 0.02, name);
        let (lo, hi, _) = whole(&img);
        close(lo, min, 0.02, name);
        close(hi, max, 0.02, name);
    }
    let t640 = sample("FLIR_T640.jpg");
    assert_eq!(t640.embedded_palette().map(|p| p.len()), Some(256));
    assert!(t640.camera().lens.is_some() && t640.camera().field_of_view.is_some(), "{:?}", t640.camera());
    let xt2 = sample("DJI_XT2.jpg");
    let g = xt2.camera().gimbal.expect("XT2 gimbal from DJI XMP");
    close(g[2], 82.4, 0.01, "gimbal yaw");
    assert_eq!(xt2.camera().relative_altitude, Some(1.9));
}

#[test]
fn radiometric_video_frames_match_independent_decoding() {
    let seq = Video::open(&path("SampleSEQ.seq")).unwrap();
    let info = seq.info();
    assert_eq!((info.frames, info.width, info.height, info.compressed), (2, 640, 480, false));
    // ExifTool: frame times 14:52:08.699 and 14:52:12.666.
    close(info.times[1] as f32, 3.967, 0.001, "SEQ frame 2 time");
    let f0 = seq.frame(0, None).unwrap();
    close(c(&f0, 320, 240), 22.364, 0.002, "SEQ frame 1 centre");
    let (lo, hi, _) = whole(&f0);
    close(lo, 18.60, 0.01, "SEQ min");
    close(hi, 38.27, 0.01, "SEQ max");
    assert_eq!(f0.camera().date_time.as_deref(), Some("2012-06-13 14:52:08.699-05:00"));

    let csq = Video::open(&path("SampleCSQ.csq")).unwrap();
    let info = csq.info();
    assert_eq!((info.width, info.height, info.compressed), (1024, 768, true));
    assert_eq!(info.frames, 8, "including the last frame, which is cut short by 4 bytes");
    let f0 = csq.frame(0, None).unwrap();
    close(c(&f0, 512, 384), 31.276, 0.002, "CSQ frame 1 centre");
    let (lo, hi, _) = whole(&f0);
    close(lo, 16.69, 0.01, "CSQ min");
    close(hi, 35.29, 0.01, "CSQ max");
    // Every frame decodes; neighbours differ (the scene moves).
    let centres: Vec<f32> = (0..info.frames).map(|i| c(&csq.frame(i, None).unwrap(), 512, 384)).collect();
    assert!(centres.iter().all(|v| v.is_finite() && *v > 20.0 && *v < 40.0), "{centres:?}");
    assert_ne!(centres[0], centres[1]);

    // Parameters apply to every frame; segments round-trip through SEQ.
    let p = Params { emissivity: Some(0.8), ..Default::default() };
    let changed = csq.frame(3, Some(&p)).unwrap();
    assert!(c(&changed, 512, 384) > centres[3]);
    let out = std::env::temp_dir().join(format!("kelvinlens-seg-{}.seq", std::process::id()));
    assert_eq!(csq.write_segment(2, 4, &out).unwrap(), 3);
    let seg = Video::open(&out).unwrap();
    assert_eq!(seg.len(), 3);
    assert_eq!(seg.frame(1, None).unwrap().kelvin(), csq.frame(3, None).unwrap().kelvin());
    std::fs::remove_file(&out).unwrap();
}

#[test]
fn flir_copy_with_new_parameters_recomputes_identically() {
    let original = std::fs::read(path("FLIR_E40.jpg")).unwrap();
    let copy = fff::rewrite_jpeg_params(&original, &changed()).unwrap();
    assert_eq!(copy.len(), original.len());
    let out = std::env::temp_dir().join(format!("kelvinlens-e40-{}.jpg", std::process::id()));
    std::fs::write(&out, &copy).unwrap();
    let reopened = ThermalImage::open(&out).unwrap();
    std::fs::remove_file(&out).unwrap();
    let p = reopened.file_params();
    close(p.emissivity.unwrap(), 0.80, 1e-6, "emissivity in copy");
    close(p.humidity.unwrap(), 0.80, 1e-6, "humidity in copy");
    close(p.distance_m.unwrap(), 10.0, 1e-6, "distance in copy");
    // Same temperatures as recomputing the original in the app.
    close(c(&reopened, 80, 60), 18.86, 0.02, "E40 copy centre");
    let mut recomputed = sample("FLIR_E40.jpg");
    recomputed.set_params(&changed()).unwrap();
    assert_eq!(reopened.kelvin(), recomputed.kelvin());
}

#[test]
fn panorama_of_overlapping_crops_rebuilds_the_scene() {
    let img = sample("FLIR_T640.jpg");
    let (w, k) = (img.width(), img.kelvin());
    let crop = |x0: usize, y0: usize, cw: usize, ch: usize| -> Vec<f32> {
        (0..ch).flat_map(|y| (0..cw).map(move |x| k[(y0 + y) * w + x0 + x])).collect()
    };
    let (a, b, cc) = (crop(0, 40, 300, 360), crop(210, 70, 300, 360), crop(330, 20, 300, 360));
    let tiles = [
        panorama::Tile { kelvin: &a, width: 300, height: 360 },
        panorama::Tile { kelvin: &b, width: 300, height: 360 },
        panorama::Tile { kelvin: &cc, width: 300, height: 360 },
    ];
    let s = panorama::stitch(&tiles, &[]).unwrap();
    assert_eq!(s.offsets, vec![[0, 20], [210, 50], [330, 0]]);
    for y in 0..s.height {
        for x in 0..s.width {
            let v = s.kelvin[y * s.width + x];
            if v.is_finite() {
                assert!((v - k[(y + 20) * w + x]).abs() < 1e-3);
            }
        }
    }
    let pano = ThermalImage::from_kelvin(
        &PathBuf::from("pano.tiff"),
        Format::Panorama,
        s.kelvin.clone(),
        s.width,
        s.height,
        Default::default(),
    )
    .unwrap();
    // Saved as a radiometric TIFF it opens again with the same temperatures.
    let tiff = export::thermal_tiff(pano.kelvin(), pano.width(), pano.height()).unwrap();
    let out = std::env::temp_dir().join(format!("kelvinlens-pano-{}.tiff", std::process::id()));
    std::fs::write(&out, tiff).unwrap();
    let back = ThermalImage::open(&out).unwrap();
    std::fs::remove_file(&out).unwrap();
    assert_eq!((back.width(), back.height()), (s.width, s.height));
    assert!(back.kelvin().iter().zip(pano.kelvin()).all(|(a, b)| a == b || (a.is_nan() && b.is_nan())));
}

#[test]
fn auto_inspect_finds_the_hottest_spot_of_a_real_image() {
    let img = sample("FLIR_T640.jpg");
    let f = analysis::inspect(
        img.kelvin(),
        img.width(),
        img.height(),
        &InspectSettings { min_delta: 10.0, ..Default::default() },
    );
    assert!(!f.is_empty());
    let (_, max, _) = whole(&img);
    // The top finding contains the image's hottest pixel.
    let hottest = f.iter().map(|f| f.extreme_k).fold(f32::MIN, f32::max);
    close(hottest - 273.15, max, 0.001, "hottest finding");
    assert!(f.iter().all(|f| f.delta_k >= 10.0));
}

/// InfiRay P2 Pro photos. "Camera" values are the readings the P2 Pro app printed on each photo;
/// "reference" values come from an independent Python decode (exiftool -ImagingData + numpy,
/// temperatures as u16/64 K, turned clockwise to match the upright picture).
#[test]
fn p2_pro_matches_camera_readings() {
    for (name, centre, max, max_at, reference_max, reference_avg) in [
        // Photo a: the camera showed centre 23.4 °C and max 34.3 °C at picture pixel (416, 656).
        ("P2Pro_a.jpg", 23.4, Some(34.3), (110.9, 174.9), 34.288, 21.85),
        // Photo b: centre 22.1 °C; its printed max (36.1 °C) differs from the stored data
        // (36.54 °C) at the same spot, so only the position is checked against the camera.
        ("P2Pro_b.jpg", 22.1, None, (150.1, 221.6), 36.538, 23.303),
    ] {
        let img = sample(name);
        assert_eq!((img.format(), img.width(), img.height()), (Format::InfiRay, 192, 256), "{name}");
        assert_eq!(img.camera().description.as_deref(), Some("Infisense P2_USB_IR"));
        assert_eq!(img.camera().serial_number.as_deref(), Some("P200019B15656148"));

        let spot = &img.embedded_measurements()[0];
        assert_eq!(spot.label, "Centre");
        let s = measure::stats(img.kelvin(), img.width(), img.height(), &spot.shape).unwrap();
        close(s.avg - 273.15, centre, 0.05, &format!("{name} centre"));

        let (_, hottest, avg) = whole(&img);
        close(hottest, reference_max, 0.002, &format!("{name} max vs reference"));
        close(avg, reference_avg, 0.01, &format!("{name} mean vs reference"));
        if let Some(max) = max {
            close(hottest, max, 0.05, &format!("{name} max vs camera"));
        }
        // The hottest pixel is where the camera put its max marker (picture is 3.75× larger).
        let i = img.kelvin().iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let (x, y) = ((i % img.width()) as f32 + 0.5, (i / img.width()) as f32 + 0.5);
        assert!((x - max_at.0).abs() < 2.0 && (y - max_at.1).abs() < 2.0, "{name}: max at ({x}, {y})");
    }
}

#[test]
fn p2_pro_parameters_are_read_and_can_be_changed() {
    let mut img = sample("P2Pro_a.jpg");
    let p = img.file_params();
    close(p.emissivity.unwrap(), 0.99, 1e-6, "emissivity");
    close(p.reflected_k.unwrap() - 273.15, 25.0, 1e-4, "reflected");
    close(p.atmosphere_k.unwrap() - 273.15, 25.0, 1e-4, "environment");
    close(p.distance_m.unwrap(), 0.25, 1e-6, "distance");
    close(p.humidity.unwrap(), 0.5, 1e-6, "humidity");
    assert_eq!(img.editable(), vec![kelvinlens_core::Param::Emissivity, kelvinlens_core::Param::ReflectedK]);

    let before = img.kelvin().to_vec();
    img.set_params(&Params { emissivity: Some(0.80), reflected_k: Some(30.0 + 273.15), ..Default::default() }).unwrap();
    let (_, hottest, _) = whole(&img);
    // Reference: the same T⁴ correction computed independently in Python.
    close(hottest, 35.222, 0.01, "max with ε 0.80, reflected 30 °C");
    img.reset_params().unwrap();
    assert!(img.kelvin().iter().zip(&before).all(|(a, b)| (a - b).abs() < 1e-3));
}

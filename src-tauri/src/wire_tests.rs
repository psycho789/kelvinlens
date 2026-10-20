//! Checks that the JSON samples in `src/wireFixtures.ts` (which `tsc` checks against the types in
//! `src/api.ts`) are what serde writes and accepts. Rust is the source of truth: when a sample
//! fails, fix `api.ts`. Types that Rust only reads cannot reveal an extra TypeScript field,
//! because serde drops unknown fields; that limit is accepted.

use std::collections::{BTreeMap, BTreeSet};

use kelvinlens_core::analysis::{Finding, InspectSettings, Severity, SeverityLevel};
use kelvinlens_core::formula::{Formula, FormulaResult};
use kelvinlens_core::measure::ProfilePoint;
use kelvinlens_core::render::{Fusion, Look};
use kelvinlens_core::report::{ImageSection, ReportInfo, Template};
use kelvinlens_core::sidecar::{Annotation, Presentation, Sidecar};
use kelvinlens_core::thermogram::{Camera, Pip, Rect};
use kelvinlens_core::video::VideoInfo;
use kelvinlens_core::{Format, Measurement, Param, Params, Shape, Stats, report};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Number, Value};

use super::*;
use crate::cmd_error::{CmdError, ErrorKind};

const FIXTURES: &str = include_str!("../../src/wireFixtures.ts");

fn fixtures() -> BTreeMap<String, Value> {
    FIXTURES
        .split("\nexport const ")
        .skip(1)
        .map(|chunk| {
            let (name, rest) = chunk.split_once(" = ").expect("`export const <name> = `");
            let json = &rest[..rest.find(" satisfies ").expect("`satisfies <Type>`")];
            (name.to_string(), serde_json::from_str(json).unwrap_or_else(|e| panic!("fixture {name}: {e}")))
        })
        .collect()
}

fn fixture(name: &str) -> Value {
    fixtures().remove(name).unwrap_or_else(|| panic!("no fixture named {name}"))
}

/// Numbers compare by value whether the fixture spells them `1` or `1.0` (serde_json treats those as different).
fn normalised(value: Value) -> Value {
    match value {
        Value::Number(n) => Number::from_f64(n.as_f64().unwrap()).map_or(Value::Null, Value::Number),
        Value::Array(a) => Value::Array(a.into_iter().map(normalised).collect()),
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, normalised(v))).collect()),
        other => other,
    }
}

fn same(name: &str, actual: Value, expected: Value) {
    let pretty = |v: Value| serde_json::to_string_pretty(&normalised(v)).unwrap();
    assert_eq!(pretty(actual), pretty(expected), "fixture {name} differs from what serde writes");
}

/// What Rust sends must equal the fixture.
fn sends<T: Serialize>(name: &str, value: &T) {
    same(name, serde_json::to_value(value).unwrap(), fixture(name));
}

/// The fixture must load, and writing it back must give the fixture again.
fn round_trips<T: Serialize + DeserializeOwned>(name: &str) -> T {
    let value: T = serde_json::from_value(fixture(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
    sends(name, &value);
    value
}

/// The fixture must load.
fn reads<T: DeserializeOwned>(name: &str) -> T {
    serde_json::from_value(fixture(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

const CHECKED: [&str; 27] = [
    "annotations",
    "camera",
    "cmdError",
    "finding",
    "formula",
    "formulaResults",
    "fusions",
    "imageInfo",
    "inspectSettings",
    "look",
    "mapItem",
    "measurement",
    "params",
    "presentation",
    "profilePoint",
    "recomputed",
    "reportInfo",
    "reportItem",
    "series",
    "shapes",
    "sidecar",
    "stats",
    "stitchResult",
    "template",
    "triageRows",
    "triageSettings",
    "videoInfo",
];

fn stats() -> Stats {
    Stats { min: 290.5, max: 320.25, avg: 300.0, std_dev: 2.0, min_at: [1, 2], max_at: [3, 4], count: 9 }
}

fn camera() -> Camera {
    Camera {
        description: Some("Camera".into()),
        serial_number: Some("SN1".into()),
        date_time: Some("2026-10-19T10:00:00".into()),
        latitude: Some(51.5),
        longitude: Some(-0.25),
        altitude: Some(12.5),
        heading: Some(90.0),
        lens: Some("24 deg".into()),
        field_of_view: Some(24.0),
        focal_length: Some(13.5),
        gimbal: Some([1.0, 2.0, 3.0]),
        relative_altitude: Some(10.5),
    }
}

fn video_info() -> VideoInfo {
    VideoInfo { frames: 3, width: 4, height: 2, compressed: false, times: vec![0.0, 0.25, 0.5], frame_rate: Some(4.0) }
}

#[test]
fn wire_fixture_file_lists_every_checked_type() {
    let names: Vec<String> = fixtures().into_keys().collect();
    assert_eq!(names, CHECKED);
}

#[test]
fn wire_params_shapes_and_measurements() {
    round_trips::<Params>("params");
    round_trips::<Vec<Shape>>("shapes");
    round_trips::<Measurement>("measurement");
    sends("stats", &stats());
}

#[test]
fn wire_look_isotherms_and_fusion() {
    round_trips::<Look>("look");
    round_trips::<Vec<Fusion>>("fusions");
}

#[test]
fn wire_formulas_annotations_and_presentation() {
    round_trips::<Formula>("formula");
    round_trips::<Vec<Annotation>>("annotations");
    round_trips::<Presentation>("presentation");
    let results = vec![
        FormulaResult { value: Some(1.5), error: None },
        FormulaResult { value: None, error: Some("Unknown name".into()) },
    ];
    sends("formulaResults", &results);
}

#[test]
fn wire_inspection_and_findings() {
    round_trips::<InspectSettings>("inspectSettings");
    let finding = Finding {
        label: "A1".into(),
        hot: true,
        area: Shape::Box { x: 2.0, y: 2.0, w: 3.0, h: 3.0 },
        pixels: 9,
        extreme_k: 325.0,
        at: [3, 3],
        reference_k: 300.0,
        delta_k: 25.0,
        thermal_index: Some(0.5),
        severity: Severity {
            level: SeverityLevel::Severe,
            label: "Major discrepancy".into(),
            action: "Repair immediately".into(),
        },
    };
    sends("finding", &finding);
    assert_eq!(round_trips::<Finding>("finding"), finding);
}

#[test]
fn wire_sidecar() {
    round_trips::<Sidecar>("sidecar");
}

fn full_info() -> ImageInfo {
    ImageInfo {
        id: 1,
        path: "/data/a.jpg".into(),
        name: "a.jpg".into(),
        format: Format::Flir,
        format_label: "FLIR radiometric JPEG",
        width: 4,
        height: 2,
        params: reads("params"),
        file_params: reads("params"),
        editable: vec![Param::Emissivity, Param::ReflectedK],
        camera: camera(),
        embedded_measurements: vec![reads("measurement")],
        embedded_range: Some([280.0, 330.0]),
        embedded_palette: Some(vec![[0, 0, 0], [255, 255, 255]]),
        range: Some([290.0, 320.0]),
        photo_size: Some([640, 480]),
        pip: Some(Pip {
            source: Rect { x: 0, y: 0, w: 80, h: 60 },
            destination: Rect { x: 10, y: -20, w: 160, h: 120 },
        }),
        flir_jpeg: true,
        video: Some(video_info()),
        sidecar: Some(reads("sidecar")),
        sidecar_error: Some("bad edits file".into()),
    }
}

fn bare_info() -> ImageInfo {
    ImageInfo {
        id: 2,
        path: "/data/pano.png".into(),
        name: "pano.png".into(),
        format: Format::Panorama,
        format_label: "Panorama",
        width: 8,
        height: 2,
        params: reads("params"),
        file_params: reads("params"),
        editable: vec![],
        camera: Camera::default(),
        embedded_measurements: vec![],
        embedded_range: None,
        embedded_palette: None,
        range: None,
        photo_size: None,
        pip: None,
        flir_jpeg: false,
        video: None,
        sidecar: None,
        sidecar_error: None,
    }
}

#[test]
fn wire_image_info_and_camera() {
    sends("camera", &camera());
    sends("videoInfo", &video_info());
    sends("imageInfo", &full_info());
}

#[test]
fn wire_command_results() {
    sends("recomputed", &Recomputed { params: reads("params"), range: Some([290.0, 320.0]) });
    sends("profilePoint", &ProfilePoint { d: 1.5, k: 300.25 });
    let rows = vec![
        TriageRow {
            path: "/data/a.jpg".into(),
            name: "a.jpg".into(),
            error: None,
            min_k: Some(290.5),
            max_k: Some(331.25),
            flagged: true,
            date_time: Some("2026-10-19T10:00:00".into()),
        },
        TriageRow {
            path: "/data/b.jpg".into(),
            name: "b.jpg".into(),
            error: Some("Could not open thermogram: bad".into()),
            min_k: None,
            max_k: None,
            flagged: false,
            date_time: None,
        },
    ];
    sends("triageRows", &rows);
    sends(
        "stitchResult",
        &StitchResult { info: bare_info(), offsets: vec![[0, 0], [10, -2]], scores: vec![0.75, 0.5] },
    );
    sends(
        "series",
        &Series { rows: vec![vec![Some(stats()), None], vec![Some(stats()), Some(stats())]], times: vec![0.0, 0.5] },
    );
}

#[test]
fn wire_report_template_info_and_item() {
    round_trips::<Template>("template");
    let keys = |v: Value| v.as_object().unwrap().keys().cloned().collect::<BTreeSet<_>>();
    let expected = keys(fixture("template"));
    for t in report::builtin_templates() {
        assert_eq!(keys(serde_json::to_value(&t).unwrap()), expected, "template {}", t.id);
    }
    round_trips::<ReportInfo>("reportInfo");
    reads::<ReportItem>("reportItem");
}

#[test]
fn wire_triage_and_map_requests() {
    let t: TriageSettings = reads("triageSettings");
    assert_eq!((t.above_k, t.below_k), (Some(330.0), Some(270.0)));
    let m: MapItem = reads("mapItem");
    assert_eq!((m.id, m.description.as_str()), (1, "North wall"));
}

#[test]
fn wire_harness_detects_a_renamed_field() {
    let mut mutated = fixture("stats");
    let count = mutated.as_object_mut().unwrap().remove("count").unwrap();
    mutated.as_object_mut().unwrap().insert("total".into(), count);
    let actual = serde_json::to_value(stats()).unwrap();
    assert!(std::panic::catch_unwind(move || same("stats", actual, mutated)).is_err());
}

#[test]
fn wire_cmd_error() {
    sends("cmdError", &CmdError::not_found("This image is no longer open"));
}

const API: &str = include_str!("../../src/api.ts");

/// Members of `export type <name> = "a" | "b";` in `src/api.ts`, sorted (empty pieces from a leading `|` dropped).
fn ts_union(name: &str) -> Vec<String> {
    let start = API.find(&format!("export type {name} =")).unwrap_or_else(|| panic!("api.ts has no type {name}"));
    let decl = &API[start..];
    let body = &decl[decl.find('=').unwrap() + 1..decl.find(';').unwrap()];
    let mut m: Vec<String> =
        body.split('|').map(|x| x.trim().trim_matches('"').to_string()).filter(|x| !x.is_empty()).collect();
    m.sort();
    m
}

/// How serde writes each value, sorted: the text of a string, the digits of a number.
fn serde_ids<T: Serialize>(values: &[T]) -> Vec<String> {
    let mut ids: Vec<String> = values
        .iter()
        .map(|v| match serde_json::to_value(v).unwrap() {
            Value::String(t) => t,
            other => other.to_string(),
        })
        .collect();
    ids.sort();
    ids
}

#[test]
fn error_kind_union_in_api_ts_matches_rust() {
    use ErrorKind::{Internal, InvalidFile, InvalidInput, Io, NotFound, Unsupported};
    // Compile error if a variant is added.
    let _exhaustive = |k: ErrorKind| match k {
        InvalidFile | InvalidInput | Unsupported | NotFound | Io | Internal => (),
    };
    assert_eq!(serde_ids(&[InvalidFile, InvalidInput, Unsupported, NotFound, Io, Internal]), ts_union("ErrorKind"));
}

#[test]
fn image_section_union_in_api_ts_matches_rust() {
    use ImageSection::*;
    let _exhaustive = |s: ImageSection| match s {
        Isotherms | Measurements | Formulas | Findings | Electrical | Building | Hvac | Profiles | Fields | Notes
        | Parameters | Details => (),
    };
    let all = [
        Isotherms,
        Measurements,
        Formulas,
        Findings,
        Electrical,
        Building,
        Hvac,
        Profiles,
        Fields,
        Notes,
        Parameters,
        Details,
    ];
    assert_eq!(serde_ids(&all), ts_union("ImageSection"));
}

#[test]
fn image_format_union_in_api_ts_matches_rust() {
    use Format::*;
    let _exhaustive = |f: Format| match f {
        Flir | Fluke | Hti | Irg | InfiRay | Png | Tiff | Video | Panorama => (),
    };
    let all = [Flir, Fluke, Hti, Irg, InfiRay, Png, Tiff, Video, Panorama];
    assert_eq!(serde_ids(&all), ts_union("ImageFormat"));
}

#[test]
fn severity_level_union_in_api_ts_matches_rust() {
    use SeverityLevel::*;
    let _exhaustive = |l: SeverityLevel| match l {
        Low | Moderate | High | Severe => (),
    };
    assert_eq!(serde_ids(&[Low, Moderate, High, Severe]), ts_union("SeverityLevel"));
}

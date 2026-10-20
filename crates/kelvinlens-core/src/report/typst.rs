//! PDF output: `Prepared` becomes Typst inputs and `report.typ` is compiled in memory.
use serde_json::Value as Json;
use typst::foundations::{Array, Bytes, Dict, Value};
use typst_as_lib::TypstEngine;

use super::Prepared;
use crate::{Error, Result};

const TEMPLATE: &str = include_str!("../report.typ");
const FONTS: [&[u8]; 3] = [
    include_bytes!("../../assets/fonts/FiraSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-Italic.ttf"),
];
pub(super) fn pdf_from(p: &Prepared) -> Result<Vec<u8>> {
    let mut inputs = match to_typst(&serde_json::to_value(p).map_err(|e| Error::Report(e.to_string()))?) {
        Value::Dict(d) => d,
        _ => unreachable!("a struct serializes to an object"),
    };
    let images: Array = p
        .images
        .iter()
        .map(|img| {
            let mut v = to_typst(&serde_json::to_value(img).unwrap_or(Json::Null));
            if let Value::Dict(d) = &mut v {
                d.insert("thermal".into(), Value::Bytes(Bytes::new(img.thermal_png.clone())));
                d.insert("bar".into(), Value::Bytes(Bytes::new(img.bar_png.clone())));
                d.insert(
                    "photo".into(),
                    img.photo_jpeg.clone().map(|b| Value::Bytes(Bytes::new(b))).unwrap_or(Value::None),
                );
            }
            v
        })
        .collect();
    inputs.insert("images".into(), Value::Array(images));
    inputs.insert("logo".into(), p.logo.clone().map(|b| Value::Bytes(Bytes::new(b))).unwrap_or(Value::None));

    let engine = TypstEngine::builder().main_file(TEMPLATE).fonts(FONTS).build();
    let doc =
        engine.compile_with_input(inputs).output.map_err(|e| Error::Report(format!("typesetting failed: {e}")))?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|diags| Error::Report(format!("PDF export failed: {diags:?}")))
}

/// serde JSON to a Typst value. Integers stay integers so Typst can use them as counts.
pub(super) fn to_typst(v: &Json) -> Value {
    match v {
        Json::Null => Value::None,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::Str(s.as_str().into()),
        Json::Array(a) => Value::Array(a.iter().map(to_typst).collect()),
        Json::Object(o) => {
            let mut d = Dict::new();
            for (k, v) in o {
                d.insert(k.as_str().into(), to_typst(v));
            }
            Value::Dict(d)
        }
    }
}

//! Maps of where images were taken: KML (Google Earth, most GIS tools) and GeoJSON.

use serde_json::json;

use crate::xml::escape;

/// One image's position and what to show for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placemark {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f32>,
    /// Short text such as "max 54.2 °C · 2 findings".
    pub description: String,
    /// The capture time, `YYYY-MM-DD HH:MM:SS…`, if known.
    pub time: Option<String>,
}

/// KML 2.2 with one placemark per image.
pub fn kml(title: &str, marks: &[Placemark]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n",
    );
    out += &format!("  <name>{}</name>\n", escape(title));
    for m in marks {
        out += "  <Placemark>\n";
        out += &format!("    <name>{}</name>\n", escape(&m.name));
        out += &format!("    <description>{}</description>\n", escape(&m.description));
        if let Some(t) = m.time.as_deref().and_then(iso_time) {
            out += &format!("    <TimeStamp><when>{t}</when></TimeStamp>\n");
        }
        let alt = m.altitude.map(|a| format!(",{a}")).unwrap_or_default();
        out += &format!("    <Point><coordinates>{},{}{alt}</coordinates></Point>\n", m.longitude, m.latitude);
        out += "  </Placemark>\n";
    }
    out += "</Document>\n</kml>\n";
    out
}

/// GeoJSON FeatureCollection with one point per image.
pub fn geojson(marks: &[Placemark]) -> String {
    let features: Vec<_> = marks
        .iter()
        .map(|m| {
            let mut coords = vec![json!(m.longitude), json!(m.latitude)];
            if let Some(a) = m.altitude {
                coords.push(json!(a));
            }
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": coords },
                "properties": { "name": m.name, "description": m.description, "time": m.time },
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "type": "FeatureCollection", "features": features })).unwrap_or_default()
        + "\n"
}

/// `2013-04-12 09:24:01.123+02:00` → `2013-04-12T09:24:01.123+02:00` (XML Schema dateTime).
fn iso_time(t: &str) -> Option<String> {
    let b = t.as_bytes();
    (b.len() >= 19 && b[4] == b'-' && b[7] == b'-' && b[10] == b' ').then(|| format!("{}T{}", &t[..10], &t[11..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark() -> Placemark {
        Placemark {
            name: "IR_0001 <A&B>".into(),
            latitude: 51.5,
            longitude: -0.12,
            altitude: Some(12.5),
            description: "max 54.2 °C".into(),
            time: Some("2013-04-12 09:24:01".into()),
        }
    }

    #[test]
    fn kml_has_lon_lat_order_and_escapes() {
        let k = kml("Site", &[mark()]);
        assert!(k.contains("<coordinates>-0.12,51.5,12.5</coordinates>"));
        assert!(k.contains("<name>IR_0001 &lt;A&amp;B&gt;</name>"));
        assert!(k.contains("<when>2013-04-12T09:24:01</when>"));
    }

    #[test]
    fn geojson_parses_back() {
        let g: serde_json::Value = serde_json::from_str(&geojson(&[mark()])).unwrap();
        assert_eq!(g["features"][0]["geometry"]["coordinates"], json!([-0.12, 51.5, 12.5]));
        assert_eq!(g["features"][0]["properties"]["name"], "IR_0001 <A&B>");
    }
}

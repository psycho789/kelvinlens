//! The report as one self-contained HTML page: images embedded as data URLs, overlays and charts
//! as inline SVG, no scripts and nothing loaded from the network. Opens in any browser and can be
//! edited in word processors that import HTML.

use std::fmt::Write as _;

use crate::measure::Shape;
use crate::report::{Chart, Prepared, PreparedImage, base64_encode};
use crate::sidecar::Annotation;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const SEVERITY: [&str; 4] = ["#64748b", "#ca8a04", "#ea580c", "#dc2626"];

fn severity_color(level: u8) -> &'static str {
    SEVERITY[(level.clamp(1, 4) - 1) as usize]
}

fn table(head: &[&str], rows: &[Vec<String>]) -> String {
    let mut t = String::from("<table><thead><tr>");
    for h in head {
        let _ = write!(t, "<th>{}</th>", esc(h));
    }
    t += "</tr></thead><tbody>";
    for r in rows {
        t += "<tr>";
        for c in r {
            // Cells are escaped by the caller when they hold markup (severity swatches).
            let _ = write!(t, "<td>{c}</td>");
        }
        t += "</tr>";
    }
    t + "</tbody></table>"
}

fn kv(rows: &[[String; 2]]) -> String {
    let mut t = String::from("<table class=\"kv\"><tbody>");
    for [k, v] in rows {
        let _ = write!(t, "<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(v));
    }
    t + "</tbody></table>"
}

fn severity_cell(level: u8, label: &str) -> String {
    format!("<span class=\"sw\" style=\"background:{}\"></span>{}", severity_color(level), esc(label))
}

pub fn html(p: &Prepared) -> String {
    let mut o = String::new();
    let _ = write!(
        o,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>{}</style></head><body>",
        esc(&p.title),
        CSS.replace("ACCENT", &p.accent)
    );
    o += "<header class=\"cover\">";
    if let Some(logo) = &p.logo {
        let _ = write!(o, "<img class=\"logo\" alt=\"\" src=\"data:image/png;base64,{}\">", base64_encode(logo));
    }
    let _ = write!(o, "<h1>{}</h1>", esc(&p.title));
    if !p.company.is_empty() {
        let _ = write!(o, "<p class=\"company\">{}</p>", esc(&p.company));
    }
    o += &kv(&p.cover_rows);
    o += "</header>";
    if !p.conditions.is_empty() {
        o += "<h2>Conditions</h2>";
        o += &kv(&p.conditions);
    }
    if p.sections.summary && !p.summary.is_empty() {
        let _ = write!(o, "<h2>Summary</h2><p class=\"pre\">{}</p>", esc(&p.summary));
    }
    if p.sections.findings_overview && !p.overview.is_empty() {
        o += "<h2>Findings</h2>";
        let rows: Vec<Vec<String>> = p
            .overview
            .iter()
            .map(|r| {
                vec![
                    esc(&r.image),
                    esc(&r.label),
                    esc(&r.kind),
                    esc(&r.delta),
                    severity_cell(r.level, &r.severity),
                    esc(&r.action),
                ]
            })
            .collect();
        o += &table(&["Image", "Finding", "Type", "ΔT", "Severity", "Action"], &rows);
    }
    if p.sections.contents && p.images.len() > 1 {
        o += "<h2>Contents</h2><ol class=\"contents\">";
        for img in &p.images {
            let _ = write!(o, "<li><a href=\"#img{}\">{}</a></li>", img.number, esc(&img.name));
        }
        o += "</ol>";
    }

    for img in &p.images {
        image_html(&mut o, p, img);
    }

    if !p.trends.is_empty() {
        o += "<section><h1>Trends</h1><p class=\"muted\">Images with the same asset tag, ordered by capture date.</p>";
        for c in &p.trends {
            o += &chart_svg(c);
        }
        o += "</section>";
    }
    if p.sections.signature {
        o += "<div class=\"sign\"><div>Inspector signature</div><div>Date</div></div>";
    }
    let _ = writeln!(o, "<footer>{}<br>Created with {}.</footer></body></html>", esc(&p.disclaimer), esc(&p.generator));
    o
}

fn image_html(o: &mut String, p: &Prepared, img: &PreparedImage) {
    let _ = write!(o, "<section class=\"image\" id=\"img{}\"><h1>{}. {}</h1>", img.number, img.number, esc(&img.name));
    if !img.asset.is_empty() {
        let _ = write!(o, "<p class=\"muted\">Asset: {}</p>", esc(&img.asset));
    }
    o.push_str("<div class=\"views\"><figure>");
    o.push_str(&thermal_svg(img));
    let _ = write!(
        o,
        "<div class=\"bar\" style=\"background-image:url(data:image/png;base64,{})\"></div><figcaption><span>{}</span><span>{} · {}</span><span>{}</span></figcaption></figure>",
        base64_encode(&img.bar_png),
        esc(&img.range_min),
        esc(&img.palette),
        esc(&img.distribution),
        esc(&img.range_max)
    );
    if let Some(photo) = &img.photo_jpeg {
        let _ =
            write!(o, "<figure><img alt=\"Photo\" src=\"data:image/jpeg;base64,{}\"></figure>", base64_encode(photo));
    }
    o.push_str("</div>");

    for name in &p.image_sections {
        match name.as_str() {
            "isotherms" if !img.isotherms.is_empty() => {
                o.push_str("<h2>Isotherms and alarms</h2><ul class=\"legend\">");
                for r in &img.isotherms {
                    let _ = write!(
                        o,
                        "<li><span class=\"sw\" style=\"background:{}\"></span>{}</li>",
                        r.color,
                        esc(&r.text)
                    );
                }
                o.push_str("</ul>");
            }
            "measurements" if !img.measurements.is_empty() => {
                let alarms = img.measurements.iter().any(|m| !m.alarm.is_empty());
                let coverage = img.measurements.iter().any(|m| !m.coverage.is_empty());
                let mut head = vec!["Label", "Tool", "Min", "Max", "Average"];
                if alarms {
                    head.push("Alarm");
                }
                if coverage {
                    head.push("In isotherm");
                }
                let rows: Vec<Vec<String>> = img
                    .measurements
                    .iter()
                    .map(|m| {
                        let mut r = vec![esc(&m.label), m.kind.into(), esc(&m.min), esc(&m.max), esc(&m.avg)];
                        if alarms {
                            r.push(if m.alarm == "Alarm" {
                                format!("<b class=\"alarm\">{}</b>", esc(&m.alarm))
                            } else {
                                esc(&m.alarm)
                            });
                        }
                        if coverage {
                            r.push(esc(&m.coverage));
                        }
                        r
                    })
                    .collect();
                o.push_str("<h2>Measurements</h2>");
                o.push_str(&table(&head, &rows));
            }
            "formulas" if !img.formulas.is_empty() => {
                let rows: Vec<Vec<String>> = img
                    .formulas
                    .iter()
                    .map(|f| vec![esc(&f[0]), format!("<code>{}</code>", esc(&f[1])), esc(&f[2])])
                    .collect();
                o.push_str("<h2>Formulas</h2>");
                o.push_str(&table(&["Label", "Formula", "Result"], &rows));
            }
            "findings" if !img.findings.is_empty() => {
                let rows: Vec<Vec<String>> = img
                    .findings
                    .iter()
                    .map(|f| {
                        vec![
                            esc(&f.label),
                            (if f.hot { "Hot" } else { "Cold" }).into(),
                            esc(&f.extreme),
                            esc(&f.delta),
                            severity_cell(f.level, &f.severity),
                            esc(&f.action),
                        ]
                    })
                    .collect();
                o.push_str("<h2>Findings</h2>");
                o.push_str(&table(&["Finding", "Type", "Temperature", "ΔT", "Severity", "Action"], &rows));
            }
            "electrical" if !img.electrical.is_empty() => {
                let rows: Vec<Vec<String>> =
                    img.electrical.iter().map(|r| r.iter().map(|c| esc(c)).collect()).collect();
                o.push_str("<h2>Temperature rise (NETA)</h2>");
                o.push_str(&table(&["Label", "Max", "Rise", "Severity", "Action"], &rows));
            }
            "building" if !img.building.is_empty() => {
                let rows: Vec<Vec<String>> = img.building.iter().map(|r| r.iter().map(|c| esc(c)).collect()).collect();
                o.push_str("<h2>Insulation and moisture</h2>");
                o.push_str(&table(&["Label", "Coldest", "Thermal index", "Surface RH", "Risk"], &rows));
            }
            "hvac" if !img.hvac.is_empty() => {
                o.push_str("<h2>Temperature split</h2>");
                o.push_str(&kv(&img.hvac));
            }
            "profiles" if !img.profiles.is_empty() => {
                o.push_str("<h2>Profiles</h2>");
                for c in &img.profiles {
                    o.push_str(&chart_svg(c));
                }
            }
            "fields" if !img.fields.is_empty() => {
                o.push_str("<h2>Inspection details</h2>");
                o.push_str(&kv(&img.fields));
            }
            "notes" if !img.notes.is_empty() => {
                let _ = write!(o, "<h2>Notes</h2><p class=\"pre\">{}</p>", esc(&img.notes));
            }
            "parameters" => {
                o.push_str("<h2>Parameters</h2>");
                if img.params.is_empty() {
                    o.push_str("<p class=\"muted\">This format stores final temperatures only.</p>");
                } else {
                    let rows: Vec<Vec<String>> =
                        img.params.iter().map(|r| r.iter().map(|c| esc(c)).collect()).collect();
                    o.push_str(&table(&["Parameter", "Used", "In file"], &rows));
                }
                if let Some(r) = &img.image_range {
                    let _ = write!(o, "<p class=\"muted\">Image temperatures: {}</p>", esc(r));
                }
            }
            "details" => {
                o.push_str("<h2>Image details</h2>");
                o.push_str(&kv(&img.camera));
            }
            _ => {}
        }
    }
    o.push_str("</section>");
}

/// The thermal image with overlays, cropped and rotated like the PDF.
fn thermal_svg(img: &PreparedImage) -> String {
    let [cx, cy, cw, ch] = img.crop;
    let th = img.rotation.to_radians();
    let (c, s) = (th.cos().abs(), th.sin().abs());
    let (bw, bh) = (cw * c + ch * s, cw * s + ch * c);
    let (mx, my) = (cx + cw / 2.0, cy + ch / 2.0);
    let mut o = String::new();
    let _ = write!(
        o,
        "<svg class=\"thermal\" viewBox=\"{} {} {bw} {bh}\" xmlns=\"http://www.w3.org/2000/svg\"><defs><clipPath id=\"c{n}\"><rect x=\"{cx}\" y=\"{cy}\" width=\"{cw}\" height=\"{ch}\"/></clipPath></defs><g transform=\"rotate({} {mx} {my})\"><g clip-path=\"url(#c{n})\">",
        mx - bw / 2.0,
        my - bh / 2.0,
        img.rotation,
        n = img.number
    );
    let _ = write!(
        o,
        "<image href=\"data:image/png;base64,{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" style=\"image-rendering:pixelated\"/>",
        base64_encode(&img.thermal_png),
        img.width,
        img.height
    );
    // Stroke width in pixels that looks like ~1.5 screen pixels at typical sizes.
    let sw = (img.width.max(img.height) as f32 / 400.0).max(0.6);
    // Labels are drawn after the rotated group, upright, at their rotated positions.
    let mut labels: Vec<(f32, f32, String, &str)> = Vec::new();
    for f in &img.findings {
        if let Shape::Box { x, y, w, h } = f.area {
            let _ = write!(
                o,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
                severity_color(f.level),
                sw * 1.4,
                sw * 3.0,
                sw * 2.0
            );
            labels.push((x, y + h, f.label.clone(), severity_color(f.level)));
        }
    }
    for m in &img.measurements {
        let shape = match &m.shape {
            Shape::Spot { x, y } => format!(
                "<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\"/><path d=\"M{} {y}H{}M{x} {}V{}\"/>",
                x - 2.0 * sw * 3.0,
                x + 2.0 * sw * 3.0,
                y - 2.0 * sw * 3.0,
                y + 2.0 * sw * 3.0,
                r = sw * 3.0
            ),
            Shape::Box { x, y, w, h } => format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                x.min(x + w),
                y.min(y + h),
                w.abs(),
                h.abs()
            ),
            Shape::Ellipse { cx, cy, rx, ry } => format!("<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\"/>"),
            Shape::Line { x1, y1, x2, y2 } => format!("<path d=\"M{x1} {y1}L{x2} {y2}\"/>"),
            Shape::Polygon { points } => format!("<polygon points=\"{}\"/>", pts(points)),
            Shape::Polyline { points } => format!("<polyline points=\"{}\"/>", pts(points)),
            Shape::Wand { x, y, .. } => {
                let loops: String = m.outline.iter().map(|l| format!("<polygon points=\"{}\"/>", pts(l))).collect();
                format!("{loops}<circle cx=\"{x}\" cy=\"{y}\" r=\"{}\"/>", sw * 2.0)
            }
        };
        let _ = write!(
            o,
            "<g fill=\"none\" stroke=\"rgba(0,0,0,.55)\" stroke-width=\"{}\">{shape}</g><g fill=\"none\" stroke=\"#fff\" stroke-width=\"{}\">{shape}</g>",
            sw * 2.4,
            sw
        );
        let (lx, ly) = match &m.shape {
            Shape::Spot { x, y } | Shape::Wand { x, y, .. } => (*x, *y),
            Shape::Box { x, y, w, h } => (x.min(x + w), y.min(y + h)),
            Shape::Ellipse { cx, cy, rx, ry } => (cx - rx, cy - ry),
            Shape::Line { x1, y1, .. } => (*x1, *y1),
            Shape::Polygon { points } | Shape::Polyline { points } => {
                points.first().map(|p| (p[0], p[1])).unwrap_or((0.0, 0.0))
            }
        };
        labels.push((lx, ly, m.label.clone(), "rgba(10,14,22,.75)"));
    }
    for a in &img.annotations {
        match a {
            Annotation::Sketch { points, color } => {
                let _ = write!(
                    o,
                    "<polyline points=\"{}\" fill=\"none\" stroke=\"rgb({},{},{})\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                    pts(points),
                    color[0],
                    color[1],
                    color[2],
                    sw * 1.6
                );
            }
            Annotation::Text { x, y, text } => {
                let _ = write!(
                    o,
                    "<circle cx=\"{x}\" cy=\"{y}\" r=\"{}\" fill=\"#facc15\" stroke=\"#000\" stroke-width=\"{}\"/>",
                    sw * 2.2,
                    sw * 0.5
                );
                labels.push((*x, *y, text.clone(), "#fef9c3"));
            }
        }
    }
    o.push_str("</g></g>");
    let (cos, sin) = (th.cos(), th.sin());
    for (x, y, text, bg) in labels {
        if x < cx || y < cy || x > cx + cw || y > cy + ch {
            continue;
        }
        let (rx, ry) = (mx + (x - mx) * cos - (y - my) * sin, my + (x - mx) * sin + (y - my) * cos);
        if bg == "#fef9c3" {
            label(&mut o, rx + sw * 3.0, ry - sw * 5.0, &text, bg, sw);
        } else {
            label(&mut o, rx, ry, &text, bg, sw);
        }
    }
    o.push_str("</svg>");
    o
}

fn pts(points: &[[f32; 2]]) -> String {
    points.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" ")
}

fn label(o: &mut String, x: f32, y: f32, text: &str, bg: &str, sw: f32) {
    let size = sw * 7.5;
    let w = size * 0.62 * text.chars().count() as f32 + size * 0.6;
    let fg = if bg == "#fef9c3" { "#111" } else { "#fff" };
    let _ = write!(
        o,
        "<rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{}\" rx=\"{}\" fill=\"{bg}\"/><text x=\"{}\" y=\"{}\" font-size=\"{size}\" fill=\"{fg}\" font-family=\"sans-serif\">{}</text>",
        x + sw * 2.0,
        y + sw,
        size * 1.35,
        sw,
        x + sw * 2.0 + size * 0.3,
        y + sw + size * 1.02,
        esc(text)
    );
}

fn chart_svg(c: &Chart) -> String {
    let (w, h, left, bottom) = (640.0, 200.0, 48.0, 22.0);
    let (pw, ph) = (w - left - 8.0, h - bottom - 8.0);
    let mut o = format!(
        "<figure class=\"chart\"><figcaption>{}</figcaption><svg viewBox=\"0 0 {w} {h}\" xmlns=\"http://www.w3.org/2000/svg\" font-family=\"sans-serif\" font-size=\"10\">",
        esc(&c.title)
    );
    let _ = write!(o, "<rect x=\"{left}\" y=\"8\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"#bbb\"/>");
    for (t, l) in &c.y_ticks {
        let y = 8.0 + ph - t * ph;
        let _ = write!(
            o,
            "<line x1=\"{left}\" x2=\"{}\" y1=\"{y}\" y2=\"{y}\" stroke=\"#e5e5e5\"/><text x=\"{}\" y=\"{}\" text-anchor=\"end\" fill=\"#666\">{}</text>",
            left + pw,
            left - 4.0,
            y + 3.0,
            esc(l)
        );
    }
    let n = c.x_ticks.len();
    for (i, (t, l)) in c.x_ticks.iter().enumerate() {
        let anchor = if i == 0 {
            "start"
        } else if i + 1 == n {
            "end"
        } else {
            "middle"
        };
        let _ = write!(
            o,
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" fill=\"#666\">{}</text>",
            left + t * pw,
            h - 6.0,
            esc(l)
        );
    }
    for s in &c.series {
        let points: Vec<String> =
            s.points.iter().map(|p| format!("{:.2},{:.2}", left + p[0] * pw, 8.0 + ph - p[1] * ph)).collect();
        let _ = write!(
            o,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.6\"/>",
            points.join(" "),
            s.color
        );
        if s.points.len() <= 12 {
            for p in &points {
                let (x, y) = p.split_once(',').unwrap();
                let _ = write!(o, "<circle cx=\"{x}\" cy=\"{y}\" r=\"2.6\" fill=\"{}\"/>", s.color);
            }
        }
    }
    o.push_str("</svg><div class=\"legend\">");
    let _ = write!(o, "<span class=\"muted\">{} · {}</span>", esc(&c.x_label), esc(&c.y_label));
    for s in &c.series {
        let _ = write!(o, "<span><span class=\"sw\" style=\"background:{}\"></span>{}</span>", s.color, esc(&s.name));
    }
    o.push_str("</div></figure>");
    o
}

const CSS: &str = r#"
:root{--accent:ACCENT;--muted:#666;--line:#d4d4d8}
body{font:15px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;color:#18181b;background:#fff;max-width:900px;margin:0 auto;padding:24px 16px}
h1{font-size:1.5em;margin:1.2em 0 .4em}h2{font-size:1.1em;margin:1.1em 0 .4em}
.cover h1{margin-top:0;border-bottom:3px solid var(--accent);padding-bottom:6px}
.cover .logo{float:right;max-height:64px;max-width:200px}
.company{color:var(--muted);margin:.2em 0 1em}
table{border-collapse:collapse;width:100%;margin:.3em 0 .8em;font-size:.92em}
th,td{border:1px solid var(--line);padding:4px 6px;text-align:left;vertical-align:top}
thead th{background:#f4f4f5}
table.kv th{width:34%;color:var(--muted);font-weight:normal;background:none}
.muted{color:var(--muted)}.pre{white-space:pre-wrap}
section.image{border-top:2px solid var(--accent);margin-top:2em;break-before:page}
.views{display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start}
.views figure{margin:0;flex:1 1 320px;min-width:0}
.views img,.views svg.thermal{width:100%;height:auto;display:block}
.bar{height:12px;background-size:100% 100%;image-rendering:auto;margin-top:4px}
figcaption{display:flex;justify-content:space-between;font-size:.8em;color:var(--muted)}
.sw{display:inline-block;width:.8em;height:.8em;border-radius:2px;margin-right:.4em;vertical-align:-1px}
ul.legend{list-style:none;padding:0}
.chart{margin:.5em 0 1em}.chart svg{width:100%;height:auto}.chart figcaption{font-weight:600;color:#18181b}
.chart .legend{display:flex;flex-wrap:wrap;gap:12px;font-size:.8em}
.alarm{color:#dc2626}
.sign{display:flex;gap:48px;margin-top:48px}.sign div{flex:1;border-top:1px solid #333;padding-top:4px;color:var(--muted);font-size:.85em}
footer{margin-top:3em;color:var(--muted);font-size:.8em;border-top:1px solid var(--line);padding-top:8px}
@media print{body{max-width:none}}
"#;

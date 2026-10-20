// Kelvinlens inspection report. All data arrives through `sys.inputs` (see report.rs `Prepared`).
#let data = sys.inputs
#let accent = rgb(data.accent)
#let muted = luma(105)
// Header row with bold labels.
#let head(..cells) = table.header(..cells.pos().map(strong))
// Same colours as analysis.rs SEVERITY_COLORS and src/overlays.ts SEVERITY (checked by a Rust test).
#let severity-colors = (rgb("#64748b"), rgb("#ca8a04"), rgb("#ea580c"), rgb("#dc2626"))
#let severity-color(level) = severity-colors.at(calc.clamp(level, 1, 4) - 1)
#let swatch(fill) = box(width: 7pt, height: 7pt, radius: 1pt, fill: fill, baseline: 0.5pt)

#set document(title: data.title)
#set page(
  paper: data.paper,
  margin: (x: 18mm, top: 22mm, bottom: 20mm),
  header: context {
    if counter(page).get().first() > 1 {
      set text(8pt, fill: muted)
      grid(
        columns: (1fr, auto),
        align: (left + horizon, right + horizon),
        if data.company != "" { data.company } else { data.title },
        if data.logo != none { image(data.logo, height: 7mm) },
      )
    }
  },
  footer: context {
    set text(8pt, fill: muted)
    [#data.title #if data.footer != "" [· #data.footer] #h(1fr) Page #counter(page).display() of #counter(page).final().first()]
  },
)
#set text(font: "Fira Sans", size: 9.5pt, lang: "en")
#set par(justify: false, leading: 0.6em)
#set table(stroke: 0.5pt + luma(205), inset: (x: 5pt, y: 4pt))
#show heading.where(level: 1): set text(15pt)
#show heading.where(level: 2): it => block(above: 1.1em, below: 0.6em, text(11pt, weight: "bold", it.body))

// ---------------------------------------------------------------- charts

// A line chart: `c` from report.rs `Chart`, points normalised 0–1 with y up.
#let line-chart(c, w, height) = {
  let axis = 13mm
  let pw = w - axis
  let ph = height
  let xy(p) = (p.at(0) * pw, ph - p.at(1) * ph)
  block(width: w, breakable: false, {
    text(9pt, weight: "bold", c.title)
    v(2pt)
    grid(
      columns: (axis, pw),
      rows: (ph, auto),
      box(width: axis, height: ph, {
        for t in c.y_ticks {
          place(top + right, dx: -3pt, dy: ph - t.at(0) * ph - 4pt, text(7pt, fill: muted, t.at(1)))
        }
      }),
      box(width: pw, height: ph, stroke: 0.5pt + luma(180), {
        for t in c.y_ticks {
          place(top + left, dy: ph - t.at(0) * ph, line(length: pw, stroke: 0.3pt + luma(225)))
        }
        for se in c.series {
          let col = rgb(se.color)
          if se.points.len() > 1 {
            place(top + left, curve(
              stroke: 1.1pt + col,
              curve.move(xy(se.points.at(0))),
              ..se.points.slice(1).map(p => curve.line(xy(p))),
            ))
          }
          if se.points.len() <= 12 {
            for p in se.points {
              let (x, y) = xy(p)
              place(top + left, dx: x - 2pt, dy: y - 2pt, circle(radius: 2pt, fill: col))
            }
          }
        }
      }),
      [],
      box(width: pw, height: 11pt, {
        let n = c.x_ticks.len()
        for (i, t) in c.x_ticks.enumerate() {
          let at = t.at(0) * pw
          if i == 0 {
            place(top + left, dx: at, text(7pt, fill: muted, t.at(1)))
          } else if i == n - 1 {
            place(top + right, dx: at - pw, text(7pt, fill: muted, t.at(1)))
          } else {
            place(top + left, dx: at - 15mm, box(width: 30mm, align(center, text(7pt, fill: muted, t.at(1)))))
          }
        }
      }),
    )
    v(1pt)
    text(7.5pt, fill: muted)[#c.x_label · #c.y_label]
    h(6pt)
    for se in c.series {
      box(text(7.5pt)[#swatch(rgb(se.color)) #se.name])
      h(6pt)
    }
  })
}

// ---------------------------------------------------------------- thermal view

#let tag(x, y, label, fill: black.transparentize(35%)) = place(
  top + left,
  dx: x + 3pt,
  dy: y + 2pt,
  box(fill: fill, inset: (x: 2pt, y: 1.5pt), radius: 1pt, text(7pt, fill: white, label)),
)

// The thermal image at `s` length per pixel, with measurements, findings and annotations.
// Overlay drawing is implemented three times (Typst, SVG, canvas); change shapes in all three. See sprint-outline 3.3.
#let overlays(img, s) = box(width: img.width * s, height: img.height * s, {
  place(top + left, image(img.thermal, width: img.width * s, height: img.height * s, scaling: "pixelated"))
  let outline = 1.8pt + black.transparentize(45%)
  let line-stroke = 0.8pt + white
  let pt(p) = (p.at(0) * s, p.at(1) * s)
  for f in img.findings {
    let a = f.area
    place(top + left, dx: a.x * s, dy: a.y * s, rect(width: a.w * s, height: a.h * s, stroke: (paint: severity-color(f.level), thickness: 1.2pt, dash: "dashed")))
  }
  for m in img.measurements {
    let sh = m.shape
    for st in (outline, line-stroke) {
      if sh.kind == "spot" {
        let (x, y) = (sh.x * s, sh.y * s)
        place(top + left, dx: x - 3.5pt, dy: y - 3.5pt, circle(radius: 3.5pt, stroke: st))
        place(top + left, line(start: (x - 7pt, y), end: (x + 7pt, y), stroke: st))
        place(top + left, line(start: (x, y - 7pt), end: (x, y + 7pt), stroke: st))
      } else if sh.kind == "box" {
        let (x0, y0) = (calc.min(sh.x, sh.x + sh.w), calc.min(sh.y, sh.y + sh.h))
        place(top + left, dx: x0 * s, dy: y0 * s, rect(width: calc.abs(sh.w) * s, height: calc.abs(sh.h) * s, stroke: st))
      } else if sh.kind == "ellipse" {
        place(top + left, dx: (sh.cx - sh.rx) * s, dy: (sh.cy - sh.ry) * s, ellipse(width: 2 * sh.rx * s, height: 2 * sh.ry * s, stroke: st))
      } else if sh.kind == "line" {
        place(top + left, line(start: (sh.x1 * s, sh.y1 * s), end: (sh.x2 * s, sh.y2 * s), stroke: st))
      } else if sh.kind == "polygon" and sh.points.len() >= 2 {
        place(top + left, polygon(stroke: st, ..sh.points.map(pt)))
      } else if sh.kind == "polyline" and sh.points.len() >= 2 {
        place(top + left, curve(stroke: st, curve.move(pt(sh.points.at(0))), ..sh.points.slice(1).map(p => curve.line(pt(p)))))
      } else if sh.kind == "wand" {
        for lp in m.outline {
          if lp.len() >= 3 { place(top + left, polygon(stroke: st, ..lp.map(pt))) }
        }
        let (x, y) = (sh.x * s, sh.y * s)
        place(top + left, dx: x - 2pt, dy: y - 2pt, circle(radius: 2pt, stroke: st))
      }
    }
  }
  for a in img.annotations {
    if a.kind == "sketch" and a.points.len() >= 2 {
      let col = rgb(a.color.at(0), a.color.at(1), a.color.at(2))
      place(top + left, curve(stroke: (paint: col, thickness: 1.4pt, cap: "round", join: "round"), curve.move(pt(a.points.at(0))), ..a.points.slice(1).map(p => curve.line(pt(p)))))
    } else if a.kind == "text" {
      let (x, y) = (a.x * s, a.y * s)
      place(top + left, dx: x - 2.5pt, dy: y - 2.5pt, circle(radius: 2.5pt, fill: rgb("#facc15"), stroke: 0.6pt + black))
    }
  }
})

// Labels as (x, y, text, kind, colour) in thermal pixels; drawn upright over the rotated view.
#let label-points(img) = {
  let out = ()
  for f in img.findings {
    out.push((f.area.x, f.area.y + f.area.h, f.label, "tag", severity-color(f.level)))
  }
  for m in img.measurements {
    let sh = m.shape
    let (lx, ly) = if sh.kind == "spot" or sh.kind == "wand" { (sh.x, sh.y) } else if sh.kind == "box" {
      (calc.min(sh.x, sh.x + sh.w), calc.min(sh.y, sh.y + sh.h))
    } else if sh.kind == "ellipse" { (sh.cx - sh.rx, sh.cy - sh.ry) } else if sh.kind == "line" { (sh.x1, sh.y1) } else {
      (sh.points.at(0).at(0), sh.points.at(0).at(1))
    }
    out.push((lx, ly, m.label, "tag", black.transparentize(35%)))
  }
  for a in img.annotations {
    if a.kind == "text" { out.push((a.x, a.y, a.text, "note", none)) }
  }
  out
}

// The view `w` wide: crop, then rotation, as set in the app.
#let thermal-view(img, w) = {
  let (cx, cy, cw, ch) = (img.crop.at(0), img.crop.at(1), img.crop.at(2), img.crop.at(3))
  let th = img.rotation * 1deg
  let bw = calc.abs(cw * calc.cos(th)) + calc.abs(ch * calc.sin(th))
  let s = w / bw
  let bh = calc.abs(cw * calc.sin(th)) + calc.abs(ch * calc.cos(th))
  let cropped = box(width: cw * s, height: ch * s, clip: true, place(top + left, dx: -cx * s, dy: -cy * s, overlays(img, s)))
  box(width: bw * s, height: bh * s, {
    place(top + left, if img.rotation == 0 { cropped } else { rotate(th, reflow: true, cropped) })
    // Where each label point lands after the crop and the rotation about the crop's centre.
    for l in label-points(img) {
      let (x, y) = (l.at(0), l.at(1))
      if x >= cx and y >= cy and x <= cx + cw and y <= cy + ch {
        let dx = (x - cx - cw / 2) * s
        let dy = (y - cy - ch / 2) * s
        let rx = dx * calc.cos(th) - dy * calc.sin(th) + bw * s / 2
        let ry = dx * calc.sin(th) + dy * calc.cos(th) + bh * s / 2
        if l.at(3) == "note" {
          place(top + left, dx: rx + 4pt, dy: ry - 5pt, box(fill: rgb("#fef9c3"), stroke: 0.4pt + luma(120), inset: (x: 2.5pt, y: 1.5pt), radius: 1pt, text(7pt, fill: black, l.at(2))))
        } else {
          tag(rx, ry, l.at(2), fill: l.at(4))
        }
      }
    }
  })
}

#let colour-bar(img, w) = box(width: w, stack(
  spacing: 2pt,
  image(img.bar, width: w, height: 3.5mm, fit: "stretch", scaling: "smooth"),
  grid(columns: (auto, 1fr, auto), text(8pt, img.range_min), align(center, text(8pt, fill: muted, [#img.palette · #img.distribution])), text(8pt, img.range_max)),
))

#let kv-table(rows) = table(
  columns: (auto, 1fr),
  ..rows.map(r => (text(fill: muted, r.at(0)), r.at(1))).flatten(),
)

#let severity-cell(level, label) = [#swatch(severity-color(level)) #label]

// ---------------------------------------------------------------- cover

#if data.logo != none {
  align(right, image(data.logo, height: 16mm))
}
#block(width: 100%, inset: (bottom: 6pt), stroke: (bottom: 2pt + accent))[
  #text(20pt, weight: "bold", data.title)
  #if data.company != "" { linebreak(); text(11pt, fill: muted, data.company) }
]

#table(
  columns: (36mm, 1fr),
  stroke: none,
  inset: (x: 0pt, y: 3pt),
  ..data.cover_rows.map(r => (text(fill: muted, r.at(0)), r.at(1))).flatten(),
)

#if data.conditions.len() > 0 [
  == Conditions
  #kv-table(data.conditions)
]

#if data.sections.summary and data.summary != "" [
  == Summary
  #data.summary
]

#if data.sections.findingsOverview and data.overview.len() > 0 [
  == Findings
  #table(
    columns: (auto, auto, auto, auto, 1fr, 1fr),
    align: (left, left, left, right, left, left),
    head([Image], [Finding], [Type], [ΔT], [Severity], [Action]),
    ..data.overview.map(r => (r.image, r.label, r.kind, r.delta, severity-cell(r.level, r.severity), r.action)).flatten(),
  )
]

#if data.sections.contents and data.images.len() > 1 [
  == Contents
  #table(
    columns: (auto, 1fr, auto, auto),
    head([No.], [Image], [Measurements], [Findings]),
    ..data.images.map(img => (str(img.number), img.name, str(img.measurements.len()), str(img.findings.len()))).flatten(),
  )
]

#v(1fr)
#text(8pt, fill: muted)[#data.disclaimer \ Created with #data.generator.]

// ---------------------------------------------------------------- one section per image

#let image-section(img, name) = {
  if name == "isotherms" and img.isotherms.len() > 0 {
    [== Isotherms and alarms]
    for r in img.isotherms [#swatch(rgb(r.color)) #r.text \ ]
  } else if name == "measurements" and img.measurements.len() > 0 {
    [== Measurements]
    let alarms = img.measurements.any(m => m.alarm != "")
    let coverage = img.measurements.any(m => m.coverage != "")
    let cols = (1fr, auto, auto, auto, auto) + if alarms { (auto,) } else { () } + if coverage { (auto,) } else { () }
    table(
      columns: cols,
      align: (left, left, right, right, right, left, right),
      head([Label], [Tool], [Min], [Max], [Average], ..if alarms { ([Alarm],) } else { () }, ..if coverage { ([In isotherm],) } else { () }),
      ..img.measurements.map(m => (
        m.label, m.kind, m.min, m.max, m.avg,
        ..if alarms { (if m.alarm == "Alarm" { text(fill: severity-color(4), weight: "bold", m.alarm) } else { m.alarm },) } else { () },
        ..if coverage { (m.coverage,) } else { () },
      )).flatten(),
    )
  } else if name == "formulas" and img.formulas.len() > 0 {
    [== Formulas]
    table(
      columns: (auto, 1fr, auto),
      align: (left, left, right),
      head([Label], [Formula], [Result]),
      ..img.formulas.map(f => (f.at(0), raw(f.at(1)), f.at(2))).flatten(),
    )
  } else if name == "findings" and img.findings.len() > 0 {
    [== Findings]
    table(
      columns: (auto, auto, auto, auto, 1fr, 1fr),
      align: (left, left, right, right, left, left),
      head([Finding], [Type], [Temperature], [ΔT], [Severity], [Action]),
      ..img.findings.map(f => (f.label, if f.hot { "Hot" } else { "Cold" }, f.extreme, f.delta, severity-cell(f.level, f.severity), f.action)).flatten(),
    )
  } else if name == "electrical" and img.electrical.len() > 0 {
    [== Temperature rise (NETA)]
    table(
      columns: (auto, auto, auto, 1fr, 1fr),
      align: (left, right, right, left, left),
      head([Label], [Max], [Rise], [Severity], [Action]),
      ..img.electrical.flatten(),
    )
  } else if name == "building" and img.building.len() > 0 {
    [== Insulation and moisture]
    table(
      columns: (1fr, auto, auto, auto, auto),
      align: (left, right, right, right, left),
      head([Label], [Coldest], [Thermal index], [Surface RH], [Risk]),
      ..img.building.map(r => (r.at(0), r.at(1), r.at(2), r.at(3), if r.at(4) == "OK" or r.at(4) == "–" { r.at(4) } else { text(fill: severity-color(if r.at(4) == "Condensation" { 4 } else { 3 }), weight: "bold", r.at(4)) })).flatten(),
    )
  } else if name == "hvac" and img.hvac.len() > 0 {
    [== Temperature split]
    kv-table(img.hvac)
  } else if name == "profiles" and img.profiles.len() > 0 {
    [== Profiles]
    for c in img.profiles { layout(size => line-chart(c, size.width, 32mm)); v(3mm) }
  } else if name == "fields" and img.fields.len() > 0 {
    [== Inspection details]
    kv-table(img.fields)
  } else if name == "notes" and img.notes != "" {
    [== Notes]
    img.notes
  } else if name == "parameters" {
    [== Parameters]
    if img.params.len() == 0 {
      text(fill: muted)[This format stores final temperatures only.]
    } else {
      table(
        columns: (1fr, auto, auto),
        align: (left, right, right),
        head([Parameter], [Used], [In file]),
        ..img.params.map(r => (r.at(0), r.at(1), text(fill: muted, r.at(2)))).flatten(),
      )
    }
    if img.image_range != none {
      text(8pt, fill: muted)[Image temperatures: #img.image_range]
    }
  } else if name == "details" {
    [== Image details]
    kv-table(img.camera)
  }
}

#let two = data.images_per_page == 2
#for (i, img) in data.images.enumerate() {
  if not two or calc.rem(i, 2) == 0 {
    pagebreak()
  } else {
    v(4mm)
    line(length: 100%, stroke: 0.6pt + luma(200))
  }
  heading(level: 1, str(img.number) + ". " + img.name)
  if img.asset != "" { text(9pt, fill: muted)[Asset: #img.asset] }

  let portrait = img.height > img.width or calc.rem(calc.round(img.rotation), 180) == 90
  let tw = if two { 70mm } else if img.photo != none or portrait { 84mm } else { 120mm }
  let thermal-column = stack(spacing: 4pt, thermal-view(img, tw), colour-bar(img, tw))
  if img.photo != none {
    grid(columns: (tw, 1fr), gutter: 6mm, thermal-column, image(img.photo, width: 100%))
  } else {
    thermal-column
  }

  // Parameters and details side by side, the rest in template order.
  let order = data.image_sections
  let side = order.filter(n => n == "parameters" or n == "details")
  for name in order.filter(n => n != "parameters" and n != "details") {
    image-section(img, name)
  }
  if side.len() == 2 {
    grid(columns: (1fr, 1fr), gutter: 6mm, image-section(img, side.at(0)), image-section(img, side.at(1)))
  } else if side.len() == 1 {
    image-section(img, side.at(0))
  }
}

// ---------------------------------------------------------------- trends and signature

#if data.trends.len() > 0 {
  pagebreak()
  heading(level: 1, "Trends")
  text(9pt, fill: muted)[Images with the same asset tag, ordered by capture date.]
  v(2mm)
  for c in data.trends { layout(size => line-chart(c, size.width, 45mm)); v(5mm) }
}

#if data.sections.signature {
  v(10mm)
  grid(
    columns: (1fr, 1fr),
    gutter: 12mm,
    [#line(length: 100%, stroke: 0.6pt) #text(8pt, fill: muted)[Inspector signature]],
    [#line(length: 100%, stroke: 0.6pt) #text(8pt, fill: muted)[Date]],
  )
}

// Kelvinlens inspection report. All data arrives through `sys.inputs` (see report.rs).
#let data = sys.inputs
#let accent = rgb("#c2410c")
#let muted = luma(105)
// Header row with bold labels.
#let head(..cells) = table.header(..cells.pos().map(strong))

#set document(title: data.title)
#set page(
  paper: data.paper,
  margin: (x: 18mm, top: 18mm, bottom: 20mm),
  footer: context {
    set text(8pt, fill: muted)
    [#data.title #h(1fr) Page #counter(page).display() of #counter(page).final().first()]
  },
)
#set text(font: "Fira Sans", size: 9.5pt, lang: "en")
#set par(justify: false, leading: 0.6em)
#set table(stroke: 0.5pt + luma(205), inset: (x: 5pt, y: 4pt))
#show heading.where(level: 1): set text(15pt)
#show heading.where(level: 2): it => block(above: 1.1em, below: 0.6em, text(11pt, weight: "bold", it.body))

// ---------------------------------------------------------------- cover

#block(width: 100%, inset: (bottom: 6pt), stroke: (bottom: 2pt + accent))[
  #text(20pt, weight: "bold", data.title)
]

#let cover-rows = (
  ("Site", data.site),
  ("Inspector", data.inspector),
  ("Date", data.date),
  ("Images", str(data.images.len())),
).filter(r => r.at(1) != "")

#table(
  columns: (32mm, 1fr),
  stroke: none,
  inset: (x: 0pt, y: 3pt),
  ..cover-rows.map(r => (text(fill: muted, r.at(0)), r.at(1))).flatten(),
)

#if data.summary != "" [
  == Summary
  #data.summary
]

#if data.images.len() > 1 [
  == Contents
  #table(
    columns: (auto, 1fr, auto),
    head([No.], [Image], [Measurements]),
    ..data.images.enumerate().map(((i, img)) => (str(i + 1), img.name, str(img.measurements.len()))).flatten(),
  )
]

#v(1fr)
#text(8pt, fill: muted)[#data.disclaimer \ Created with #data.generator.]

// ---------------------------------------------------------------- helpers

// Thermal image with measurement overlays, `w` wide.
#let thermal-view(img, w) = {
  let s = w / img.width
  let h = img.height * s
  let outline = 1.8pt + black.transparentize(45%)
  let line-stroke = 0.8pt + white
  let tag(x, y, label) = place(top + left, dx: x * s + 3pt, dy: y * s + 2pt,
    box(fill: black.transparentize(35%), inset: (x: 2pt, y: 1.5pt), radius: 1pt, text(7pt, fill: white, label)))
  box(width: w, height: h, clip: true, {
    place(top + left, image(img.thermal, width: w, height: h, scaling: "pixelated"))
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
          place(top + left, dx: (sh.cx - sh.rx) * s, dy: (sh.cy - sh.ry) * s,
            ellipse(width: 2 * sh.rx * s, height: 2 * sh.ry * s, stroke: st))
        } else if sh.kind == "line" {
          place(top + left, line(start: (sh.x1 * s, sh.y1 * s), end: (sh.x2 * s, sh.y2 * s), stroke: st))
        }
      }
      let (lx, ly) = if sh.kind == "spot" { (sh.x, sh.y) } else if sh.kind == "box" {
        (calc.min(sh.x, sh.x + sh.w), calc.min(sh.y, sh.y + sh.h))
      } else if sh.kind == "ellipse" { (sh.cx - sh.rx, sh.cy - sh.ry) } else { (sh.x1, sh.y1) }
      tag(lx, ly, m.label)
    }
  })
}

#let colour-bar(img, w) = box(width: w, stack(
  spacing: 2pt,
  image(img.bar, width: w, height: 3.5mm, fit: "stretch", scaling: "smooth"),
  grid(columns: (auto, 1fr, auto), text(8pt, img.range_min), align(center, text(8pt, fill: muted, img.palette)), text(8pt, img.range_max)),
))


// ---------------------------------------------------------------- one section per image

#for (i, img) in data.images.enumerate() {
  pagebreak()
  heading(level: 1, str(i + 1) + ". " + img.name)

  let portrait = img.height > img.width
  let tw = if img.photo != none or portrait { 84mm } else { 120mm }
  let thermal-column = stack(spacing: 4pt, thermal-view(img, tw), colour-bar(img, tw))
  if img.photo != none {
    grid(columns: (tw, 1fr), gutter: 6mm, thermal-column, image(img.photo, width: 100%))
  } else {
    thermal-column
  }

  if img.measurements.len() > 0 {
    [== Measurements]
    table(
      columns: (1fr, auto, auto, auto, auto),
      align: (left, left, right, right, right),
      head([Label], [Tool], [Min], [Max], [Average]),
      ..img.measurements.map(m => (m.label, m.kind, m.min, m.max, m.avg)).flatten(),
    )
  }

  if img.notes != "" [
    == Notes
    #img.notes
  ]

  grid(columns: (1fr, 1fr), gutter: 6mm,
    {
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
    },
    {
      [== Image details]
      table(
        columns: (auto, 1fr),
        ..img.camera.map(r => (text(fill: muted, r.at(0)), r.at(1))).flatten(),
      )
    },
  )
}

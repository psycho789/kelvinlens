# Kelvinlens

**A free thermal image viewer, analyser and report tool for Linux, macOS and Windows.**

Open radiometric photos from thermal cameras, correct the measurement parameters, measure
temperatures and produce PDF inspection reports. Kelvinlens runs entirely offline: it has no
network access and never uploads your images.

## Features

**Files**
- FLIR radiometric JPEGs (including DJI Zenmuse XT2/XTR) and **FLIR radiometric video** (`.seq`,
  `.csq`), Fluke `.is2`, InfiRay/Topdon `.irg`, HTI/ToolTop, and 16-bit PNG or 32-bit TIFF
  thermograms (decoding by [libblackbody](https://crates.io/crates/libblackbody)).
- **InfiRay P2 Pro** photos saved by its phone app: temperatures, capture settings, serial number
  and the camera's centre reading, turned to match how the photo was taken.
- Lens, field of view, GPS, altitude, heading and drone gimbal angles where the file records them.

**Temperatures**
- Recalculates every pixel when you change emissivity, reflected temperature, distance,
  humidity, air temperature or IR-window settings (FLIR raw data; Fluke correction model).
  For P2 Pro photos, emissivity and reflected temperature can be changed using the same correction
  as Fluke; this is an approximation of what the camera would compute.
- Saves a **copy of a FLIR JPEG with the new parameters written in**, which FLIR software reads too.

**Measurements and analysis**
- Spot, box, ellipse, line, **polygon, polyline and magic wand** tools: min, max, average,
  standard deviation, ΔT, and a **temperature profile** along lines.
- **Formulas** over measurement results (`Bx1.max - Sp1`, `avg(Sp1, Sp2)`, …).
- **Alarms** per measurement, and **isotherms**: above, below, interval, **condensation /
  mould risk** (dew point) and **insulation** (thermal index), with the share of each area they cover.
- **Auto-inspect**: finds hot or cold anomalies and grades them (NETA MTS Table 100.18 for
  electrical work, thermal index for buildings).
- **Hotspot triage** of a whole folder, **trends** across visits (by asset tag), a **map** of image
  positions, **3D relief** view, and **panorama stitching** into a new radiometric image.

**Display**
- Iron, Rainbow, Arctic, Lava, grey and scientific palettes, the **camera's own palette** and
  **custom palettes**; temperature-linear, signal-linear, histogram-equalised and
  detail-enhanced colour distribution.
- Thermal, photo, blend, **edges (MSX-style)** and **thermal fusion** views; free rotation and crop;
  notes and sketches on the image.

**Video**
- Play and step through SEQ/CSQ recordings, apply parameters to all frames, plot measurements
  over time, **high-sensitivity (gas) view** with binning, and export segments as SEQ or AVI.

**Output**
- **Reports** as PDF or HTML from templates (Standard, Electrical, Building/moisture, HVAC,
  Compact) or your own, with company name, logo, colours, fields and sections; **Rapid report**
  in one click.
- Annotated PNG/JPEG with text overlay and logo, temperatures as CSV, radiometric TIFF, KML/GeoJSON
  maps, measurement tables, settings files.
- **Batch processing**: apply palette, colour distribution, scale, isotherms, rotation,
  parameters, measurements and formulas to many images and export them all.
- Edits are saved next to each image (`<file>.kelvinlens.json`); originals are never changed.
- °C, °F or K. Entirely offline.

## Download

Get the installer for your system from the
[latest release](https://github.com/psycho789/kelvinlens/releases/latest).

The builds are not code-signed yet, so the first launch shows a warning:

- **macOS:** open the app, then **System Settings → Privacy & Security → Open Anyway**.
- **Windows:** SmartScreen → **More info → Run anyway**.
- **Linux:** AppImage needs `chmod +x`; `.deb` and `.rpm` install normally.

## Accuracy

Temperatures are checked automatically on every change against real camera files:

| File | Camera shows | Kelvinlens |
|---|---|---|
| FLIR E40, stored spot | 20.9 °C | 20.92 °C |
| FLIR AX8, whole image | max 25.5 / min 24.3 / avg 25.1 °C | 25.47 / 24.36 / 25.03 °C |
| FLIR B60, centre | 19.0 °F (−7.22 °C) | −7.26 °C (3×3 mean) |

More files are checked against an independent decoder (exiftool, and CharLS for compressed
video): FLIR T640 and a portrait FLIR camera, DJI Zenmuse XT2 and XTR, and the first frames of
a FLIR SEQ and CSQ recording all agree to 0.01 °C.

Recalculation with changed parameters matches an independent implementation of the published
FLIR model to within 0.02 °C (see `crates/kelvinlens-core/tests/golden.rs`). Accuracy still
depends on the camera's calibration and on entering parameters that match the real scene.

## Building from source

Requirements: [Rust](https://rustup.rs) (stable), Node.js 22, and on Linux the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(`libwebkit2gtk-4.1-dev`, `libxdo-dev`, `libssl-dev`, `librsvg2-dev`).

```bash
npm ci
npm run tauri dev           # run with live reload
npm run tauri build         # installers in target/release/bundle/

scripts/fetch-testdata.sh   # sample thermograms for the tests
cargo test --workspace
```

`kelvinlens --self-test <file>` decodes a file and prints its centre temperature without opening a
window.

### Layout

| Path | What |
|---|---|
| `crates/kelvinlens-core` | Opening files and video, recalculation, measurements, formulas, isotherms, inspection, rendering, panoramas, exports, PDF/HTML reports (Typst) |
| `src-tauri` | Desktop shell: the commands the UI calls |
| `src` | UI (TypeScript, no framework) |
| `.github/workflows` | CI on Linux, macOS and Windows; automatic releases |

### Releasing

`scripts/bump-version.sh 0.2.0`, commit and push to `main`. When CI passes, the Release workflow
builds installers for all platforms and publishes the GitHub release.

## Licence

[EUPL-1.2](LICENSE), the same licence as libblackbody. The bundled Fira Sans fonts are under the
SIL Open Font License (`crates/kelvinlens-core/assets/fonts/OFL.txt`). Test images are downloaded
at test time from [thermal_parser](https://github.com/SanNianYiSi/thermal_parser) (MIT) and test
videos from [Thermimage](https://github.com/gtatters/Thermimage); neither is redistributed here.

Kelvinlens is an independent project, not affiliated with FLIR, Fluke, InfiRay, Topdon, HTI or
any camera maker. Brand names are used only to describe file compatibility.

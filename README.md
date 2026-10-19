# Kelvinlens

**A free thermal image viewer, analyser and report tool for Linux, macOS and Windows.**

Open radiometric photos from thermal cameras, correct the measurement parameters, measure
temperatures and produce PDF inspection reports. Kelvinlens runs entirely offline: it has no
network access and never uploads your images.

## Features

- **Opens** FLIR radiometric JPEGs, Fluke `.is2`, InfiRay/Topdon `.irg`, HTI/ToolTop, and 16-bit PNG
  or TIFF thermograms (decoding by [libblackbody](https://crates.io/crates/libblackbody)).
- **Recalculates temperatures** when you change emissivity, reflected temperature, distance,
  humidity, air temperature or IR-window settings, using the camera's raw data (FLIR) or the
  file's correction model (Fluke).
- **Measures** with spot, box, ellipse and line tools: minimum, maximum, average and ΔT. The
  camera's own measurements are imported.
- **Displays** with Iron, Rainbow, grey and scientific palettes, adjustable range, histogram,
  hot and cold spot markers, and the camera's photo or a blend of both.
- **Exports** the annotated image (PNG), all temperatures (CSV) and **PDF reports** with images,
  measurements, parameters, notes and camera details.
- **Saves your edits** next to each image (`<file>.kelvinlens.json`); originals are never changed.
- **Batch:** open a whole folder and apply parameters to every image.
- °C, °F or K.

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
| `crates/kelvinlens-core` | Opening files, recalculation, measurements, palettes, CSV/PNG export, PDF reports (Typst) |
| `src-tauri` | Desktop shell: the commands the UI calls |
| `src` | UI (TypeScript, no framework) |
| `.github/workflows` | CI on Linux, macOS and Windows; automatic releases |

### Releasing

`scripts/bump-version.sh 0.2.0`, commit and push to `main`. When CI passes, the Release workflow
builds installers for all platforms and publishes the GitHub release.

## Licence

[EUPL-1.2](LICENSE), the same licence as libblackbody. The bundled Fira Sans fonts are under the
SIL Open Font License (`crates/kelvinlens-core/assets/fonts/OFL.txt`). Test images are downloaded
at test time from [thermal_parser](https://github.com/SanNianYiSi/thermal_parser) (MIT) and are not
redistributed here.

Kelvinlens is an independent project, not affiliated with FLIR, Fluke, InfiRay, Topdon, HTI or
any camera maker. Brand names are used only to describe file compatibility.

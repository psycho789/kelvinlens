import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { pickers } from "./pickers";
import { mapDialog, panoramaDialog, seriesDialog, trendDialog, triageDialog } from "./analysisDialogs";
import { api, type Distribution, type ImageInfo, type Params, type Range, type Unit } from "./api";
import { batchDialog } from "./batch";
import { $, debounce, errorText, h, perFrame, toast } from "./dom";
import { encode, exportOptions, exportPng, renderExport } from "./exportImage";
import { modal, progress, promptAt } from "./modal";
import { Panel, alarmOn, gradientCss } from "./panel";
import { paletteEditor } from "./paletteEditor";
import { reliefDialog } from "./relief3d";
import { rapidInfo, reportDialog } from "./reportDialog";
import {
  DEFAULT_EXPORT,
  DEFAULT_INSPECT,
  currentDoc,
  displayRange,
  finiteRange,
  isArea,
  loadSettings,
  lookOf,
  nextLabel,
  palette,
  saveSettings,
  state,
  type Doc,
  type Tool,
  type View,
} from "./state";
import { templateEditor } from "./templateEditor";
import { UNITS, symbol, temp } from "./units";
import { Viewer } from "./viewer";

// ---------------------------------------------------------------------------- layout

const TOOLS: [Tool, string, string, string][] = [
  ["select", "Select", "V", "M4 3l14 8-6 1.6L9.5 19z"],
  ["spot", "Spot", "S", "M12 5v4M12 15v4M5 12h4M15 12h4M12 12m-2.2 0a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0"],
  ["box", "Box", "B", "M5 6h14v12H5z"],
  ["ellipse", "Ellipse", "E", "M12 6c4.4 0 8 2.7 8 6s-3.6 6-8 6-8-2.7-8-6 3.6-6 8-6z"],
  ["line", "Line", "L", "M5 18L19 6"],
  ["polygon", "Polygon", "P", "M6 7l9-3 5 8-6 8-9-4z"],
  ["polyline", "Polyline", "Y", "M4 17l5-8 5 6 6-9"],
  ["wand", "Wand", "W", "M5 19L15 9M15 4v2M19 8h2M17.5 5.5l1.5-1.5M13 7l4 4"],
  ["text", "Note", "N", "M5 5h14v10H10l-4 4v-4H5z"],
  ["sketch", "Sketch", "K", "M4 18c3-6 5 2 8-4s4-6 8-8"],
];

function icon(path: string): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", path);
  svg.append(p);
  return svg;
}

type MenuItem = [label: string, action: () => void, enabled?: () => boolean] | "-";

/** A toolbar button with a drop-down list of actions. */
function menu(id: string, label: string, items: MenuItem[], primary = false): HTMLElement {
  const list = h("div", { class: "menu-list", role: "menu" });
  const button = h("button", { id, class: `menu-button ${primary ? "primary" : ""}`, "aria-haspopup": "menu" }, `${label} ▾`);
  const wrap = h("div", { class: "menu" }, button, list);
  const close = () => wrap.classList.remove("open");
  button.addEventListener("click", (e) => {
    e.stopPropagation();
    const opening = !wrap.classList.contains("open");
    document.querySelectorAll(".menu.open").forEach((m) => m.classList.remove("open"));
    if (!opening) return;
    list.replaceChildren(
      ...items.map((it) =>
        it === "-"
          ? h("hr")
          : h(
              "button",
              {
                role: "menuitem",
                disabled: it[2] ? !it[2]() : false,
                onclick: () => {
                  close();
                  it[1]();
                },
              },
              it[0],
            ),
      ),
    );
    wrap.classList.add("open");
  });
  return wrap;
}
document.addEventListener("click", () => document.querySelectorAll(".menu.open").forEach((m) => m.classList.remove("open")));

const hasDoc = () => !!currentDoc();
const app = $("#app");
app.append(
  h(
    "header",
    { class: "toolbar" },
    h("div", { class: "brand" }, h("img", { src: "/icon.png", alt: "" }), h("span", {}, "Kelvinlens")),
    h(
      "div",
      { class: "group" },
      h("button", { id: "open", title: "Open images or videos (Ctrl/⌘ O)" }, "Open"),
      h("button", { id: "open-folder", title: "Open every image in a folder (Ctrl/⌘ Shift O)" }, "Folder"),
    ),
    h(
      "div",
      { class: "group tools", role: "toolbar", "aria-label": "Tools" },
      TOOLS.map(([tool, label, key, path]) =>
        h("button", { class: "tool", "data-tool": tool, title: `${label} (${key})`, "aria-label": label }, icon(path), h("span", {}, label)),
      ),
    ),
    h("div", { class: "spacer" }),
    h(
      "div",
      { class: "group" },
      menu("menu-analyze", "Analyze", [
        ["Auto-inspect this image", () => void runInspect(), hasDoc],
        ["Hotspot triage…", () => void triage()],
        ["Panorama…", () => void panorama()],
        ["Trends…", () => void trendDialog()],
        ["Map of positions…", () => void mapDialog((i) => selectDoc(i))],
        ["3D relief…", () => void (currentDoc() && reliefDialog(currentDoc()!)), hasDoc],
      ]),
      h("button", { id: "batch", title: "Apply settings and export many images at once" }, "Batch…"),
      h("button", { id: "save", title: "Save edits next to the image (Ctrl/⌘ S)" }, "Save"),
      menu("menu-export", "Export", [
        ["Image (PNG/JPEG)…", () => void exportImage(), hasDoc],
        ["Temperatures (CSV)…", () => void exportCsv(), hasDoc],
        ["Radiometric TIFF…", () => void exportTiff(), hasDoc],
        ["FLIR copy with these parameters…", () => void exportFlirCopy(), () => !!currentDoc()?.info.flirJpeg],
        ["Map (KML/GeoJSON)…", () => void mapDialog((i) => selectDoc(i))],
        "-",
        ["Export settings (JSON)…", () => void exportSettings(), hasDoc],
        ["Import settings (JSON)…", () => void importSettings(), hasDoc],
      ]),
      menu(
        "menu-report",
        "Report",
        [
          ["Create report…", () => void createReport(), hasDoc],
          ["Rapid report (one click)", () => void rapidReport(), hasDoc],
          "-",
          ["Edit templates…", () => void templateEditor().then(() => panel.renderInspection(currentDoc()))],
        ],
        true,
      ),
    ),
    h("div", { class: "segmented units", role: "group", "aria-label": "Temperature unit" }, UNITS.map((u) => h("button", { "data-unit": u }, symbol(u)))),
  ),
  h("aside", { class: "sidebar" }, h("div", { class: "sidebar-head" }, h("h2", {}, "Images"), h("span", { id: "count", class: "muted" })), h("ul", { id: "files", class: "files" })),
  h(
    "main",
    { class: "stage" },
    h("div", { id: "viewer", class: "viewer" }),
    h(
      "div",
      { id: "empty", class: "empty" },
      h("img", { src: "/icon.png", alt: "", class: "empty-icon" }),
      h("h1", {}, "Open a thermal image"),
      h("p", {}, "Drop files or a folder here, or use Open. Everything stays on this computer."),
      h("div", { class: "row buttons" }, h("button", { class: "primary", id: "empty-open" }, "Open images…"), h("button", { id: "empty-folder" }, "Open folder…")),
      h("p", { class: "muted small" }, "FLIR radiometric JPEG and SEQ/CSQ video · DJI Zenmuse XT2/XTR · Fluke IS2 · InfiRay P2 Pro JPEG · InfiRay/Topdon IRG · HTI · 16-bit PNG and TIFF"),
    ),
    h("div", { id: "drop", class: "drop-overlay" }, h("div", {}, "Drop to open")),
    h("div", { id: "tool-hint", class: "tool-hint", hidden: true }),
    h(
      "div",
      { class: "zoombar" },
      h("button", { id: "zoom-out", title: "Zoom out (−)", "aria-label": "Zoom out" }, "−"),
      h("button", { id: "zoom-fit", title: "Fit to window (F)" }, "100%"),
      h("button", { id: "zoom-in", title: "Zoom in (+)", "aria-label": "Zoom in" }, "+"),
    ),
  ),
  h("aside", { id: "panel", class: "panel" }),
  h("footer", { class: "statusbar" }, h("span", { id: "readout", class: "readout" }), h("span", { id: "status-file", class: "muted" }), h("span", { class: "spacer" }), h("span", { id: "busy", class: "muted" })),
);

// ---------------------------------------------------------------------------- viewer & panel

function lookKey(doc: Doc): string {
  const l = lookOf(doc);
  return `${l.palette.id}|${l.palette.colors.length}|${l.palette.colors[0]}|${l.palette.colors[l.palette.colors.length - 1]}|${l.range[0]}|${l.range[1]}|${l.distribution}|${JSON.stringify(l.isotherms)}`;
}

const viewer = new Viewer($("#viewer"), {
  hover: (pos) => showReadout(pos),
  measurementsChanged: (final) => {
    const doc = currentDoc();
    if (!doc) return;
    markDirty(doc);
    if (final) void refreshAnalysis(doc);
    else measureSoon(doc);
  },
  select: (index) => {
    state.selected = index;
    panel.renderMeasurements(currentDoc());
  },
  zoomChanged: () => updateZoomLabel(),
  newLabel: (kind) => nextLabel(currentDoc()!, kind),
  annotationsChanged: () => {
    const doc = currentDoc();
    if (!doc) return;
    markDirty(doc);
    panel.renderInspection(doc);
  },
  cropChanged: (crop) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.presentation = { ...doc.presentation, crop };
    markDirty(doc);
    setTool("select");
    refreshView();
    panel.renderDisplay(doc);
  },
  requestText: (at) => promptAt($(".stage"), at, "Note text, then Enter"),
  wandTolerance: () => state.wandTolerance,
  sketchColor: () => state.sketchColor,
  renderThermal: (doc) => api.render(doc.info.id, lookOf(doc), doc.info.width, doc.info.height),
  renderComposite: (doc) => {
    const range = displayRange(doc);
    const fusion =
      state.view === "msx"
        ? { mode: "msx" as const, strength: state.msxStrength }
        : { mode: "interval" as const, lo: (state.fusionRange ?? [range[0] + (range[1] - range[0]) * 0.6, range[1]])[0], hi: (state.fusionRange ?? [range[0], range[1]])[1] };
    return api.fuse(doc.info.id, lookOf(doc), fusion, 1600, doc.presentation.align ?? null);
  },
});

const panel = new Panel($("#panel"), {
  setPalette: (id) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.palette = id;
    if (id !== "camera") {
      state.defaultPalette = id;
      saveSettings({ defaultPalette: id });
    }
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
    renderFiles();
  },
  editPalettes: () =>
    void paletteEditor().then((id) => {
      const doc = currentDoc();
      if (id && doc) panelHandlers().setPalette(id);
      else panel.renderDisplay(doc);
    }),
  setRange: (range) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.range = range;
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
  },
  setDistribution: (d: Distribution) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.distribution = d;
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
  },
  setView: (view: View) => {
    state.view = view;
    viewer.view = view;
    viewer.resetComposite();
    viewer.fit();
    refreshView();
    panel.renderDisplay(currentDoc());
  },
  setBlend: (value) => {
    state.blend = value;
    viewer.blend = value;
    viewer.draw();
  },
  setMsxStrength: (value) => {
    state.msxStrength = value;
    saveSettings({ msxStrength: value });
    refreshView();
  },
  setFusionRange: (range) => {
    state.fusionRange = range[0] <= range[1] ? range : [range[1], range[0]];
    refreshView();
    panel.renderDisplay(currentDoc());
  },
  setSmooth: (on) => {
    state.smooth = on;
    viewer.smooth = on;
    saveSettings({ smooth: on });
    viewer.draw();
  },
  setShowExtremes: (on) => {
    state.showExtremes = on;
    viewer.showExtremes = on;
    saveSettings({ showExtremes: on });
    viewer.draw();
  },
  setRotation: (deg) => {
    const doc = currentDoc();
    if (!doc) return;
    const r = ((deg % 360) + 360) % 360;
    doc.presentation = { ...doc.presentation, rotation: Math.round(r * 100) / 100 };
    markDirty(doc);
    viewer.fit();
    panel.renderDisplay(doc);
  },
  setAlign: (align) => {
    const doc = currentDoc();
    if (!doc) return;
    const neutral = !align || (align[0] === 0 && align[1] === 0 && align[2] === 1);
    doc.presentation = { ...doc.presentation, align: neutral ? null : align };
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
  },
  startCrop: () => {
    setTool("crop");
  },
  clearCrop: () => {
    const doc = currentDoc();
    if (!doc) return;
    doc.presentation = { ...doc.presentation, crop: null };
    markDirty(doc);
    viewer.fit();
    panel.renderDisplay(doc);
  },
  setParams: (params) => {
    const doc = currentDoc();
    if (doc) void applyParams(doc, params);
  },
  resetParams: () => {
    const doc = currentDoc();
    if (doc) void resetParams(doc);
  },
  applyParamsToAll: () => void applyParamsToAll(),
  select: (index) => {
    state.selected = index;
    viewer.selected = index;
    viewer.draw();
    panel.renderMeasurements(currentDoc());
  },
  renameMeasurement: (index, label) => {
    const doc = currentDoc();
    const m = doc?.measurements[index];
    if (!doc || !m) return;
    m.label = label;
    markDirty(doc);
    viewer.draw();
    void refreshAnalysis(doc);
  },
  deleteMeasurement: (index) => deleteMeasurement(index),
  setAlarm: (index, alarm) => {
    const doc = currentDoc();
    const m = doc?.measurements[index];
    if (!doc || !m) return;
    m.alarm = alarm && (alarm.above != null || alarm.below != null) ? alarm : alarm ? { ...alarm } : null;
    markDirty(doc);
    updateAlarms(doc);
    panel.renderMeasurements(doc);
  },
  setWandTolerance: (index, tolerance) => {
    const doc = currentDoc();
    const m = doc?.measurements[index];
    if (!doc || !m || m.shape.kind !== "wand") return;
    m.shape = { ...m.shape, tolerance };
    state.wandTolerance = tolerance;
    markDirty(doc);
    void refreshAnalysis(doc);
  },
  setFormulas: (formulas) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.formulas = formulas;
    markDirty(doc);
    void refreshAnalysis(doc);
  },
  setIsotherms: (isotherms) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.isotherms = isotherms;
    markDirty(doc);
    refreshView();
    panel.renderIsotherms(doc);
    void refreshAnalysis(doc);
  },
  runInspect: () => void runInspect(),
  clearFindings: () => {
    const doc = currentDoc();
    if (!doc) return;
    doc.findings = [];
    markDirty(doc);
    viewer.draw();
    panel.renderInspect(doc);
  },
  findingsToMeasurements: () => {
    const doc = currentDoc();
    if (!doc) return;
    for (const f of doc.findings) {
      if (!doc.measurements.some((m) => m.label === f.label)) doc.measurements.push({ label: f.label, shape: structuredClone(f.area) });
    }
    markDirty(doc);
    void refreshAnalysis(doc);
    viewer.draw();
  },
  setAsset: (asset) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.asset = asset;
    markDirty(doc);
    renderFiles();
  },
  setField: (name, value) => {
    const doc = currentDoc();
    if (!doc) return;
    if (value.trim()) doc.fields[name] = value;
    else delete doc.fields[name];
    markDirty(doc);
  },
  setAnnotationText: (index, text) => {
    const doc = currentDoc();
    const a = doc?.annotations[index];
    if (!doc || !a || a.kind !== "text") return;
    if (text.trim()) a.text = text.trim();
    else doc.annotations.splice(index, 1);
    markDirty(doc);
    viewer.draw();
    panel.renderInspection(doc);
  },
  deleteAnnotation: (index) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.annotations.splice(index, 1);
    markDirty(doc);
    viewer.draw();
    panel.renderInspection(doc);
  },
  setNotes: (notes) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.notes = notes;
    markDirty(doc);
  },
  seek: (frame) => void seek(frame),
  togglePlay: () => togglePlay(),
  setHsm: (patch) => {
    const doc = currentDoc();
    if (!doc?.video) return;
    Object.assign(doc.video, patch);
    void updateHsm(doc);
    panel.renderVideo(doc);
  },
  setInOut: (first, last) => {
    const doc = currentDoc();
    if (!doc?.video) return;
    doc.video.first = first;
    doc.video.last = last;
    panel.renderVideo(doc);
  },
  plotSeries: () => void plotSeries(),
  exportVideo: () => void exportVideo(),
  exportSegment: () => void exportSegment(),
});

function panelHandlers() {
  return (panel as unknown as { on: { setPalette(id: string): void } }).on;
}

function refreshView(): void {
  const doc = currentDoc();
  viewer.setDoc(doc, false);
  if (!doc) return;
  viewer.lookKey = lookKey(doc);
  const range = displayRange(doc);
  viewer.fusionKey = `${state.view === "msx" ? `msx${state.msxStrength}` : `iv${JSON.stringify(state.fusionRange ?? range)}`}|${JSON.stringify(doc.presentation.align ?? null)}`;
  viewer.selected = state.selected;
  viewer.alarms = doc.measurements.map((_, i) => alarmOn(doc, i));
  viewer.draw();
}

// ---------------------------------------------------------------------------- opening files

async function openPaths(paths: string[]): Promise<void> {
  const files: string[] = [];
  for (const p of paths) {
    try {
      if (await api.isFolder(p)) files.push(...(await api.listFolder(p)));
      else files.push(p);
    } catch (e) {
      toast(`${p}: ${errorText(e)}`, "error");
    }
  }
  if (files.length === 0) {
    if (paths.length > 0) toast("No supported images found there.", "error");
    return;
  }
  const failed: string[] = [];
  let firstNew = -1;
  setBusy(`Opening ${files.length} file${files.length > 1 ? "s" : ""}…`);
  for (const path of files) {
    const existing = state.docs.findIndex((d) => d.info.path === path);
    if (existing >= 0) {
      if (firstNew < 0) firstNew = existing;
      continue;
    }
    try {
      const doc = await loadDoc(await api.openImage(path));
      state.docs.push(doc);
      if (firstNew < 0) firstNew = state.docs.length - 1;
      renderFiles();
    } catch (e) {
      failed.push(`${basename(path)} (${errorText(e).replace(/^Could not open thermogram: /, "")})`);
    }
  }
  setBusy("");
  if (failed.length) {
    const shown = failed.slice(0, 3).join(", ");
    toast(`Could not open ${failed.length} file${failed.length > 1 ? "s" : ""}: ${shown}${failed.length > 3 ? " …" : ""}`, "error");
  }
  if (firstNew >= 0) selectDoc(firstNew);
}

async function loadDoc(info: ImageInfo): Promise<Doc> {
  const [kelvin, photo] = await Promise.all([api.thermalData(info.id), info.photoSize ? api.photo(info.id).catch(() => null) : null]);
  const doc: Doc = {
    info,
    kelvin,
    dataRange: finiteRange(kelvin),
    photo,
    measurements: info.embeddedMeasurements.map((m) => ({ label: m.label || "", shape: m.shape })),
    stats: [],
    outlines: [],
    coverage: [],
    notes: "",
    palette: state.defaultPalette,
    range: null,
    distribution: "temperatureLinear",
    isotherms: [],
    formulas: [],
    formulaResults: [],
    annotations: [],
    findings: [],
    asset: "",
    fields: {},
    presentation: { rotation: 0, crop: null },
    params: info.params,
    dirty: false,
    includeInReport: true,
    video: info.video
      ? { frame: 0, playing: false, hsm: false, hsmReference: 8, hsmSpan: 0.5, binning: false, hsmData: null, first: 0, last: info.video.frames - 1 }
      : null,
  };
  // Cameras sometimes give several tools the same label; keep labels unique so formulas and
  // trends can tell them apart.
  const seen = new Set<string>();
  doc.measurements.forEach((m) => {
    if (!m.label || seen.has(m.label)) m.label = nextLabel(doc, m.shape.kind);
    seen.add(m.label);
  });

  const edits = info.sidecar;
  if (info.sidecarError) toast(info.sidecarError, "error");
  if (edits) {
    doc.measurements = edits.measurements;
    doc.notes = edits.notes;
    if (edits.palette && (edits.palette === "camera" ? !!info.embeddedPalette : palette(edits.palette, doc).id === edits.palette)) doc.palette = edits.palette;
    doc.range = edits.range;
    doc.distribution = edits.distribution ?? "temperatureLinear";
    doc.isotherms = edits.isotherms ?? [];
    doc.formulas = edits.formulas ?? [];
    doc.annotations = edits.annotations ?? [];
    doc.findings = edits.findings ?? [];
    doc.asset = edits.asset ?? "";
    doc.fields = edits.fields ?? {};
    doc.presentation = edits.presentation ?? { rotation: 0, crop: null };
    if (edits.params && info.editable.length > 0) {
      try {
        const r = await api.setParams(info.id, edits.params);
        doc.params = r.params;
        doc.kelvin = await api.thermalData(info.id);
        doc.dataRange = finiteRange(doc.kelvin);
      } catch (e) {
        toast(`${info.name}: saved parameters were not applied (${errorText(e)})`, "error");
      }
    }
  }
  await refreshAnalysis(doc, false);
  return doc;
}

function selectDoc(index: number): void {
  const previous = currentDoc();
  if (previous?.video?.playing) previous.video.playing = false;
  state.current = index < 0 || index >= state.docs.length ? -1 : index;
  state.selected = null;
  showReadout(null);
  const doc = currentDoc();
  $("#empty").hidden = !!doc;
  if (doc && !doc.photo && state.view !== "thermal") {
    state.view = "thermal";
    viewer.view = "thermal";
  }
  viewer.resetComposite();
  viewer.setDoc(doc, true);
  refreshView();
  panel.renderAll(doc);
  renderFiles();
  updateToolbar();
  updateZoomLabel();
  $("#status-file").textContent = doc ? `${doc.info.name} · ${doc.info.formatLabel} · ${doc.info.width} × ${doc.info.height}` : "";
}

async function closeDoc(index: number): Promise<void> {
  const doc = state.docs[index];
  if (!doc) return;
  if (doc.dirty && !(await pickers.ask(`Close ${doc.info.name} without saving your edits?`, { title: "Unsaved edits", kind: "warning" }))) return;
  if (doc.video) doc.video.playing = false;
  state.docs.splice(index, 1);
  void api.closeImage(doc.info.id);
  const next = state.current > index ? state.current - 1 : Math.min(state.current, state.docs.length - 1);
  selectDoc(next);
}

// ---------------------------------------------------------------------------- analysis

/** Measurement statistics, wand outlines, isotherm coverage and formulas for the doc. */
async function refreshAnalysis(doc: Doc, render = true): Promise<void> {
  const id = doc.info.id;
  const shapes = doc.measurements.map((m) => m.shape);
  try {
    const [stats, outlines, coverage, formulas] = await Promise.all([
      api.measure(id, shapes),
      shapes.some((s) => s.kind === "wand") ? api.outlines(id, shapes) : Promise.resolve(shapes.map(() => [])),
      doc.isotherms.length && shapes.some((s) => isArea(s.kind)) ? api.coverage(id, shapes, doc.isotherms) : Promise.resolve(shapes.map(() => null)),
      doc.formulas.length ? api.evaluateFormulas(id, doc.measurements, doc.formulas, state.unit) : Promise.resolve([]),
    ]);
    doc.stats = stats;
    doc.outlines = outlines;
    doc.coverage = coverage;
    doc.formulaResults = formulas;
  } catch (e) {
    toast(errorText(e), "error");
  }
  if (!render) return;
  if (doc === currentDoc()) {
    updateAlarms(doc);
    panel.renderMeasurements(doc);
    panel.renderFormulas(doc);
  }
  renderFiles();
}

const measureSoon = perFrame((doc: Doc) => void refreshAnalysis(doc));

function updateAlarms(doc: Doc): void {
  viewer.alarms = doc.measurements.map((_, i) => alarmOn(doc, i));
  viewer.draw();
}

async function runInspect(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  try {
    doc.findings = await api.inspect(doc.info.id, state.inspect);
    saveSettings({ inspect: state.inspect });
    markDirty(doc);
    viewer.draw();
    panel.renderInspect(doc);
    toast(doc.findings.length ? `${doc.findings.length} finding${doc.findings.length > 1 ? "s" : ""}` : "No anomalies at these settings.");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---------------------------------------------------------------------------- parameters

async function reloadTemperatures(doc: Doc, params: Params): Promise<void> {
  doc.params = params;
  doc.kelvin = await api.thermalData(doc.info.id);
  doc.dataRange = finiteRange(doc.kelvin);
  await refreshAnalysis(doc, false);
  if (doc === currentDoc()) {
    viewer.dataChanged();
    refreshView();
    panel.renderDisplay(doc);
    panel.renderParams(doc);
    panel.renderMeasurements(doc);
    panel.renderFormulas(doc);
  }
}

async function applyParams(doc: Doc, params: Params): Promise<void> {
  try {
    const r = await api.setParams(doc.info.id, params);
    markDirty(doc);
    await reloadTemperatures(doc, r.params);
    if (doc.video?.hsm) await updateHsm(doc);
  } catch (e) {
    toast(errorText(e), "error");
    panel.renderParams(doc);
  }
}

async function resetParams(doc: Doc): Promise<void> {
  try {
    const r = await api.resetParams(doc.info.id);
    markDirty(doc);
    await reloadTemperatures(doc, r.params);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

async function applyParamsToAll(): Promise<void> {
  const source = currentDoc();
  if (!source) return;
  let applied = 0;
  for (const doc of state.docs) {
    if (doc === source || doc.info.editable.length === 0) continue;
    const params: Params = { ...doc.params };
    for (const key of doc.info.editable) if (source.info.editable.includes(key)) params[key] = source.params[key];
    try {
      const r = await api.setParams(doc.info.id, params);
      markDirty(doc);
      await reloadTemperatures(doc, r.params);
      applied++;
    } catch (e) {
      toast(`${doc.info.name}: ${errorText(e)}`, "error");
    }
  }
  toast(applied ? `Applied to ${applied} other image${applied > 1 ? "s" : ""}.` : "No other image supports these parameters.");
}

// ---------------------------------------------------------------------------- measurements

function deleteMeasurement(index: number): void {
  const doc = currentDoc();
  if (!doc || !doc.measurements[index]) return;
  doc.measurements.splice(index, 1);
  doc.stats.splice(index, 1);
  doc.outlines.splice(index, 1);
  doc.coverage.splice(index, 1);
  state.selected = null;
  viewer.selected = null;
  markDirty(doc);
  viewer.draw();
  void refreshAnalysis(doc);
}

// ---------------------------------------------------------------------------- video

async function seek(frame: number): Promise<void> {
  const doc = currentDoc();
  if (doc) await seekDoc(doc, frame);
}

async function seekDoc(doc: Doc, frame: number): Promise<void> {
  const v = doc.video;
  if (!v) return;
  const n = doc.info.video!.frames;
  frame = Math.max(0, Math.min(n - 1, frame));
  if (frame === v.frame && !v.playing) {
    if (doc === currentDoc()) panel.renderVideo(doc);
    return;
  }
  try {
    const r = await api.selectFrame(doc.info.id, frame);
    v.frame = frame;
    doc.params = r.params;
    doc.kelvin = await api.thermalData(doc.info.id);
    doc.dataRange = finiteRange(doc.kelvin);
    if (v.hsm) await updateHsm(doc, false);
    await refreshAnalysis(doc, false);
    if (doc === currentDoc()) {
      viewer.dataChanged();
      refreshView();
      panel.renderVideo(doc);
      panel.renderMeasurements(doc);
      panel.renderFormulas(doc);
      panel.renderDisplay(doc);
      $("#status-file").textContent = `${doc.info.name} · frame ${frame + 1}/${n} · ${doc.info.width} × ${doc.info.height}`;
    }
  } catch (e) {
    v.playing = false;
    toast(errorText(e), "error");
  }
}

async function updateHsm(doc: Doc, redraw = true): Promise<void> {
  const v = doc.video;
  if (!v) return;
  if (!v.hsm) {
    v.hsmData = null;
  } else {
    try {
      v.hsmData = await api.hsm(doc.info.id, v.frame, v.hsmReference, v.binning);
    } catch (e) {
      v.hsm = false;
      v.hsmData = null;
      toast(errorText(e), "error");
    }
  }
  if (redraw && doc === currentDoc()) {
    viewer.dataChanged();
    refreshView();
  }
}

function togglePlay(): void {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  v.playing = !v.playing;
  panel.renderVideo(doc);
  if (!v.playing) return;
  const info = doc.info.video!;
  const fps = Math.min(30, info.frameRate ?? 10);
  const step = async () => {
    if (!v.playing || doc !== currentDoc()) {
      v.playing = false;
      panel.renderVideo(currentDoc());
      return;
    }
    const started = performance.now();
    const next = v.frame + 1 > v.last ? v.first : v.frame + 1;
    await seek(next);
    const wait = Math.max(0, 1000 / fps - (performance.now() - started));
    setTimeout(() => void step(), wait);
  };
  void step();
}

async function plotSeries(): Promise<void> {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  if (doc.measurements.length === 0) {
    toast("Add measurements first; they are tracked across the frames.", "error");
    return;
  }
  setBusy("Measuring every frame…");
  try {
    const series = await api.videoSeries(doc.info.id, v.first, v.last, doc.measurements);
    setBusy("");
    await seriesDialog(doc, series, v.first);
  } catch (e) {
    setBusy("");
    toast(errorText(e), "error");
  }
}

async function exportSegment(): Promise<void> {
  const doc = currentDoc();
  const v = doc?.video;
  if (!doc || !v) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-frames-${v.first + 1}-${v.last + 1}.seq`), filters: [{ name: "FLIR SEQ video", extensions: ["seq"] }] });
  if (!path) return;
  try {
    const n = await api.exportSegment(doc.info.id, v.first, v.last, path);
    toast(`Saved ${n} frames to ${basename(path)}`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function exportVideo(): Promise<void> {
  const doc = currentDoc();
  if (!doc?.video) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}.avi`), filters: [{ name: "AVI video", extensions: ["avi"] }] });
  if (!path) return;
  const p = progress("Exporting video");
  try {
    const n = await writeVideo(doc, path, (done, total, text) => {
      p.set(done, total, text);
      return !p.cancelled();
    });
    if (n > 0) toast(`Saved ${n} frames to ${basename(path)}`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  } finally {
    p.close();
  }
}

/** Renders the doc's segment (in to out) as an MJPEG AVI. `onFrame` returns false to cancel.
 * Returns the number of frames written (0 if cancelled). */
async function writeVideo(doc: Doc, path: string, onFrame: (done: number, total: number, text: string) => boolean = () => true): Promise<number> {
  const v = doc.video!;
  const wasPlaying = v.playing;
  v.playing = false;
  const startFrame = v.frame;
  // One colour scale for the whole clip, so colours mean the same temperature in every frame.
  const autoRange = doc.range === null;
  if (autoRange) doc.range = [...displayRange(doc)];
  const opts = { ...DEFAULT_EXPORT, ...(loadSettings().exportOverlay ?? {}), scale: 0 };
  let size: [number, number] | null = null;
  let cancelled = false;
  try {
    await api.aviReset();
    const total = v.last - v.first + 1;
    for (let f = v.first; f <= v.last; f++) {
      if (!onFrame(f - v.first, total, `Frame ${f + 1}`)) {
        cancelled = true;
        break;
      }
      await seekDoc(doc, f);
      const canvas = await renderExport(doc, state.unit, state.smooth, { ...opts, fileName: true });
      // Frame sizes must not change within a video.
      if (!size) size = [canvas.width, canvas.height];
      const fixed = size[0] === canvas.width && size[1] === canvas.height ? canvas : resizeCanvas(canvas, size[0], size[1]);
      await api.aviAddFrame(await encode(fixed, "jpg"));
    }
    if (cancelled || !size) {
      await api.aviReset();
      return 0;
    }
    return await api.aviFinish(path, size[0], size[1], doc.info.video!.frameRate ?? 10);
  } finally {
    if (autoRange) doc.range = null;
    await seekDoc(doc, startFrame);
    if (wasPlaying && doc === currentDoc()) togglePlay();
  }
}

function resizeCanvas(c: HTMLCanvasElement, w: number, h: number): HTMLCanvasElement {
  const out = document.createElement("canvas");
  out.width = w;
  out.height = h;
  out.getContext("2d")!.drawImage(c, 0, 0, w, h);
  return out;
}

// ---------------------------------------------------------------------------- readout & status

function showReadout(pos: [number, number] | null): void {
  const el = $("#readout");
  const doc = currentDoc();
  if (!doc || !pos) {
    el.textContent = "";
    return;
  }
  const x = Math.floor(pos[0]);
  const y = Math.floor(pos[1]);
  const i = y * doc.info.width + x;
  if (doc.video?.hsm && doc.video.hsmData) {
    const d = doc.video.hsmData[i]!;
    el.textContent = `Δ ${Number.isFinite(d) ? d.toFixed(3) : "–"} K   x ${x}  y ${y}`;
    return;
  }
  el.textContent = `${temp(doc.kelvin[i], state.unit, 2)}   x ${x}  y ${y}`;
}

function setBusy(text: string): void {
  $("#busy").textContent = text;
  document.body.classList.toggle("busy", text !== "");
}

function updateZoomLabel(): void {
  $("#zoom-fit").textContent = currentDoc() ? `${viewer.zoomPercent()}%` : "100%";
}

function markDirty(doc: Doc): void {
  if (!doc.dirty) {
    doc.dirty = true;
    renderFiles();
    updateToolbar();
  }
}

function renderFiles(): void {
  const list = $("#files");
  $("#count").textContent = state.docs.length ? String(state.docs.length) : "";
  list.replaceChildren(
    ...state.docs.map((doc, i) =>
      h(
        "li",
        { class: i === state.current ? "active" : "", title: doc.info.path, onclick: () => selectDoc(i) },
        h("canvas", { class: "thumb", width: 48, height: 36 }),
        h(
          "div",
          { class: "file-text" },
          h("span", { class: "file-name" }, doc.dirty ? h("span", { class: "dirty", title: "Unsaved edits" }, "●") : null, doc.info.name),
          h(
            "span",
            { class: "file-meta" },
            `${doc.info.video ? `VIDEO · ${doc.info.video.frames} frames` : doc.info.format.toUpperCase()} · ${doc.measurements.length} meas.${doc.findings.length ? ` · ${doc.findings.length} findings` : ""}${doc.asset ? ` · ${doc.asset}` : ""}`,
          ),
        ),
        h(
          "button",
          {
            class: "icon close",
            title: "Close image",
            "aria-label": `Close ${doc.info.name}`,
            onclick: (e: Event) => {
              e.stopPropagation();
              void closeDoc(i);
            },
          },
          "×",
        ),
      ),
    ),
  );
  list.querySelectorAll<HTMLCanvasElement>("canvas.thumb").forEach((c, i) => drawThumb(c, state.docs[i]!));
}

function drawThumb(canvas: HTMLCanvasElement, doc: Doc): void {
  const { width: w, height: hh } = doc.info;
  const tmp = document.createElement("canvas");
  tmp.width = w;
  tmp.height = hh;
  const pal = palette(doc.palette, doc).colors;
  const range = displayRange(doc);
  const img = new ImageData(w, hh);
  const span = range[1] - range[0] || 1;
  const n = pal.length;
  for (let i = 0; i < w * hh; i++) {
    const v = doc.kelvin[i]!;
    const c = Number.isNaN(v) ? [128, 128, 128] : pal[Math.min(n - 1, Math.max(0, Math.floor(((v - range[0]) / span) * n)))]!;
    img.data.set([c[0]!, c[1]!, c[2]!, 255], i * 4);
  }
  tmp.getContext("2d")!.putImageData(img, 0, 0);
  const ctx = canvas.getContext("2d")!;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const s = Math.min(canvas.width / w, canvas.height / hh);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(tmp, (canvas.width - w * s) / 2, (canvas.height - hh * s) / 2, w * s, hh * s);
}

const TOOL_HINTS: Partial<Record<Tool, string>> = {
  polygon: "Click points; double-click, Enter or click the first point to finish. Backspace removes the last point, Esc cancels.",
  polyline: "Click points; double-click or Enter to finish. Backspace removes the last point, Esc cancels.",
  wand: "Click a pixel: the area grows over connected pixels within the tolerance (Measurements section).",
  text: "Click where the note goes, type, then Enter.",
  sketch: "Drag to draw on the image.",
  crop: "Drag the area to keep. Esc cancels.",
};

function updateToolbar(): void {
  const doc = currentDoc();
  for (const id of ["save", "batch"]) ($(`#${id}`) as HTMLButtonElement).disabled = !doc;
  ($("#save") as HTMLButtonElement).classList.toggle("attention", !!doc?.dirty);
  document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => {
    b.classList.toggle("active", b.dataset.tool === state.tool);
    b.disabled = !doc;
  });
  document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => b.classList.toggle("active", b.dataset.unit === state.unit));
  const hint = $("#tool-hint");
  const text = doc ? TOOL_HINTS[state.tool] : undefined;
  hint.hidden = !text;
  hint.textContent = text ?? "";
}

function setTool(tool: Tool): void {
  state.tool = tool;
  viewer.tool = tool;
  updateToolbar();
}

function setUnit(unit: Unit): void {
  state.unit = unit;
  saveSettings({ unit });
  updateToolbar();
  const doc = currentDoc();
  if (doc?.formulas.length) void refreshAnalysis(doc);
  panel.renderAll(doc);
}

// ---------------------------------------------------------------------------- saving & exporting

function stem(doc: Doc): string {
  return doc.info.name.replace(/\.[^.]+$/, "");
}

function siblingPath(doc: Doc, name: string): string {
  const sep = doc.info.path.includes("\\") && !doc.info.path.includes("/") ? "\\" : "/";
  const dir = doc.info.path.slice(0, doc.info.path.lastIndexOf(sep) + 1);
  return dir + name;
}

function sidecarOf(doc: Doc) {
  const changed = doc.info.editable.some((k) => doc.params[k] !== doc.info.fileParams[k]);
  return {
    params: changed ? doc.params : null,
    measurements: doc.measurements,
    notes: doc.notes,
    palette: doc.palette,
    range: doc.range,
    distribution: doc.distribution,
    isotherms: doc.isotherms,
    formulas: doc.formulas,
    annotations: doc.annotations,
    findings: doc.findings,
    asset: doc.asset,
    fields: doc.fields,
    presentation: doc.presentation,
  };
}

async function saveDoc(doc: Doc): Promise<boolean> {
  try {
    await api.saveSidecar(doc.info.id, sidecarOf(doc));
    doc.dirty = false;
    renderFiles();
    updateToolbar();
    return true;
  } catch (e) {
    toast(`Could not save edits: ${errorText(e)}`, "error");
    return false;
  }
}

async function saveEdits(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  if (await saveDoc(doc)) toast(`Saved edits to ${doc.info.name}.kelvinlens.json`);
}

async function exportImage(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const opts = await exportOptions();
  if (!opts) return;
  const ext = opts.format === "png" ? "png" : "jpg";
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-thermal.${ext}`), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  try {
    const canvas = await renderExport(doc, state.unit, state.smooth, opts);
    await api.saveBytes(path, await encode(canvas, opts.format));
    toast(`Exported ${basename(path)} (${canvas.width} × ${canvas.height})`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function exportCsv(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-temperatures-${state.unit}.csv`), filters: [{ name: "CSV", extensions: ["csv"] }] });
  if (!path) return;
  try {
    await api.exportCsv(doc.info.id, path, state.unit);
    toast(`Exported ${basename(path)} (${symbol(state.unit)}, one row per image row)`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function exportTiff(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-radiometric.tiff`), filters: [{ name: "Radiometric TIFF (kelvin)", extensions: ["tiff", "tif"] }] });
  if (!path) return;
  try {
    await api.exportThermalTiff(doc.info.id, path);
    toast(`Saved ${basename(path)}: 32-bit temperatures in kelvin, opens in Kelvinlens and Blackbody`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function exportFlirCopy(): Promise<void> {
  const doc = currentDoc();
  if (!doc?.info.flirJpeg) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-kelvinlens.jpg`), filters: [{ name: "FLIR radiometric JPEG", extensions: ["jpg"] }] });
  if (!path) return;
  try {
    await api.saveFlirCopy(doc.info.id, path);
    toast(`Saved ${basename(path)} with the current parameters written into it`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

interface SettingsFile {
  kelvinlensSettings: 1;
  params: Params | null;
  palette: string;
  range: Range | null;
  distribution: Distribution;
  isotherms: Doc["isotherms"];
  measurements: Doc["measurements"];
  formulas: Doc["formulas"];
  presentation: Doc["presentation"];
}

async function exportSettings(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const s = sidecarOf(doc);
  const data: SettingsFile = {
    kelvinlensSettings: 1,
    params: s.params,
    palette: doc.palette,
    range: doc.range,
    distribution: doc.distribution,
    isotherms: doc.isotherms,
    measurements: doc.measurements,
    formulas: doc.formulas,
    presentation: doc.presentation,
  };
  const path = await pickers.save({ defaultPath: siblingPath(doc, `${stem(doc)}-settings.json`), filters: [{ name: "Kelvinlens settings", extensions: ["json"] }] });
  if (!path) return;
  try {
    await api.writeText(path, JSON.stringify(data, null, 2) + "\n");
    toast(`Saved ${basename(path)}`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

async function importSettings(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.open({ filters: [{ name: "Kelvinlens settings", extensions: ["json"] }] });
  if (typeof path !== "string") return;
  let data: Partial<SettingsFile>;
  try {
    data = JSON.parse(await api.readText(path)) as Partial<SettingsFile>;
    if (data.kelvinlensSettings !== 1) throw new Error("Not a Kelvinlens settings file");
  } catch (e) {
    toast(`Could not import: ${errorText(e)}`, "error");
    return;
  }
  const scope = await modal("Import settings", [h("p", {}, `Apply ${basename(path)} to…`)], [
    { label: "Cancel", value: "cancel" },
    { label: "All open images", value: "all" },
    { label: "This image", value: "one", primary: true },
  ]);
  if (scope !== "one" && scope !== "all") return;
  const targets = scope === "all" ? state.docs : [doc];
  for (const d of targets) {
    if (data.palette) d.palette = data.palette === "camera" && !d.info.embeddedPalette ? state.defaultPalette : data.palette;
    if (data.range !== undefined) d.range = data.range;
    if (data.distribution) d.distribution = data.distribution;
    if (data.isotherms) d.isotherms = structuredClone(data.isotherms);
    if (data.measurements) d.measurements = structuredClone(data.measurements);
    if (data.formulas) d.formulas = structuredClone(data.formulas);
    if (data.presentation) d.presentation = structuredClone(data.presentation);
    if (data.params && d.info.editable.length) await applyParams(d, { ...d.params, ...Object.fromEntries(Object.entries(data.params).filter(([k, v]) => v != null && d.info.editable.includes(k as keyof Params))) });
    markDirty(d);
    await refreshAnalysis(d, false);
  }
  selectDoc(state.current);
  toast(`Applied settings to ${targets.length} image${targets.length > 1 ? "s" : ""}`);
}

function reportItems(docs: Doc[]) {
  return docs.map((d) => ({
    id: d.info.id,
    look: lookOf(d),
    measurements: d.measurements,
    notes: d.notes,
    formulas: d.formulas,
    annotations: d.annotations,
    findings: d.findings,
    fields: d.fields,
    asset: d.asset,
    presentation: d.presentation,
  }));
}

async function createReport(): Promise<void> {
  if (!currentDoc()) return;
  const choice = await reportDialog(templateEditor);
  if (!choice) return;
  if (choice.docs.length === 0) {
    toast("Select at least one image for the report.", "error");
    return;
  }
  const first = choice.docs[0]!;
  const ext = choice.info.format === "html" ? "html" : "pdf";
  const name = (choice.info.title || "report").replace(/[\\/:*?"<>|]+/g, "-");
  const path = await pickers.save({ defaultPath: siblingPath(first, `${name}.${ext}`), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  await writeReport(choice.info, choice.docs, path);
}

async function writeReport(info: Parameters<typeof api.makeReport>[0], docs: Doc[], path: string): Promise<void> {
  setBusy("Creating report…");
  try {
    await api.makeReport(info, reportItems(docs), path);
    toast(`Report saved: ${basename(path)}`);
  } catch (e) {
    toast(`Report failed: ${errorText(e)}`, "error");
  } finally {
    setBusy("");
  }
}

/** One click: the last report settings and template, every open image, saved next to the first. */
async function rapidReport(): Promise<void> {
  const docs = state.docs.filter((d) => d.includeInReport);
  if (docs.length === 0) return;
  const info = rapidInfo();
  const ext = info.format === "html" ? "html" : "pdf";
  const first = docs[0]!;
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
  const path = siblingPath(first, `${stem(first)}-report-${stamp}.${ext}`);
  await writeReport(info, docs, path);
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

async function pickFiles(): Promise<void> {
  const picked = await pickers.open({
    multiple: true,
    filters: [
      { name: "Thermal images and videos", extensions: ["jpg", "jpeg", "JPG", "is2", "IS2", "irg", "IRG", "tif", "tiff", "png", "seq", "SEQ", "csq", "CSQ"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  if (picked) await openPaths(Array.isArray(picked) ? picked : [picked]);
}

async function pickFolder(): Promise<void> {
  const picked = await pickers.open({ directory: true });
  if (typeof picked === "string") await openPaths([picked]);
}

async function triage(): Promise<void> {
  const paths = await triageDialog();
  if (paths.length) await openPaths(paths);
}

async function panorama(): Promise<void> {
  const req = await panoramaDialog();
  if (!req) return;
  setBusy("Stitching…");
  try {
    const r = await api.stitch(req.ids, req.offsets);
    const doc = await loadDoc(r.info);
    doc.dirty = true;
    state.docs.push(doc);
    selectDoc(state.docs.length - 1);
    const weak = r.scores.slice(1).some((s) => Number.isFinite(s) && s < 0.8);
    toast(
      `Panorama ${r.info.width} × ${r.info.height} from ${req.ids.length} images (offsets ${r.offsets.map((o) => `${o[0]},${o[1]}`).join(" · ")}). ${weak ? "Some images matched weakly; check the seams or place them by hand. " : ""}Save it with Export ▸ Radiometric TIFF.`,
      weak ? "error" : "info",
    );
  } catch (e) {
    toast(`Panorama failed: ${errorText(e)}`, "error");
  } finally {
    setBusy("");
  }
}

// ---------------------------------------------------------------------------- events

$("#open").onclick = () => void pickFiles();
$("#empty-open").onclick = () => void pickFiles();
$("#open-folder").onclick = () => void pickFolder();
$("#empty-folder").onclick = () => void pickFolder();
$("#save").onclick = () => void saveEdits();
$("#batch").onclick = () =>
  void batchDialog({
    applyParams: async (doc, params) => {
      const r = await api.setParams(doc.info.id, params);
      await reloadTemperatures(doc, r.params);
    },
    refreshAnalysis: (doc) => refreshAnalysis(doc, false),
    saveEdits: (doc) => saveDoc(doc),
    writeVideo: (doc, path) => writeVideo(doc, path),
    markDirty,
    redraw: () => selectDoc(state.current),
  });
$("#zoom-in").onclick = () => viewer.zoomBy(1.25);
$("#zoom-out").onclick = () => viewer.zoomBy(0.8);
$("#zoom-fit").onclick = () => viewer.fit();
document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => (b.onclick = () => setTool(b.dataset.tool as Tool)));
document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => (b.onclick = () => setUnit(b.dataset.unit as Unit)));

window.addEventListener("keydown", (e) => {
  const target = e.target as HTMLElement;
  const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
  const mod = e.metaKey || e.ctrlKey;
  if (document.querySelector("dialog[open]")) return;
  if (mod && e.key.toLowerCase() === "o") {
    e.preventDefault();
    void (e.shiftKey ? pickFolder() : pickFiles());
  } else if (mod && e.key.toLowerCase() === "s") {
    e.preventDefault();
    void saveEdits();
  } else if (mod && e.key.toLowerCase() === "p") {
    e.preventDefault();
    void createReport();
  } else if (typing || mod || viewer.isDrawing()) {
    return;
  } else if ((e.key === "Delete" || e.key === "Backspace") && state.selected !== null) {
    e.preventDefault();
    deleteMeasurement(state.selected);
  } else if (e.key === "Escape") {
    state.selected = null;
    viewer.selected = null;
    setTool("select");
    viewer.draw();
    panel.renderMeasurements(currentDoc());
  } else if (currentDoc()?.video && (e.key === "," || e.key === ".")) {
    const v = currentDoc()!.video!;
    void seek(v.frame + (e.key === "," ? -1 : 1));
  } else if (currentDoc()?.video && e.key.toLowerCase() === "k") {
    togglePlay();
  } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
    if (state.current < state.docs.length - 1) selectDoc(state.current + 1);
  } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
    if (state.current > 0) selectDoc(state.current - 1);
  } else if (e.key === "f" || e.key === "F") {
    viewer.fit();
  } else if (e.key === "+" || e.key === "=") {
    viewer.zoomBy(1.25);
  } else if (e.key === "-") {
    viewer.zoomBy(0.8);
  } else {
    const tool = TOOLS.find(([, , key]) => key.toLowerCase() === e.key.toLowerCase());
    if (tool && currentDoc()) setTool(tool[0]);
  }
});

window.addEventListener("resize", debounce(() => updateZoomLabel(), 100));

void getCurrentWebview().onDragDropEvent((event) => {
  const overlay = $("#drop");
  if (event.payload.type === "enter" || event.payload.type === "over") overlay.classList.add("visible");
  else overlay.classList.remove("visible");
  if (event.payload.type === "drop") void openPaths(event.payload.paths);
});

void getCurrentWindow().onCloseRequested(async (event) => {
  const dirty = state.docs.filter((d) => d.dirty);
  if (dirty.length === 0) return;
  const ok = await pickers.ask(`${dirty.length} image${dirty.length > 1 ? "s have" : " has"} unsaved edits. Quit anyway?`, {
    title: "Unsaved edits",
    kind: "warning",
    okLabel: "Quit",
    cancelLabel: "Cancel",
  });
  if (!ok) event.preventDefault();
});

// ---------------------------------------------------------------------------- start

async function start(): Promise<void> {
  const settings = loadSettings();
  if (settings.unit && UNITS.includes(settings.unit)) state.unit = settings.unit;
  if (typeof settings.smooth === "boolean") state.smooth = settings.smooth;
  if (typeof settings.showExtremes === "boolean") state.showExtremes = settings.showExtremes;
  if (typeof settings.msxStrength === "number") state.msxStrength = settings.msxStrength;
  if (Array.isArray(settings.customPalettes)) state.customPalettes = settings.customPalettes;
  if (Array.isArray(settings.customTemplates)) state.customTemplates = settings.customTemplates;
  if (settings.inspect) state.inspect = { ...DEFAULT_INSPECT, ...settings.inspect };
  [state.palettes, state.builtinTemplates] = await Promise.all([api.palettes(), api.templates()]);
  if (settings.defaultPalette && palette(settings.defaultPalette, null).id === settings.defaultPalette) state.defaultPalette = settings.defaultPalette;
  viewer.smooth = state.smooth;
  viewer.showExtremes = state.showExtremes;
  document.documentElement.style.setProperty("--brand-gradient", gradientCss(palette("iron", null).colors));
  selectDoc(-1);
  const initial = await api.initialPaths();
  if (initial.length) await openPaths(initial);
}

void start().catch((e) => toast(`Startup failed: ${errorText(e)}`, "error"));

if (__DEV_HOOKS__) {
  Object.assign(window, {
    kelvinlensDev: {
      state,
      api,
      viewer,
      panel,
      exportPng,
      renderExport,
      palette,
      displayRange,
      selectDoc,
      openPaths,
      refreshAnalysis,
      runInspect,
      seek,
      setTool,
      setUnit,
      saveEdits,
      writeReport,
      reportItems,
      rapidInfo,
      sidecarOf,
      lookOf,
      pickers,
      handlers: (panel as unknown as { on: unknown }).on,
    },
  });
}

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { api, type Params, type Range, type Unit } from "./api";
import { $, debounce, errorText, h, perFrame, toast } from "./dom";
import { exportPng } from "./exportImage";
import { Panel, gradientCss } from "./panel";
import { reportDialog } from "./reportDialog";
import { currentDoc, displayRange, finiteRange, loadSettings, nextLabel, palette, saveSettings, state, type Doc, type Tool, type View } from "./state";
import { UNITS, symbol, temp } from "./units";
import { Viewer } from "./viewer";

// ---------------------------------------------------------------------------- layout

const TOOLS: [Tool, string, string, string][] = [
  ["select", "Select", "V", "M4 3l14 8-6 1.6L9.5 19z"],
  ["spot", "Spot", "S", "M12 5v4M12 15v4M5 12h4M15 12h4M12 12m-2.2 0a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0"],
  ["box", "Box", "B", "M5 6h14v12H5z"],
  ["ellipse", "Ellipse", "E", "M12 6c4.4 0 8 2.7 8 6s-3.6 6-8 6-8-2.7-8-6 3.6-6 8-6z"],
  ["line", "Line", "L", "M5 18L19 6"],
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

const app = $("#app");
app.append(
  h(
    "header",
    { class: "toolbar" },
    h("div", { class: "brand" }, h("img", { src: "/icon.png", alt: "" }), h("span", {}, "Kelvinlens")),
    h(
      "div",
      { class: "group" },
      h("button", { id: "open", title: "Open images (Ctrl/⌘ O)" }, "Open"),
      h("button", { id: "open-folder", title: "Open every image in a folder (Ctrl/⌘ Shift O)" }, "Folder"),
    ),
    h(
      "div",
      { class: "group tools", role: "toolbar", "aria-label": "Measurement tools" },
      TOOLS.map(([tool, label, key, path]) =>
        h("button", { class: "tool", "data-tool": tool, title: `${label} (${key})`, "aria-label": label }, icon(path), h("span", {}, label)),
      ),
    ),
    h("div", { class: "spacer" }),
    h(
      "div",
      { class: "group" },
      h("button", { id: "save", title: "Save edits next to the image (Ctrl/⌘ S)" }, "Save"),
      h("button", { id: "export-png", title: "Export the image with measurements as PNG" }, "PNG"),
      h("button", { id: "export-csv", title: "Export all temperatures as CSV" }, "CSV"),
      h("button", { id: "report", class: "primary", title: "Create a PDF report (Ctrl/⌘ P)" }, "Report"),
    ),
    h(
      "div",
      { class: "segmented units", role: "group", "aria-label": "Temperature unit" },
      UNITS.map((u) => h("button", { "data-unit": u }, symbol(u))),
    ),
  ),
  h(
    "aside",
    { class: "sidebar" },
    h("div", { class: "sidebar-head" }, h("h2", {}, "Images"), h("span", { id: "count", class: "muted" })),
    h("ul", { id: "files", class: "files" }),
  ),
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
      h(
        "div",
        { class: "row buttons" },
        h("button", { class: "primary", id: "empty-open" }, "Open images…"),
        h("button", { id: "empty-folder" }, "Open folder…"),
      ),
      h("p", { class: "muted small" }, "FLIR radiometric JPEG · Fluke IS2 · InfiRay/Topdon IRG · HTI · 16-bit PNG and TIFF"),
    ),
    h("div", { id: "drop", class: "drop-overlay" }, h("div", {}, "Drop to open")),
    h(
      "div",
      { class: "zoombar" },
      h("button", { id: "zoom-out", title: "Zoom out (−)", "aria-label": "Zoom out" }, "−"),
      h("button", { id: "zoom-fit", title: "Fit to window (F)" }, "100%"),
      h("button", { id: "zoom-in", title: "Zoom in (+)", "aria-label": "Zoom in" }, "+"),
    ),
  ),
  h("aside", { id: "panel", class: "panel" }),
  h(
    "footer",
    { class: "statusbar" },
    h("span", { id: "readout", class: "readout" }),
    h("span", { id: "status-file", class: "muted" }),
    h("span", { class: "spacer" }),
    h("span", { id: "busy", class: "muted" }),
  ),
);

// ---------------------------------------------------------------------------- viewer & panel

const viewer = new Viewer($("#viewer"), {
  hover: (pos) => showReadout(pos),
  measurementsChanged: (final) => {
    const doc = currentDoc();
    if (!doc) return;
    markDirty(doc);
    if (final) void measureNow(doc);
    else measureSoon(doc);
  },
  select: (index) => {
    state.selected = index;
    panel.renderMeasurements(currentDoc());
  },
  zoomChanged: () => updateZoomLabel(),
  newLabel: (kind) => nextLabel(currentDoc()!, kind),
});

const panel = new Panel($("#panel"), {
  setPalette: (id) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.palette = id;
    state.defaultPalette = id;
    saveSettings({ defaultPalette: id });
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
  },
  setRange: (range) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.range = range;
    markDirty(doc);
    refreshView();
    panel.renderDisplay(doc);
  },
  setView: (view: View) => {
    state.view = view;
    viewer.view = view;
    viewer.fit();
    panel.renderDisplay(currentDoc());
  },
  setBlend: (value) => {
    state.blend = value;
    viewer.blend = value;
    viewer.draw();
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
    panel.renderMeasurements(doc);
  },
  deleteMeasurement: (index) => deleteMeasurement(index),
  setNotes: (notes) => {
    const doc = currentDoc();
    if (!doc) return;
    doc.notes = notes;
    markDirty(doc);
  },
});

function refreshView(): void {
  const doc = currentDoc();
  viewer.setDoc(doc, false);
  if (!doc) return;
  viewer.palette = palette(doc.palette);
  viewer.range = displayRange(doc);
  viewer.selected = state.selected;
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
      const doc = await loadDoc(path);
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

async function loadDoc(path: string): Promise<Doc> {
  const info = await api.openImage(path);
  const [kelvin, photo] = await Promise.all([api.thermalData(info.id), info.photoSize ? api.photo(info.id).catch(() => null) : null]);
  const doc: Doc = {
    info,
    kelvin,
    dataRange: finiteRange(kelvin),
    photo,
    measurements: info.embeddedMeasurements.map((m) => ({ label: m.label || "", shape: m.shape })),
    stats: [],
    notes: "",
    palette: state.defaultPalette,
    range: null,
    params: info.params,
    dirty: false,
    includeInReport: true,
  };
  doc.measurements.forEach((m) => (m.label ||= nextLabel(doc, m.shape.kind)));

  const edits = info.sidecar;
  if (info.sidecarError) toast(info.sidecarError, "error");
  if (edits) {
    doc.measurements = edits.measurements;
    doc.notes = edits.notes;
    if (edits.palette && state.palettes.some((p) => p.id === edits.palette)) doc.palette = edits.palette;
    doc.range = edits.range;
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
  doc.stats = await api.measure(info.id, doc.measurements.map((m) => m.shape));
  return doc;
}

function selectDoc(index: number): void {
  if (index < 0 || index >= state.docs.length) {
    state.current = -1;
  } else {
    state.current = index;
  }
  state.selected = null;
  showReadout(null);
  const doc = currentDoc();
  $("#empty").hidden = !!doc;
  if (doc && !doc.photo && state.view !== "thermal") {
    state.view = "thermal";
    viewer.view = "thermal";
  }
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
  if (doc.dirty && !(await ask(`Close ${doc.info.name} without saving your edits?`, { title: "Unsaved edits", kind: "warning" }))) return;
  state.docs.splice(index, 1);
  void api.closeImage(doc.info.id);
  const next = state.current > index ? state.current - 1 : Math.min(state.current, state.docs.length - 1);
  selectDoc(next);
}

// ---------------------------------------------------------------------------- parameters

async function reloadTemperatures(doc: Doc, params: Params): Promise<void> {
  doc.params = params;
  doc.kelvin = await api.thermalData(doc.info.id);
  doc.dataRange = finiteRange(doc.kelvin);
  doc.stats = await api.measure(doc.info.id, doc.measurements.map((m) => m.shape));
  if (doc === currentDoc()) {
    viewer.dataChanged();
    refreshView();
    panel.renderDisplay(doc);
    panel.renderParams(doc);
    panel.renderMeasurements(doc);
  }
}

async function applyParams(doc: Doc, params: Params): Promise<void> {
  try {
    const r = await api.setParams(doc.info.id, params);
    markDirty(doc);
    await reloadTemperatures(doc, r.params);
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

async function measureNow(doc: Doc): Promise<void> {
  try {
    doc.stats = await api.measure(doc.info.id, doc.measurements.map((m) => m.shape));
  } catch (e) {
    toast(errorText(e), "error");
  }
  if (doc === currentDoc()) panel.renderMeasurements(doc);
  renderFiles();
}

const measureSoon = perFrame((doc: Doc) => void measureNow(doc));

function deleteMeasurement(index: number): void {
  const doc = currentDoc();
  if (!doc || !doc.measurements[index]) return;
  doc.measurements.splice(index, 1);
  doc.stats.splice(index, 1);
  state.selected = null;
  viewer.selected = null;
  markDirty(doc);
  viewer.draw();
  panel.renderMeasurements(doc);
  renderFiles();
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
  const k = doc.kelvin[y * doc.info.width + x];
  el.textContent = `${temp(k, state.unit, 2)}   x ${x}  y ${y}`;
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
          h("span", { class: "file-meta" }, `${doc.info.format.toUpperCase()} · ${doc.measurements.length} meas.`),
        ),
        h("button", {
          class: "icon close",
          title: "Close image",
          "aria-label": `Close ${doc.info.name}`,
          onclick: (e: Event) => {
            e.stopPropagation();
            void closeDoc(i);
          },
        }, "×"),
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
  const pal = palette(doc.palette);
  const range = displayRange(doc);
  const img = new ImageData(w, hh);
  const span = range[1] - range[0] || 1;
  for (let i = 0; i < w * hh; i++) {
    const v = doc.kelvin[i]!;
    const c = Number.isNaN(v) ? [128, 128, 128] : pal.colors[Math.min(255, Math.max(0, Math.floor(((v - range[0]) / span) * 256)))]!;
    img.data.set([c[0]!, c[1]!, c[2]!, 255], i * 4);
  }
  tmp.getContext("2d")!.putImageData(img, 0, 0);
  const ctx = canvas.getContext("2d")!;
  const s = Math.min(canvas.width / w, canvas.height / hh);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(tmp, (canvas.width - w * s) / 2, (canvas.height - hh * s) / 2, w * s, hh * s);
}

function updateToolbar(): void {
  const doc = currentDoc();
  for (const id of ["save", "export-png", "export-csv", "report"]) {
    ($(`#${id}`) as HTMLButtonElement).disabled = !doc;
  }
  ($("#save") as HTMLButtonElement).classList.toggle("attention", !!doc?.dirty);
  document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => {
    b.classList.toggle("active", b.dataset.tool === state.tool);
    b.disabled = !doc;
  });
  document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => b.classList.toggle("active", b.dataset.unit === state.unit));
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
  panel.renderAll(currentDoc());
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

async function saveEdits(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const changed = doc.info.editable.some((k) => doc.params[k] !== doc.info.fileParams[k]);
  try {
    const path = await api.saveSidecar(doc.info.id, {
      params: changed ? doc.params : null,
      measurements: doc.measurements,
      notes: doc.notes,
      palette: doc.palette,
      range: doc.range,
    });
    doc.dirty = false;
    renderFiles();
    updateToolbar();
    toast(`Saved edits to ${basename(path)}`);
  } catch (e) {
    toast(`Could not save edits: ${errorText(e)}`, "error");
  }
}

async function exportImage(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await save({ defaultPath: siblingPath(doc, `${stem(doc)}-thermal.png`), filters: [{ name: "PNG image", extensions: ["png"] }] });
  if (!path) return;
  try {
    const bytes = await exportPng(doc, palette(doc.palette), displayRange(doc), state.unit, state.smooth);
    await api.saveBytes(path, bytes);
    toast(`Exported ${basename(path)}`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function exportCsv(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await save({
    defaultPath: siblingPath(doc, `${stem(doc)}-temperatures-${state.unit}.csv`),
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  if (!path) return;
  try {
    await api.exportCsv(doc.info.id, path, state.unit);
    toast(`Exported ${basename(path)} (${symbol(state.unit)}, one row per image row)`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

async function createReport(): Promise<void> {
  if (!currentDoc()) return;
  const choice = await reportDialog();
  if (!choice) return;
  if (choice.docs.length === 0) {
    toast("Select at least one image for the report.", "error");
    return;
  }
  const first = choice.docs[0]!;
  const name = (choice.info.title || "report").replace(/[\\/:*?"<>|]+/g, "-");
  const path = await save({ defaultPath: siblingPath(first, `${name}.pdf`), filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!path) return;
  setBusy("Creating report…");
  try {
    await api.makeReport(
      choice.info,
      choice.docs.map((d) => ({ id: d.info.id, palette: d.palette, range: displayRange(d) as Range, measurements: d.measurements, notes: d.notes })),
      path,
    );
    toast(`Report saved: ${basename(path)}`);
  } catch (e) {
    toast(`Report failed: ${errorText(e)}`, "error");
  } finally {
    setBusy("");
  }
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

async function pickFiles(): Promise<void> {
  const picked = await open({
    multiple: true,
    filters: [
      { name: "Thermal images", extensions: ["jpg", "jpeg", "JPG", "is2", "IS2", "irg", "IRG", "tif", "tiff", "png"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  if (picked) await openPaths(Array.isArray(picked) ? picked : [picked]);
}

async function pickFolder(): Promise<void> {
  const picked = await open({ directory: true });
  if (typeof picked === "string") await openPaths([picked]);
}

// ---------------------------------------------------------------------------- events

$("#open").onclick = () => void pickFiles();
$("#empty-open").onclick = () => void pickFiles();
$("#open-folder").onclick = () => void pickFolder();
$("#empty-folder").onclick = () => void pickFolder();
$("#save").onclick = () => void saveEdits();
$("#export-png").onclick = () => void exportImage();
$("#export-csv").onclick = () => void exportCsv();
$("#report").onclick = () => void createReport();
$("#zoom-in").onclick = () => viewer.zoomBy(1.25);
$("#zoom-out").onclick = () => viewer.zoomBy(0.8);
$("#zoom-fit").onclick = () => viewer.fit();
document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => (b.onclick = () => setTool(b.dataset.tool as Tool)));
document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => (b.onclick = () => setUnit(b.dataset.unit as Unit)));

window.addEventListener("keydown", (e) => {
  const target = e.target as HTMLElement;
  const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key.toLowerCase() === "o") {
    e.preventDefault();
    void (e.shiftKey ? pickFolder() : pickFiles());
  } else if (mod && e.key.toLowerCase() === "s") {
    e.preventDefault();
    void saveEdits();
  } else if (mod && e.key.toLowerCase() === "p") {
    e.preventDefault();
    void createReport();
  } else if (typing || mod) {
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
  const ok = await ask(
    `${dirty.length} image${dirty.length > 1 ? "s have" : " has"} unsaved edits. Quit anyway?`,
    { title: "Unsaved edits", kind: "warning", okLabel: "Quit", cancelLabel: "Cancel" },
  );
  if (!ok) event.preventDefault();
});

// ---------------------------------------------------------------------------- start

async function start(): Promise<void> {
  const settings = loadSettings();
  if (settings.unit && UNITS.includes(settings.unit)) state.unit = settings.unit;
  if (typeof settings.smooth === "boolean") state.smooth = settings.smooth;
  if (typeof settings.showExtremes === "boolean") state.showExtremes = settings.showExtremes;
  state.palettes = await api.palettes();
  if (settings.defaultPalette && state.palettes.some((p) => p.id === settings.defaultPalette)) state.defaultPalette = settings.defaultPalette;
  viewer.smooth = state.smooth;
  viewer.showExtremes = state.showExtremes;
  document.documentElement.style.setProperty("--brand-gradient", gradientCss(palette("iron").colors));
  selectDoc(-1);
  const initial = await api.initialPaths();
  if (initial.length) await openPaths(initial);
}

void start().catch((e) => toast(`Startup failed: ${errorText(e)}`, "error"));


if (__DEV_HOOKS__) {
  Object.assign(window, { kelvinlensDev: { state, api, viewer, exportPng, palette, displayRange, selectDoc, openPaths } });
}

// The right-hand panel: display, measurements, formulas, isotherms, automatic inspection, video,
// capture parameters, inspection details, notes and image details.
import { api, type Alarm, type Distribution, type Formula, type Isotherm, type IsothermKind, type Param, type Params, type Range, type ResultKind, type Stats, type Unit } from "./api";
import { SERIES_COLORS, lineChart } from "./charts";
import { fill, h } from "./dom";
import { severityColor } from "./overlays";
import { CAMERA_PALETTE, allTemplates, displayRange, isArea, isPath, loadSettings, palette, palettesFor, state, type Doc, type View } from "./state";
import { delta, deltaFromKelvin, fromKelvin, inputValue, symbol, temp, toKelvin } from "./units";

export interface PanelHandlers {
  setPalette(id: string): void;
  editPalettes(): void;
  setRange(range: Range | null): void;
  setDistribution(d: Distribution): void;
  setView(view: View): void;
  setBlend(value: number): void;
  setMsxStrength(value: number): void;
  setFusionRange(range: Range): void;
  setSmooth(on: boolean): void;
  setShowExtremes(on: boolean): void;
  setRotation(degrees: number): void;
  setAlign(align: [number, number, number] | null): void;
  startCrop(): void;
  clearCrop(): void;
  setParams(params: Params): void;
  resetParams(): void;
  applyParamsToAll(): void;
  select(index: number | null): void;
  renameMeasurement(index: number, label: string): void;
  deleteMeasurement(index: number): void;
  setAlarm(index: number, alarm: Alarm | null): void;
  setWandTolerance(index: number, tolerance: number): void;
  setFormulas(formulas: Formula[]): void;
  setIsotherms(isotherms: Isotherm[]): void;
  runInspect(): void;
  clearFindings(): void;
  findingsToMeasurements(): void;
  setAsset(asset: string): void;
  setField(name: string, value: string): void;
  setAnnotationText(index: number, text: string): void;
  deleteAnnotation(index: number): void;
  setNotes(notes: string): void;
  seek(frame: number): void;
  togglePlay(): void;
  setHsm(patch: { hsm?: boolean; hsmReference?: number; hsmSpan?: number; binning?: boolean }): void;
  setInOut(first: number, last: number): void;
  plotSeries(): void;
  exportVideo(): void;
  exportSegment(): void;
}

// Typical values from Testo's "Pocket Guide Thermography" emissivity table. Real surfaces vary.
const EMISSIVITY_PRESETS: [string, number][] = [
  ["Human skin", 0.98],
  ["Paint, black matt", 0.97],
  ["Paper", 0.97],
  ["Ice, smooth", 0.97],
  ["Paint, white", 0.95],
  ["Glass", 0.94],
  ["Wood", 0.94],
  ["Plastics (PE, PP, PVC)", 0.94],
  ["Rubber, hard", 0.94],
  ["Transformer paint", 0.94],
  ["Brick, mortar, plaster", 0.93],
  ["Concrete", 0.93],
  ["Gypsum", 0.9],
  ["Steel, oxidized", 0.79],
  ["Copper, oxidized", 0.76],
  ["Cast iron, oxidized", 0.64],
  ["Aluminium, heavily oxidized", 0.2],
];

interface ParamField {
  key: Param;
  label: string;
  kind: "temperature" | "ratio" | "percent" | "metres";
  min: number;
  max: number;
  step: number;
}

const PARAM_FIELDS: ParamField[] = [
  { key: "emissivity", label: "Emissivity", kind: "ratio", min: 0.01, max: 1, step: 0.01 },
  { key: "reflectedK", label: "Reflected temp.", kind: "temperature", min: -100, max: 2000, step: 0.1 },
  { key: "distanceM", label: "Distance", kind: "metres", min: 0, max: 10000, step: 0.1 },
  { key: "humidity", label: "Rel. humidity", kind: "percent", min: 0, max: 100, step: 1 },
  { key: "atmosphereK", label: "Air temp.", kind: "temperature", min: -100, max: 200, step: 0.1 },
  { key: "windowK", label: "IR window temp.", kind: "temperature", min: -100, max: 500, step: 0.1 },
  { key: "windowTransmission", label: "IR window trans.", kind: "ratio", min: 0.01, max: 1, step: 0.01 },
];

const DISTRIBUTIONS: [Distribution, string][] = [
  ["temperatureLinear", "Temperature linear"],
  ["signalLinear", "Signal linear"],
  ["histogram", "Histogram equalised"],
  ["detail", "Detail enhanced"],
];

function fieldValue(f: ParamField, v: number | null, unit: Unit): string {
  if (v == null) return "";
  switch (f.kind) {
    case "temperature":
      return inputValue(v, unit);
    case "percent":
      return String(Math.round(v * 100));
    case "ratio":
      return v.toFixed(2);
    case "metres":
      return String(Math.round(v * 100) / 100);
  }
}

function parseField(f: ParamField, text: string, unit: Unit): number | null {
  const n = Number(text.replace(",", "."));
  if (text.trim() === "" || !Number.isFinite(n)) return null;
  switch (f.kind) {
    case "temperature":
      return toKelvin(n, unit);
    case "percent":
      return n / 100;
    default:
      return n;
  }
}

function unitLabel(f: ParamField, unit: Unit): string {
  return f.kind === "temperature" ? symbol(unit) : f.kind === "percent" ? "%" : f.kind === "metres" ? "m" : "";
}

function num(text: string): number | null {
  const n = Number(text.replace(",", "."));
  return text.trim() === "" || !Number.isFinite(n) ? null : n;
}

/** A number input for a temperature (kelvin in, kelvin out) in the current unit. */
function tempInput(k: number | null, onSet: (k: number | null) => void, label: string): HTMLInputElement {
  return h("input", {
    type: "number",
    step: "0.1",
    class: "num",
    "aria-label": label,
    value: inputValue(k, state.unit),
    onchange: (e: Event) => {
      const v = num((e.target as HTMLInputElement).value);
      onSet(v == null ? null : toKelvin(v, state.unit));
    },
  });
}

/** A number input for a temperature difference in the current unit (kelvin in and out). */
function deltaInput(dk: number, onSet: (dk: number) => void, label: string): HTMLInputElement {
  return h("input", {
    type: "number",
    step: "0.1",
    min: "0",
    class: "num",
    "aria-label": label,
    value: String(Math.round(deltaFromKelvin(dk, state.unit) * 10) / 10),
    onchange: (e: Event) => {
      const v = num((e.target as HTMLInputElement).value);
      if (v != null && v >= 0) onSet(state.unit === "F" ? v / 1.8 : v);
    },
  });
}

function section(id: string, title: string, body: Node, extra?: Node): HTMLElement {
  return h("section", { class: "panel-section", id: `section-${id}` }, h("header", {}, h("h2", {}, title), extra ?? null), body);
}

function rgbHex(c: [number, number, number]): string {
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function hexRgb(s: string): [number, number, number] {
  const n = parseInt(s.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function isothermText(i: IsothermKind, unit: Unit): string {
  switch (i.kind) {
    case "above":
      return `Above ${temp(i.t, unit)}`;
    case "below":
      return `Below ${temp(i.t, unit)}`;
    case "interval":
      return `${temp(Math.min(i.lo, i.hi), unit)} to ${temp(Math.max(i.lo, i.hi), unit)}`;
    case "condensation":
      return `Condensation risk (surface RH ≥ ${Math.round(i.limit * 100)} %)`;
    case "insulation":
      return `Insulation (thermal index < ${Math.round(i.index * 100)} %)`;
  }
}

export class Panel {
  private els = {
    display: h("div"),
    measurements: h("div"),
    formulas: h("div"),
    isotherms: h("div"),
    inspect: h("div"),
    video: h("div"),
    params: h("div"),
    inspection: h("div"),
    notes: h("div"),
    details: h("div"),
  };
  private videoSection: HTMLElement;
  private profileFor = "";

  constructor(
    host: HTMLElement,
    private on: PanelHandlers,
  ) {
    this.videoSection = section("video", "Video", this.els.video);
    host.append(
      section("display", "Display", this.els.display),
      this.videoSection,
      section("measurements", "Measurements", this.els.measurements),
      section("formulas", "Formulas", this.els.formulas),
      section("isotherms", "Isotherms & alarms", this.els.isotherms),
      section("inspect", "Auto-inspect", this.els.inspect),
      section("params", "Parameters", this.els.params),
      section("inspection", "Inspection", this.els.inspection),
      section("notes", "Notes", this.els.notes),
      section("details", "Image details", this.els.details),
    );
  }

  renderAll(doc: Doc | null): void {
    this.renderDisplay(doc);
    this.renderVideo(doc);
    this.renderMeasurements(doc);
    this.renderFormulas(doc);
    this.renderIsotherms(doc);
    this.renderInspect(doc);
    this.renderParams(doc);
    this.renderInspection(doc);
    this.renderNotes(doc);
    this.renderDetails(doc);
  }

  // ------------------------------------------------------------------ display

  renderDisplay(doc: Doc | null): void {
    const el = this.els.display;
    if (!doc) {
      fill(el, h("p", { class: "muted" }, "Open an image to begin."));
      return;
    }
    if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") {
      this.drawHistogram(doc);
      return;
    }
    const unit = state.unit;
    const range = displayRange(doc);
    const pal = palette(doc.palette, doc);
    const all = palettesFor(doc);

    const option = (p: { id: string; name: string }) => h("option", { value: p.id, selected: p.id === pal.id }, p.name);
    const select = h(
      "select",
      {
        id: "palette",
        onchange: (e: Event) => {
          const v = (e.target as HTMLSelectElement).value;
          if (v === "__edit") {
            (e.target as HTMLSelectElement).value = pal.id;
            this.on.editPalettes();
          } else this.on.setPalette(v);
        },
      },
      all.some((p) => p.id === CAMERA_PALETTE) ? h("optgroup", { label: "From the camera" }, option(all.find((p) => p.id === CAMERA_PALETTE)!)) : null,
      h("optgroup", { label: "Built in" }, state.palettes.map(option)),
      state.customPalettes.length ? h("optgroup", { label: "Custom" }, state.customPalettes.map(option)) : null,
      h("option", { value: "__edit" }, "Edit custom palettes…"),
    );
    const swatch = h("div", { class: "swatch" });
    swatch.style.background = gradientCss(pal.colors);

    const rangeInput = (i: 0 | 1) =>
      h("input", {
        type: "number",
        step: "0.1",
        class: "num",
        value: inputValue(range[i], unit),
        "aria-label": i === 0 ? "Range minimum" : "Range maximum",
        onchange: (e: Event) => {
          const v = Number((e.target as HTMLInputElement).value);
          if (!Number.isFinite(v)) return;
          const next: Range = [...range];
          next[i] = toKelvin(v, unit);
          if (next[0] < next[1]) this.on.setRange(next);
          else this.renderDisplay(doc);
        },
      });

    const hasPhoto = !!doc.photo;
    const views: [View, string, string][] = [
      ["thermal", "Thermal", "Thermal image only"],
      ["photo", "Photo", "The camera's photo"],
      ["blend", "Blend", "Thermal over the photo"],
      ["msx", "Edges", "Thermal with the photo's edges embossed (like FLIR MSX)"],
      ["fusion", "Fusion", "The photo with thermal colours only in a temperature interval"],
    ];
    const viewButtons = h(
      "div",
      { class: "segmented wrap", role: "group", "aria-label": "View" },
      views.map(([v, label, title]) =>
        h(
          "button",
          {
            class: state.view === v ? "active" : "",
            disabled: v !== "thermal" && !hasPhoto,
            title: v !== "thermal" && !hasPhoto ? "This file has no photo" : title,
            onclick: () => this.on.setView(v),
          },
          label,
        ),
      ),
    );

    const fusion = state.fusionRange ?? [range[0] + (range[1] - range[0]) * 0.6, range[1]];
    const rotation = doc.presentation.rotation;

    fill(
      el,
      h("div", { class: "row" }, h("label", { for: "palette" }, "Palette"), select),
      swatch,
      h("canvas", { class: "histogram", width: 280, height: 56, title: "Temperature distribution; the shaded band is the display range" }),
      h("div", { class: "row range-row" }, rangeInput(0), h("span", { class: "muted" }, "to"), rangeInput(1), h("span", { class: "unit" }, symbol(unit))),
      h(
        "div",
        { class: "row buttons" },
        h("button", { class: doc.range ? "" : "active", onclick: () => this.on.setRange(null), title: "Fit the range to the image" }, "Auto"),
        doc.info.embeddedRange ? h("button", { onclick: () => this.on.setRange(doc.info.embeddedRange), title: "The range the camera displayed" }, "Camera") : null,
        h("button", { onclick: () => this.on.setRange(narrowed(doc)), title: "Ignore the hottest and coldest 1 % of pixels" }, "Contrast"),
      ),
      h(
        "div",
        { class: "row" },
        h("label", { for: "distribution" }, "Colours"),
        h(
          "select",
          { id: "distribution", title: "How temperatures are spread over the palette", onchange: (e: Event) => this.on.setDistribution((e.target as HTMLSelectElement).value as Distribution) },
          DISTRIBUTIONS.map(([d, name]) => h("option", { value: d, selected: d === doc.distribution }, name)),
        ),
      ),
      h("div", { class: "row" }, h("span", { class: "label" }, "View"), viewButtons),
      state.view === "blend" && hasPhoto
        ? h(
            "div",
            { class: "row" },
            h("label", { for: "blend" }, "Opacity"),
            h("input", { id: "blend", type: "range", min: "0", max: "1", step: "0.01", value: String(state.blend), oninput: (e: Event) => this.on.setBlend(Number((e.target as HTMLInputElement).value)) }),
          )
        : null,
      state.view === "msx" && hasPhoto
        ? h(
            "div",
            { class: "row" },
            h("label", { for: "msx" }, "Edges"),
            h("input", { id: "msx", type: "range", min: "0", max: "1", step: "0.05", value: String(state.msxStrength), onchange: (e: Event) => this.on.setMsxStrength(Number((e.target as HTMLInputElement).value)) }),
          )
        : null,
      state.view === "fusion" && hasPhoto
        ? h(
            "div",
            { class: "row range-row" },
            h("span", { class: "label" }, "Show"),
            tempInput(fusion[0], (k) => k != null && this.on.setFusionRange([k, fusion[1]]), "Fusion from"),
            h("span", { class: "muted" }, "to"),
            tempInput(fusion[1], (k) => k != null && this.on.setFusionRange([fusion[0], k]), "Fusion to"),
            h("span", { class: "unit" }, symbol(unit)),
          )
        : null,
      hasPhoto && state.view !== "thermal" && state.view !== "photo" ? this.alignRow(doc) : null,
      h(
        "div",
        { class: "row buttons" },
        h("span", { class: "label" }, "Rotate"),
        h("button", { title: "Rotate 90° left", "aria-label": "Rotate left", onclick: () => this.on.setRotation(rotation - 90) }, "⟲"),
        h("button", { title: "Rotate 90° right", "aria-label": "Rotate right", onclick: () => this.on.setRotation(rotation + 90) }, "⟳"),
        h("input", {
          type: "number",
          class: "num small-num",
          step: "1",
          value: String(Math.round(rotation * 10) / 10),
          title: "Rotation in degrees (any angle)",
          "aria-label": "Rotation in degrees",
          onchange: (e: Event) => {
            const v = num((e.target as HTMLInputElement).value);
            if (v != null) this.on.setRotation(v);
          },
        }),
        h("span", { class: "unit" }, "°"),
      ),
      h(
        "div",
        { class: "row buttons" },
        h("span", { class: "label" }, "Crop"),
        h("button", { onclick: () => this.on.startCrop(), title: "Drag a rectangle on the image (thermal view, exports and reports)" }, doc.presentation.crop ? "Re-crop" : "Crop…"),
        doc.presentation.crop ? h("button", { onclick: () => this.on.clearCrop() }, "Clear") : null,
      ),
      h(
        "div",
        { class: "row checks" },
        h("label", {}, h("input", { type: "checkbox", checked: state.smooth, onchange: (e: Event) => this.on.setSmooth((e.target as HTMLInputElement).checked) }), "Smooth"),
        h(
          "label",
          {},
          h("input", { type: "checkbox", checked: state.showExtremes, onchange: (e: Event) => this.on.setShowExtremes((e.target as HTMLInputElement).checked) }),
          "Hot/cold spots",
        ),
      ),
    );
    this.drawHistogram(doc);
  }

  /** Nudges the thermal image on the photo when the camera's alignment is off. */
  private alignRow(doc: Doc): HTMLElement {
    const [dx, dy, s] = doc.presentation.align ?? [0, 0, 1];
    const step = Math.max(1, Math.round((doc.photo?.width ?? 640) / 320));
    const set = (a: [number, number, number]) => this.on.setAlign([Math.round(a[0]), Math.round(a[1]), Math.round(a[2] * 1000) / 1000]);
    const b = (label: string, title: string, a: [number, number, number]) => h("button", { title, "aria-label": title, onclick: () => set(a) }, label);
    return h(
      "div",
      { class: "row buttons align-row" },
      h("span", { class: "label", title: "Move or scale the thermal image on the photo if they don't line up" }, "Align"),
      b("←", "Move left", [dx - step, dy, s]),
      b("→", "Move right", [dx + step, dy, s]),
      b("↑", "Move up", [dx, dy - step, s]),
      b("↓", "Move down", [dx, dy + step, s]),
      b("−", "Smaller", [dx, dy, s * 0.99]),
      b("+", "Larger", [dx, dy, s * 1.01]),
      doc.presentation.align ? h("button", { title: "Back to the camera's alignment", onclick: () => this.on.setAlign(null) }, "Reset") : null,
    );
  }

  private drawHistogram(doc: Doc): void {
    const canvas = this.els.display.querySelector<HTMLCanvasElement>("canvas.histogram");
    if (!canvas || !doc.dataRange) return;
    const ctx = canvas.getContext("2d")!;
    const [lo, hi] = doc.dataRange;
    const bins = new Array<number>(70).fill(0);
    const span = hi - lo || 1;
    for (const v of doc.kelvin) {
      if (Number.isNaN(v)) continue;
      bins[Math.min(bins.length - 1, Math.floor(((v - lo) / span) * bins.length))]! += 1;
    }
    const peak = Math.max(...bins, 1);
    const { width: w, height: hh } = canvas;
    ctx.clearRect(0, 0, w, hh);
    const [r0, r1] = displayRange(doc);
    const x = (k: number) => ((k - lo) / span) * w;
    ctx.fillStyle = "rgba(56,189,248,0.14)";
    ctx.fillRect(Math.max(0, x(r0)), 0, Math.min(w, x(r1)) - Math.max(0, x(r0)), hh);
    const pal = palette(doc.palette, doc).colors;
    const bw = w / bins.length;
    bins.forEach((n, i) => {
      const k = lo + ((i + 0.5) / bins.length) * span;
      const t = Math.min(255, Math.max(0, Math.floor(((k - r0) / (r1 - r0 || 1)) * 256)));
      const c = pal[Math.min(pal.length - 1, Math.floor((t * pal.length) / 256))]!;
      ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
      const bh = Math.max(n > 0 ? 1 : 0, (Math.sqrt(n) / Math.sqrt(peak)) * (hh - 4));
      ctx.fillRect(i * bw + 0.5, hh - bh, bw - 1, bh);
    });
  }

  // ------------------------------------------------------------------ video

  renderVideo(doc: Doc | null): void {
    const v = doc?.video;
    this.videoSection.hidden = !v;
    if (!doc || !v) return;
    const el = this.els.video;
    const info = doc.info.video!;
    const n = info.frames;
    const t = info.times[v.frame] ?? 0;
    const existing = el.querySelector<HTMLInputElement>("input.frame-slider");
    if (existing && el.contains(document.activeElement) && document.activeElement === existing) {
      el.querySelector(".frame-label")!.textContent = `Frame ${v.frame + 1} / ${n} · ${t.toFixed(2)} s`;
      return;
    }
    fill(
      el,
      h("input", {
        type: "range",
        class: "frame-slider",
        min: "0",
        max: String(n - 1),
        step: "1",
        value: String(v.frame),
        "aria-label": "Frame",
        oninput: (e: Event) => this.on.seek(Number((e.target as HTMLInputElement).value)),
      }),
      h("div", { class: "row frame-row" },
        h("span", { class: "frame-label" }, `Frame ${v.frame + 1} / ${n} · ${t.toFixed(2)} s`),
        h("span", { class: "spacer" }),
        h("button", { title: "First frame", "aria-label": "First frame", onclick: () => this.on.seek(0) }, "⏮"),
        h("button", { title: "Previous frame (,)", "aria-label": "Previous frame", onclick: () => this.on.seek(Math.max(0, v.frame - 1)) }, "◀"),
        h("button", { class: "play", title: "Play / pause (K)", "aria-label": v.playing ? "Pause" : "Play", onclick: () => this.on.togglePlay() }, v.playing ? "⏸" : "▶"),
        h("button", { title: "Next frame (.)", "aria-label": "Next frame", onclick: () => this.on.seek(Math.min(n - 1, v.frame + 1)) }, "▶|"),
        h("button", { title: "Last frame", "aria-label": "Last frame", onclick: () => this.on.seek(n - 1) }, "⏭"),
      ),
      h(
        "div",
        { class: "row buttons" },
        h("span", { class: "label" }, "Segment"),
        h("button", { title: "Set the segment start to this frame", onclick: () => this.on.setInOut(v.frame, Math.max(v.frame, v.last)) }, `In ${v.first + 1}`),
        h("button", { title: "Set the segment end to this frame", onclick: () => this.on.setInOut(Math.min(v.first, v.frame), v.frame) }, `Out ${v.last + 1}`),
        h("span", { class: "muted small" }, info.frameRate ? `${info.frameRate.toFixed(1)} fps` : ""),
      ),
      h(
        "div",
        { class: "row buttons wrap" },
        h("button", { onclick: () => this.on.plotSeries(), title: "Plot every measurement across the segment" }, "Plot over time"),
        h("button", { onclick: () => this.on.exportVideo(), title: "Save the segment as an AVI video with palette and measurements" }, "Export AVI…"),
        h("button", { onclick: () => this.on.exportSegment(), title: "Save the segment as a radiometric SEQ file" }, "Save SEQ…"),
      ),
      h(
        "div",
        { class: "row checks" },
        h("label", { title: "High-sensitivity mode: shows what changes between frames, such as gas plumes" }, h("input", { type: "checkbox", checked: v.hsm, onchange: (e: Event) => this.on.setHsm({ hsm: (e.target as HTMLInputElement).checked }) }), "High sensitivity (gas)"),
        h("label", { title: "Average 2×2 pixel blocks to reduce noise" }, h("input", { type: "checkbox", checked: v.binning, onchange: (e: Event) => this.on.setHsm({ binning: (e.target as HTMLInputElement).checked }) }), "Binning"),
      ),
      v.hsm
        ? h(
            "div",
            { class: "row" },
            h("span", { class: "label" }, "Reference"),
            h("input", { type: "number", class: "num small-num", min: "1", max: "60", value: String(v.hsmReference), title: "Frames averaged as the background", onchange: (e: Event) => this.on.setHsm({ hsmReference: Math.max(1, Math.min(60, Number((e.target as HTMLInputElement).value) || 8)) }) }),
            h("span", { class: "muted small" }, "frames, ±"),
            deltaInput(v.hsmSpan, (dk) => this.on.setHsm({ hsmSpan: Math.max(0.01, dk) }), "HSM sensitivity"),
            h("span", { class: "unit" }, state.unit === "K" ? "K" : symbol(state.unit)),
          )
        : null,
    );
  }

  // ------------------------------------------------------------------ parameters

  renderParams(doc: Doc | null): void {
    const el = this.els.params;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
    if (doc.info.editable.length === 0) {
      fill(el, h("p", { class: "muted" }, `${doc.info.formatLabel} files store final temperatures, so their parameters cannot be changed.`));
      return;
    }
    const unit = state.unit;
    const rows = PARAM_FIELDS.filter((f) => doc.info.editable.includes(f.key)).map((f) => {
      const fileValue = doc.info.fileParams[f.key];
      const current = doc.params[f.key];
      const changed = fieldValue(f, current, unit) !== fieldValue(f, fileValue, unit);
      const input = h("input", {
        type: "number",
        class: "num",
        id: `param-${f.key}`,
        step: String(f.step),
        min: String(f.kind === "temperature" ? fromKelvin(toKelvin(f.min, "C"), unit) : f.min),
        max: String(f.kind === "temperature" ? fromKelvin(toKelvin(f.max, "C"), unit) : f.max),
        value: fieldValue(f, current, unit),
        onchange: (e: Event) => {
          const v = parseField(f, (e.target as HTMLInputElement).value, unit);
          if (v == null) return;
          this.on.setParams({ ...doc.params, [f.key]: v });
        },
      });
      return h(
        "div",
        { class: `param ${changed ? "changed" : ""}` },
        h("label", { for: `param-${f.key}` }, f.label),
        input,
        h("span", { class: "unit" }, unitLabel(f, unit)),
        h("span", { class: "file-value", title: "Value stored in the file" }, changed ? `file ${fieldValue(f, fileValue, unit)}` : ""),
      );
    });

    const presets = h(
      "select",
      {
        class: "presets",
        "aria-label": "Emissivity presets",
        onchange: (e: Event) => {
          const v = Number((e.target as HTMLSelectElement).value);
          if (v > 0) this.on.setParams({ ...doc.params, emissivity: v });
        },
      },
      h("option", { value: "" }, "Typical emissivities…"),
      EMISSIVITY_PRESETS.map(([name, v]) => h("option", { value: String(v) }, `${name} — ${v.toFixed(2)}`)),
    );

    const anyChanged = PARAM_FIELDS.some((f) => doc.params[f.key] !== doc.info.fileParams[f.key]);
    fill(
      el,
      h("div", { class: "params" }, rows),
      doc.info.editable.includes("emissivity") ? presets : null,
      h(
        "div",
        { class: "row buttons" },
        h("button", { disabled: !anyChanged, onclick: () => this.on.resetParams() }, "Reset to file"),
        state.docs.length > 1
          ? h("button", { onclick: () => this.on.applyParamsToAll(), title: "Use these values for every open image of a format that supports them" }, "Apply to all images")
          : null,
      ),
    );
  }

  // ------------------------------------------------------------------ measurements

  renderMeasurements(doc: Doc | null): void {
    const el = this.els.measurements;
    if (!doc) {
      fill(el);
      return;
    }
    const unit = state.unit;
    const focused = el.contains(document.activeElement);
    const existing = el.querySelectorAll("tbody tr.m-row");
    const showCoverage = doc.isotherms.length > 0 && doc.measurements.some((m) => isArea(m.shape.kind));
    if (focused && existing.length === doc.measurements.length) {
      // Keep the input being edited; refresh numbers only.
      existing.forEach((tr, i) => {
        const s = doc.stats[i] ?? null;
        const cells = tr.querySelectorAll("td.n");
        cells[0]!.textContent = temp(s?.min, unit);
        cells[1]!.textContent = temp(s?.max, unit);
        cells[2]!.textContent = temp(s?.avg, unit);
        if (showCoverage && cells[3]) cells[3].textContent = coverageText(doc.coverage[i]);
        tr.classList.toggle("selected", state.selected === i);
        tr.classList.toggle("alarm", alarmOn(doc, i));
      });
      this.renderMeasurementDetail(doc);
      return;
    }
    if (doc.measurements.length === 0) {
      fill(
        el,
        h("p", { class: "muted" }, "Pick a tool in the toolbar and draw on the image. Polygon and polyline: click points, double-click or Enter to finish."),
        h("div", { class: "row" }, h("span", { class: "label" }, "Wand ±"), deltaInput(state.wandTolerance, (dk) => (state.wandTolerance = dk), "Magic wand tolerance"), h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit))),
        imageStats(doc, unit),
        h("div", { class: "measurement-detail" }),
      );
      return;
    }
    const rows = doc.measurements.map((m, i) => {
      const s = doc.stats[i] ?? null;
      return h(
        "tr",
        { class: `m-row ${state.selected === i ? "selected" : ""} ${alarmOn(doc, i) ? "alarm" : ""}`, onclick: () => this.on.select(i) },
        h(
          "td",
          {},
          h("input", {
            class: "label-input",
            value: m.label,
            "aria-label": "Measurement label",
            onchange: (e: Event) => this.on.renameMeasurement(i, (e.target as HTMLInputElement).value.trim() || m.label),
          }),
        ),
        h("td", { class: "n" }, temp(s?.min, unit)),
        h("td", { class: "n" }, temp(s?.max, unit)),
        h("td", { class: "n" }, temp(s?.avg, unit)),
        showCoverage ? h("td", { class: "n", title: "Share of the area inside the isotherms" }, coverageText(doc.coverage[i])) : null,
        h(
          "td",
          {},
          h(
            "button",
            {
              class: "icon",
              title: "Delete measurement",
              "aria-label": `Delete ${m.label}`,
              onclick: (e: Event) => {
                e.stopPropagation();
                this.on.deleteMeasurement(i);
              },
            },
            "×",
          ),
        ),
      );
    });
    fill(
      el,
      h(
        "table",
        { class: "measurements" },
        h("thead", {}, h("tr", {}, h("th", {}, "Label"), h("th", {}, "Min"), h("th", {}, "Max"), h("th", {}, "Avg"), showCoverage ? h("th", { title: "Share inside the isotherms" }, "Iso %") : null, h("th", {}))),
        h("tbody", {}, rows),
      ),
      doc.measurements.length >= 2 ? this.deltaT(doc) : null,
      h("div", { class: "row" }, h("span", { class: "label" }, "Wand ±"), deltaInput(state.wandTolerance, (dk) => (state.wandTolerance = dk), "New magic wand tolerance"), h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)), h("span", { class: "muted small" }, "for new wands")),
      imageStats(doc, unit),
      h("div", { class: "measurement-detail" }),
    );
    this.renderMeasurementDetail(doc);
  }

  /** Alarm, wand tolerance and profile for the selected measurement. */
  private renderMeasurementDetail(doc: Doc): void {
    const host = this.els.measurements.querySelector<HTMLElement>(".measurement-detail");
    if (!host) return;
    const i = state.selected;
    const m = i === null ? null : doc.measurements[i];
    if (i === null || !m) {
      fill(host);
      this.profileFor = "";
      return;
    }
    if (host.contains(document.activeElement)) return;
    const unit = state.unit;
    const alarm: Alarm = m.alarm ?? { stat: "max", above: null, below: null };
    const status = alarmOn(doc, i) ? h("span", { class: "alarm-badge" }, "ALARM") : m.alarm && (m.alarm.above != null || m.alarm.below != null) ? h("span", { class: "muted small" }, "OK") : null;
    const parts: (HTMLElement | null)[] = [
      h("div", { class: "detail-title" }, `${m.label} · ${kindName(m.shape.kind)}`),
      h(
        "div",
        { class: "row alarm-row" },
        h("span", { class: "label" }, "Alarm"),
        h(
          "select",
          { "aria-label": "Alarm statistic", onchange: (e: Event) => this.on.setAlarm(i, { ...alarm, stat: (e.target as HTMLSelectElement).value as Alarm["stat"] }) },
          (["max", "min", "avg"] as const).map((s) => h("option", { value: s, selected: alarm.stat === s }, s)),
        ),
        h("span", { class: "muted small" }, ">"),
        tempInput(alarm.above, (k) => this.on.setAlarm(i, { ...alarm, above: k }), "Alarm above"),
        h("span", { class: "muted small" }, "<"),
        tempInput(alarm.below, (k) => this.on.setAlarm(i, { ...alarm, below: k }), "Alarm below"),
        status,
      ),
    ];
    if (m.shape.kind === "wand") {
      parts.push(
        h(
          "div",
          { class: "row" },
          h("span", { class: "label" }, "Tolerance"),
          deltaInput(m.shape.tolerance, (dk) => this.on.setWandTolerance(i, dk), "Wand tolerance"),
          h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)),
          h("span", { class: "muted small" }, `${doc.stats[i]?.count ?? 0} px`),
        ),
      );
    }
    const profile = isPath(m.shape.kind) ? h("div", { class: "profile" }, h("p", { class: "muted small" }, "Loading profile…")) : null;
    parts.push(profile);
    fill(host, parts);
    if (profile) {
      const key = `${doc.info.id}|${JSON.stringify(m.shape)}|${doc.kelvin.length}|${doc.stats[i]?.avg}|${unit}`;
      this.profileFor = key;
      void api.profile(doc.info.id, m.shape).then((points) => {
        if (this.profileFor !== key) return;
        fill(
          profile,
          lineChart([{ name: m.label, color: SERIES_COLORS[0]!, points: points.map((p) => [p.d, fromKelvin(p.k, unit)] as [number, number]) }], {
            xLabel: "Distance (px)",
            yLabel: symbol(unit),
            height: 110,
          }),
        );
      });
    }
  }

  private deltaA = 0;
  private deltaB = 1;

  private deltaT(doc: Doc): HTMLElement {
    const n = doc.measurements.length;
    if (this.deltaA >= n) this.deltaA = 0;
    if (this.deltaB >= n) this.deltaB = Math.min(1, n - 1);
    const unit = state.unit;
    const pick = (value: number, set: (v: number) => void) =>
      h(
        "select",
        { onchange: (e: Event) => (set(Number((e.target as HTMLSelectElement).value)), this.renderMeasurements(doc)) },
        doc.measurements.map((m, i) => h("option", { value: String(i), selected: i === value }, m.label)),
      );
    const a: Stats | null = doc.stats[this.deltaA] ?? null;
    const b: Stats | null = doc.stats[this.deltaB] ?? null;
    return h(
      "div",
      { class: "delta" },
      h("span", { class: "label" }, "ΔT"),
      pick(this.deltaA, (v) => (this.deltaA = v)),
      h("span", { class: "muted" }, "−"),
      pick(this.deltaB, (v) => (this.deltaB = v)),
      h(
        "span",
        { class: "delta-values" },
        h("span", { title: "Difference of maxima" }, `max ${a && b ? delta(a.max - b.max, unit) : "–"}`),
        h("span", { title: "Difference of averages" }, `avg ${a && b ? delta(a.avg - b.avg, unit) : "–"}`),
      ),
    );
  }

  // ------------------------------------------------------------------ formulas

  renderFormulas(doc: Doc | null): void {
    const el = this.els.formulas;
    if (!doc) {
      fill(el);
      return;
    }
    const unit = state.unit;
    const focused = el.contains(document.activeElement);
    const rows = el.querySelectorAll<HTMLElement>(".formula");
    if (focused && rows.length === doc.formulas.length) {
      rows.forEach((row, i) => (row.querySelector(".formula-result")!.textContent = resultText(doc, i, unit)));
      return;
    }
    const update = (i: number, patch: Partial<Formula>) => {
      const next = doc.formulas.map((f, k) => (k === i ? { ...f, ...patch } : f));
      this.on.setFormulas(next);
    };
    const labels = doc.measurements.map((m) => m.label);
    const ref = (l: string) => (/^[\p{L}_][\p{L}\p{N}_]*$/u.test(l) ? l : `[${l}]`);
    const presets: [string, () => Formula | null][] = [
      ["ΔT of maxima (first two)", () => (labels.length >= 2 ? { label: "dT", expression: `${ref(labels[0]!)}.max - ${ref(labels[1]!)}.max`, kind: "difference" } : null)],
      ["Hottest of all", () => (labels.length ? { label: "Hottest", expression: `max(${labels.map((l) => `${ref(l)}.max`).join(", ")})`, kind: "temperature" } : null)],
      ["Average of all", () => (labels.length ? { label: "Mean", expression: `avg(${labels.map((l) => `${ref(l)}.avg`).join(", ")})`, kind: "temperature" } : null)],
      ["Rise over a reference temperature", () => (labels.length ? { label: "Rise", expression: `${ref(labels[0]!)}.max - 20`, kind: "difference" } : null)],
      ["Thermal index (inside 20, outside 0)", () => (labels.length ? { label: "TI", expression: `(${ref(labels[0]!)}.min - 0) / (20 - 0) * 100`, kind: "number" } : null)],
      ["Empty formula", () => ({ label: `F${doc.formulas.length + 1}`, expression: "", kind: "temperature" })],
    ];
    fill(
      el,
      doc.formulas.length === 0 ? h("p", { class: "muted small" }, "Combine measurement results, e.g. Bx1.max - Sp1. Statistics: .max .min .avg .sd .count; functions abs, min, max, avg, sqrt, round.") : null,
      doc.formulas.map((f, i) =>
        h(
          "div",
          { class: "formula" },
          h("input", { class: "label-input", value: f.label, "aria-label": "Formula label", onchange: (e: Event) => update(i, { label: (e.target as HTMLInputElement).value.trim() }) }),
          h("input", { type: "text", class: "expr", value: f.expression, spellcheck: false, placeholder: "Bx1.max - Sp1", "aria-label": "Formula", onchange: (e: Event) => update(i, { expression: (e.target as HTMLInputElement).value }) }),
          h(
            "select",
            { "aria-label": "Result type", title: "How the result is shown", onchange: (e: Event) => update(i, { kind: (e.target as HTMLSelectElement).value as ResultKind }) },
            ([["temperature", "°"], ["difference", "Δ"], ["number", "#"]] as const).map(([k, l]) => h("option", { value: k, selected: f.kind === k }, l)),
          ),
          h("button", { class: "icon", title: "Delete formula", "aria-label": `Delete ${f.label}`, onclick: () => this.on.setFormulas(doc.formulas.filter((_, k) => k !== i)) }, "×"),
          h("div", { class: `formula-result ${doc.formulaResults[i]?.error ? "error" : ""}` }, resultText(doc, i, unit)),
        ),
      ),
      h(
        "select",
        {
          class: "presets",
          "aria-label": "Add a formula",
          onchange: (e: Event) => {
            const sel = e.target as HTMLSelectElement;
            const make = presets[Number(sel.value)]?.[1];
            sel.value = "";
            const f = make?.();
            if (f) this.on.setFormulas([...doc.formulas, f]);
          },
        },
        h("option", { value: "" }, "Add a formula…"),
        presets.map(([name], i) => h("option", { value: String(i) }, name)),
      ),
    );
  }

  // ------------------------------------------------------------------ isotherms

  renderIsotherms(doc: Doc | null): void {
    const el = this.els.isotherms;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
    const unit = state.unit;
    const range = displayRange(doc);
    const update = (i: number, iso: Isotherm) => this.on.setIsotherms(doc.isotherms.map((x, k) => (k === i ? iso : x)));
    const air = doc.params.atmosphereK ?? 293.15;
    const make: Record<string, () => Isotherm> = {
      above: () => ({ kind: "above", t: range[0] + (range[1] - range[0]) * 0.8, color: [255, 40, 40], opacity: 1 }),
      below: () => ({ kind: "below", t: range[0] + (range[1] - range[0]) * 0.2, color: [40, 120, 255], opacity: 1 }),
      interval: () => ({ kind: "interval", lo: range[0] + (range[1] - range[0]) * 0.4, hi: range[0] + (range[1] - range[0]) * 0.6, color: [60, 220, 90], opacity: 1 }),
      condensation: () => ({ kind: "condensation", airK: air, rh: doc.params.humidity ?? 0.5, limit: 0.8, color: [30, 144, 255], opacity: 0.85 }),
      insulation: () => ({ kind: "insulation", insideK: 293.15, outsideK: 273.15, index: 0.7, color: [0, 200, 255], opacity: 0.85 }),
    };
    const pct = (v: number, set: (v: number) => void, label: string) =>
      h("input", {
        type: "number",
        class: "num small-num",
        min: "0",
        max: "100",
        step: "1",
        value: String(Math.round(v * 100)),
        "aria-label": label,
        onchange: (e: Event) => {
          const n = num((e.target as HTMLInputElement).value);
          if (n != null && n >= 0 && n <= 100) set(n / 100);
        },
      });
    const rows = doc.isotherms.map((iso, i) => {
      const values: (HTMLElement | string)[] = [];
      switch (iso.kind) {
        case "above":
        case "below":
          values.push(tempInput(iso.t, (k) => k != null && update(i, { ...iso, t: k }), "Isotherm temperature"), symbol(unit));
          break;
        case "interval":
          values.push(
            tempInput(iso.lo, (k) => k != null && update(i, { ...iso, lo: k }), "From"),
            "–",
            tempInput(iso.hi, (k) => k != null && update(i, { ...iso, hi: k }), "To"),
            symbol(unit),
          );
          break;
        case "condensation":
          values.push(
            "air",
            tempInput(iso.airK, (k) => k != null && update(i, { ...iso, airK: k }), "Air temperature"),
            "RH",
            pct(iso.rh, (v) => update(i, { ...iso, rh: v }), "Air relative humidity"),
            "% limit",
            pct(iso.limit, (v) => update(i, { ...iso, limit: Math.max(0.01, v) }), "Surface humidity limit"),
            "%",
          );
          break;
        case "insulation":
          values.push(
            "in",
            tempInput(iso.insideK, (k) => k != null && update(i, { ...iso, insideK: k }), "Indoor temperature"),
            "out",
            tempInput(iso.outsideK, (k) => k != null && update(i, { ...iso, outsideK: k }), "Outdoor temperature"),
            "index <",
            pct(iso.index, (v) => update(i, { ...iso, index: v }), "Thermal index limit"),
            "%",
          );
          break;
      }
      return h(
        "div",
        { class: "isotherm" },
        h("div", { class: "row" },
          h("input", { type: "color", value: rgbHex(iso.color), "aria-label": "Isotherm colour", onchange: (e: Event) => update(i, { ...iso, color: hexRgb((e.target as HTMLInputElement).value) }) }),
          h("span", { class: "iso-name" }, isothermText(iso, unit)),
          h("span", { class: "spacer" }),
          h("input", { type: "range", class: "opacity", min: "0.1", max: "1", step: "0.05", value: String(iso.opacity), title: "Opacity", "aria-label": "Opacity", onchange: (e: Event) => update(i, { ...iso, opacity: Number((e.target as HTMLInputElement).value) }) }),
          h("button", { class: "icon", title: "Remove", "aria-label": "Remove isotherm", onclick: () => this.on.setIsotherms(doc.isotherms.filter((_, k) => k !== i)) }, "×"),
        ),
        h("div", { class: "row iso-values" }, values.map((v) => (typeof v === "string" ? h("span", { class: "muted small" }, v) : v))),
      );
    });
    fill(
      el,
      rows,
      h(
        "select",
        {
          class: "presets",
          "aria-label": "Add an isotherm",
          onchange: (e: Event) => {
            const sel = e.target as HTMLSelectElement;
            const f = make[sel.value];
            sel.value = "";
            if (f) this.on.setIsotherms([...doc.isotherms, f()]);
          },
        },
        h("option", { value: "" }, "Add an isotherm or alarm…"),
        h("option", { value: "above" }, "Above a temperature"),
        h("option", { value: "below" }, "Below a temperature"),
        h("option", { value: "interval" }, "Interval"),
        h("option", { value: "condensation" }, "Condensation / mould (humidity)"),
        h("option", { value: "insulation" }, "Insulation (thermal index)"),
      ),
    );
  }

  // ------------------------------------------------------------------ auto-inspect

  renderInspect(doc: Doc | null): void {
    const el = this.els.inspect;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
    const s = state.inspect;
    const unit = state.unit;
    const set = (patch: Partial<typeof s>) => {
      Object.assign(s, patch);
      this.renderInspect(doc);
    };
    fill(
      el,
      h(
        "div",
        { class: "row" },
        h("span", { class: "label" }, "Find"),
        h(
          "select",
          { "aria-label": "What to find", onchange: (e: Event) => set({ mode: (e.target as HTMLSelectElement).value as typeof s.mode }) },
          h("option", { value: "hot", selected: s.mode === "hot" }, "Hot spots (electrical, NETA)"),
          h("option", { value: "cold", selected: s.mode === "cold" }, "Cold spots (building)"),
          h("option", { value: "both", selected: s.mode === "both" }, "Both"),
        ),
      ),
      h(
        "div",
        { class: "row" },
        h("span", { class: "label" }, "Min ΔT"),
        deltaInput(s.minDelta, (dk) => set({ minDelta: dk }), "Smallest difference"),
        h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)),
        h("span", { class: "muted small" }, "size ≥"),
        h("input", { type: "number", class: "num small-num", min: "1", value: String(s.minPixels), "aria-label": "Smallest size in pixels", onchange: (e: Event) => set({ minPixels: Math.max(1, Number((e.target as HTMLInputElement).value) || 1) }) }),
        h("span", { class: "muted small" }, "px"),
      ),
      h(
        "div",
        { class: "row" },
        h("span", { class: "label" }, "Reference"),
        tempInput(s.referenceK, (k) => set({ referenceK: k }), "Reference temperature (empty: image median)"),
        h("span", { class: "muted small" }, s.referenceK == null ? "median" : symbol(unit)),
        s.mode !== "cold"
          ? h(
              "select",
              { "aria-label": "NETA basis", title: "NETA MTS Table 100.18 basis", onchange: (e: Event) => set({ basis: (e.target as HTMLSelectElement).value as typeof s.basis }) },
              h("option", { value: "similar", selected: s.basis === "similar" }, "vs similar"),
              h("option", { value: "ambient", selected: s.basis === "ambient" }, "vs ambient"),
            )
          : null,
      ),
      s.mode !== "hot"
        ? h(
            "div",
            { class: "row" },
            h("span", { class: "label" }, "In / out"),
            tempInput(s.insideK, (k) => set({ insideK: k }), "Indoor temperature"),
            tempInput(s.outsideK, (k) => set({ outsideK: k }), "Outdoor temperature"),
            h("span", { class: "unit" }, symbol(unit)),
          )
        : null,
      h(
        "div",
        { class: "row buttons" },
        h("button", { class: "primary", onclick: () => this.on.runInspect() }, "Inspect"),
        doc.findings.length ? h("button", { onclick: () => this.on.findingsToMeasurements(), title: "Add a box measurement for each finding" }, "Add as boxes") : null,
        doc.findings.length ? h("button", { onclick: () => this.on.clearFindings() }, "Clear") : null,
      ),
      doc.findings.length
        ? h(
            "table",
            { class: "measurements findings" },
            h("tbody", {}, doc.findings.map((f) =>
              h(
                "tr",
                { title: f.severity.action },
                h("td", {}, h("i", { class: "dot", style: `background:${severityColor(f.severity.level)}` }), f.label),
                h("td", { class: "n" }, delta(f.deltaK, unit)),
                h("td", {}, f.severity.label),
              ),
            )),
          )
        : null,
    );
  }

  // ------------------------------------------------------------------ inspection

  renderInspection(doc: Doc | null): void {
    const el = this.els.inspection;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement)) return;
    const template = allTemplates().find((t) => t.id === loadSettings().templateId) ?? allTemplates()[0];
    const fieldNames = [...new Set([...(template?.imageFields ?? []), ...Object.keys(doc.fields)])];
    const assets = [...new Set(state.docs.map((d) => d.asset).filter(Boolean))];
    fill(
      el,
      h(
        "div",
        { class: "row" },
        h("label", { for: "asset" }, "Asset"),
        h("input", { id: "asset", type: "text", list: "assets", value: doc.asset, placeholder: "e.g. Panel B, breaker 3", title: "Images with the same asset form a trend over visits", onchange: (e: Event) => this.on.setAsset((e.target as HTMLInputElement).value.trim()) }),
        h("datalist", { id: "assets" }, assets.map((a) => h("option", { value: a }))),
      ),
      fieldNames.map((name) =>
        h(
          "div",
          { class: "row" },
          h("label", { title: name }, name),
          h("input", { type: "text", value: doc.fields[name] ?? "", "aria-label": name, onchange: (e: Event) => this.on.setField(name, (e.target as HTMLInputElement).value) }),
        ),
      ),
      template ? h("p", { class: "muted small" }, `Fields from the "${template.name}" report template.`) : null,
      doc.annotations.length
        ? h(
            "div",
            { class: "annotations" },
            h("div", { class: "detail-title" }, "Notes and sketches on the image"),
            doc.annotations.map((a, i) =>
              h(
                "div",
                { class: "row" },
                a.kind === "text"
                  ? h("input", { type: "text", value: a.text, "aria-label": "Note text", onchange: (e: Event) => this.on.setAnnotationText(i, (e.target as HTMLInputElement).value) })
                  : h("span", { class: "muted" }, `Sketch, ${a.points.length} points`),
                h("button", { class: "icon", title: "Remove", "aria-label": "Remove", onclick: () => this.on.deleteAnnotation(i) }, "×"),
              ),
            ),
          )
        : h("p", { class: "muted small" }, "Use the Note and Sketch tools to annotate the image."),
    );
  }

  // ------------------------------------------------------------------ notes, details

  renderNotes(doc: Doc | null): void {
    const el = this.els.notes;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement)) return;
    fill(
      el,
      h("textarea", {
        rows: 3,
        placeholder: "Findings, location, recommended action… (included in the report)",
        value: doc.notes,
        oninput: (e: Event) => this.on.setNotes((e.target as HTMLTextAreaElement).value),
      }),
    );
  }

  renderDetails(doc: Doc | null): void {
    const el = this.els.details;
    if (!doc) {
      fill(el);
      return;
    }
    const c = doc.info.camera;
    const v = doc.info.video;
    const rows: [string, string | null][] = [
      ["Camera", c.description],
      ["Captured", c.dateTime],
      ["Serial", c.serialNumber],
      ["Lens", c.lens ? `${c.lens}${c.fieldOfView ? ` · ${c.fieldOfView.toFixed(1)}°` : ""}` : c.fieldOfView ? `${c.fieldOfView.toFixed(1)}°` : null],
      ["GPS", c.latitude != null && c.longitude != null ? `${c.latitude.toFixed(6)}, ${c.longitude.toFixed(6)}` : null],
      ["Altitude", c.altitude != null ? `${c.altitude.toFixed(1)} m${c.relativeAltitude != null ? ` (${c.relativeAltitude.toFixed(1)} m above take-off)` : ""}` : null],
      ["Heading", c.heading != null ? `${c.heading.toFixed(0)}°` : null],
      ["Gimbal", c.gimbal ? `roll ${c.gimbal[0].toFixed(1)}° · pitch ${c.gimbal[1].toFixed(1)}° · yaw ${c.gimbal[2].toFixed(1)}°` : null],
      ["Format", doc.info.formatLabel],
      ["Size", `${doc.info.width} × ${doc.info.height}${doc.photo ? ` · photo ${doc.photo.width} × ${doc.photo.height}` : ""}`],
      ["Video", v ? `${v.frames} frames${v.frameRate ? ` · ${v.frameRate.toFixed(1)} fps` : ""}${v.compressed ? " · CSQ" : " · SEQ"}` : null],
      ["File", doc.info.path],
    ];
    fill(el, h("dl", { class: "details" }, rows.filter(([, v]) => v).map(([k, v]) => [h("dt", {}, k), h("dd", { title: v ?? "" }, v ?? "")]).flat()));
  }
}

function kindName(kind: string): string {
  return ({ spot: "Spot", box: "Box", ellipse: "Ellipse", line: "Line", polygon: "Polygon", polyline: "Polyline", wand: "Magic wand" } as Record<string, string>)[kind] ?? kind;
}

function coverageText(c: number | null | undefined): string {
  return c == null ? "" : `${(c * 100).toFixed(1)} %`;
}

export function alarmOn(doc: Doc, i: number): boolean {
  const m = doc.measurements[i];
  const s = doc.stats[i];
  if (!m?.alarm || !s) return false;
  const v = s[m.alarm.stat];
  return (m.alarm.above != null && v > m.alarm.above) || (m.alarm.below != null && v < m.alarm.below);
}

function resultText(doc: Doc, i: number, unit: Unit): string {
  const r = doc.formulaResults[i];
  const f = doc.formulas[i];
  if (!r || !f) return "";
  if (r.error) return r.error;
  if (r.value == null) return "–";
  const v = r.value;
  if (f.kind === "number") return String(Math.round(v * 1000) / 1000);
  if (f.kind === "difference") return `${v >= 0 ? "+" : ""}${v.toFixed(1)} ${unit === "K" ? "K" : symbol(unit)}`;
  return `${v.toFixed(1)} ${symbol(unit)}`;
}

function imageStats(doc: Doc, unit: Unit): HTMLElement {
  const r = doc.dataRange;
  return h("p", { class: "muted small" }, r ? `Whole image: ${temp(r[0], unit)} to ${temp(r[1], unit)}` : "No valid temperatures with these parameters.");
}

/** Range that skips the hottest and coldest 1 % of pixels. */
function narrowed(doc: Doc): Range | null {
  const values = Array.from(doc.kelvin).filter((v) => !Number.isNaN(v));
  if (values.length < 10) return null;
  values.sort((a, b) => a - b);
  const lo = values[Math.floor(values.length * 0.01)]!;
  const hi = values[Math.min(values.length - 1, Math.ceil(values.length * 0.99))]!;
  return hi > lo ? [lo, hi] : null;
}

export function gradientCss(colors: [number, number, number][]): string {
  const n = colors.length;
  const stops = [0, 32, 64, 96, 128, 160, 192, 224, 255].map((i) => {
    const c = colors[Math.min(n - 1, Math.floor((i * n) / 256))]!;
    return `rgb(${c[0]},${c[1]},${c[2]}) ${((i / 255) * 100).toFixed(1)}%`;
  });
  return `linear-gradient(90deg, ${stops.join(", ")})`;
}

// The right-hand panel: display settings, capture parameters, measurements, notes and details.
import type { Param, Params, Range, Stats, Unit } from "./api";
import { fill, h } from "./dom";
import { displayRange, palette, state, type Doc, type View } from "./state";
import { delta, fromKelvin, inputValue, symbol, temp, toKelvin } from "./units";

export interface PanelHandlers {
  setPalette(id: string): void;
  setRange(range: Range | null): void;
  setView(view: View): void;
  setBlend(value: number): void;
  setSmooth(on: boolean): void;
  setShowExtremes(on: boolean): void;
  setParams(params: Params): void;
  resetParams(): void;
  applyParamsToAll(): void;
  select(index: number | null): void;
  renameMeasurement(index: number, label: string): void;
  deleteMeasurement(index: number): void;
  setNotes(notes: string): void;
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

function section(title: string, body: Node, extra?: Node): HTMLElement {
  return h("section", { class: "panel-section" }, h("header", {}, h("h2", {}, title), extra ?? null), body);
}

export class Panel {
  private els = {
    display: h("div"),
    params: h("div"),
    measurements: h("div"),
    notes: h("div"),
    details: h("div"),
  };

  constructor(
    host: HTMLElement,
    private on: PanelHandlers,
  ) {
    host.append(
      section("Display", this.els.display),
      section("Measurements", this.els.measurements),
      section("Parameters", this.els.params),
      section("Notes", this.els.notes),
      section("Image details", this.els.details),
    );
  }

  renderAll(doc: Doc | null): void {
    this.renderDisplay(doc);
    this.renderParams(doc);
    this.renderMeasurements(doc);
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
    const pal = palette(doc.palette);

    const select = h(
      "select",
      { id: "palette", onchange: (e: Event) => this.on.setPalette((e.target as HTMLSelectElement).value) },
      state.palettes.map((p) => h("option", { value: p.id, selected: p.id === pal.id }, p.name)),
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

    const views: [View, string][] = [
      ["thermal", "Thermal"],
      ["photo", "Photo"],
      ["blend", "Blend"],
    ];
    const viewButtons = h(
      "div",
      { class: "segmented", role: "group", "aria-label": "View" },
      views.map(([v, label]) =>
        h(
          "button",
          {
            class: state.view === v ? "active" : "",
            disabled: v !== "thermal" && !doc.photo,
            title: v !== "thermal" && !doc.photo ? "This file has no photo" : "",
            onclick: () => this.on.setView(v),
          },
          label,
        ),
      ),
    );

    fill(el, 
      h("div", { class: "row" }, h("label", { for: "palette" }, "Palette"), select),
      swatch,
      h("canvas", { class: "histogram", width: 280, height: 56, title: "Temperature distribution; the shaded band is the display range" }),
      h(
        "div",
        { class: "row range-row" },
        rangeInput(0),
        h("span", { class: "muted" }, "to"),
        rangeInput(1),
        h("span", { class: "unit" }, symbol(unit)),
      ),
      h(
        "div",
        { class: "row buttons" },
        h("button", { class: doc.range ? "" : "active", onclick: () => this.on.setRange(null), title: "Fit the range to the image" }, "Auto"),
        doc.info.embeddedRange
          ? h("button", { onclick: () => this.on.setRange(doc.info.embeddedRange), title: "The range the camera displayed" }, "Camera")
          : null,
        h("button", { onclick: () => this.on.setRange(narrowed(doc)), title: "Ignore the hottest and coldest 1 % of pixels" }, "Contrast"),
      ),
      h("div", { class: "row" }, h("span", { class: "label" }, "View"), viewButtons),
      state.view === "blend" && doc.photo
        ? h(
            "div",
            { class: "row" },
            h("label", { for: "blend" }, "Opacity"),
            h("input", {
              id: "blend",
              type: "range",
              min: "0",
              max: "1",
              step: "0.01",
              value: String(state.blend),
              oninput: (e: Event) => this.on.setBlend(Number((e.target as HTMLInputElement).value)),
            }),
          )
        : null,
      h(
        "div",
        { class: "row checks" },
        h(
          "label",
          {},
          h("input", { type: "checkbox", checked: state.smooth, onchange: (e: Event) => this.on.setSmooth((e.target as HTMLInputElement).checked) }),
          "Smooth",
        ),
        h(
          "label",
          {},
          h("input", {
            type: "checkbox",
            checked: state.showExtremes,
            onchange: (e: Event) => this.on.setShowExtremes((e.target as HTMLInputElement).checked),
          }),
          "Hot/cold spots",
        ),
      ),
    );
    this.drawHistogram(doc);
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
    const pal = palette(doc.palette).colors;
    const bw = w / bins.length;
    bins.forEach((n, i) => {
      const k = lo + ((i + 0.5) / bins.length) * span;
      const t = Math.min(255, Math.max(0, Math.floor(((k - r0) / (r1 - r0 || 1)) * 256)));
      const c = pal[t]!;
      ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
      const bh = Math.max(n > 0 ? 1 : 0, (Math.sqrt(n) / Math.sqrt(peak)) * (hh - 4));
      ctx.fillRect(i * bw + 0.5, hh - bh, bw - 1, bh);
    });
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
      fill(el, 
        h("p", { class: "muted" }, `${doc.info.formatLabel} files store final temperatures, so their parameters cannot be changed.`),
      );
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
    fill(el, 
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
    const existing = el.querySelectorAll("tbody tr");
    if (focused && existing.length === doc.measurements.length) {
      // Keep the label being edited; refresh numbers only.
      existing.forEach((tr, i) => {
        const s = doc.stats[i] ?? null;
        const cells = tr.querySelectorAll("td.n");
        cells[0]!.textContent = temp(s?.min, unit);
        cells[1]!.textContent = temp(s?.max, unit);
        cells[2]!.textContent = temp(s?.avg, unit);
        tr.classList.toggle("selected", state.selected === i);
      });
      return;
    }
    if (doc.measurements.length === 0) {
      fill(el, 
        h("p", { class: "muted" }, "Pick a tool in the toolbar (Spot, Box, Ellipse or Line) and draw on the image."),
        imageStats(doc, unit),
      );
      return;
    }
    const rows = doc.measurements.map((m, i) => {
      const s = doc.stats[i] ?? null;
      return h(
        "tr",
        { class: state.selected === i ? "selected" : "", onclick: () => this.on.select(i) },
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
    fill(el, 
      h(
        "table",
        { class: "measurements" },
        h("thead", {}, h("tr", {}, h("th", {}, "Label"), h("th", {}, "Min"), h("th", {}, "Max"), h("th", {}, "Avg"), h("th", {}))),
        h("tbody", {}, rows),
      ),
      doc.measurements.length >= 2 ? this.deltaT(doc) : null,
      imageStats(doc, unit),
    );
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

  // ------------------------------------------------------------------ notes, details

  renderNotes(doc: Doc | null): void {
    const el = this.els.notes;
    if (!doc) {
      fill(el);
      return;
    }
    if (el.contains(document.activeElement)) return;
    fill(el, 
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
    const rows: [string, string | null][] = [
      ["Camera", c.description],
      ["Captured", c.dateTime],
      ["Serial", c.serialNumber],
      ["GPS", c.latitude != null && c.longitude != null ? `${c.latitude.toFixed(6)}, ${c.longitude.toFixed(6)}` : null],
      ["Format", doc.info.formatLabel],
      ["Size", `${doc.info.width} × ${doc.info.height}${doc.photo ? ` · photo ${doc.photo.width} × ${doc.photo.height}` : ""}`],
      ["File", doc.info.path],
    ];
    fill(el, 
      h(
        "dl",
        { class: "details" },
        rows.filter(([, v]) => v).map(([k, v]) => [h("dt", {}, k), h("dd", { title: v ?? "" }, v ?? "")]).flat(),
      ),
    );
  }
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
  const stops = [0, 32, 64, 96, 128, 160, 192, 224, 255].map((i) => {
    const c = colors[i]!;
    return `rgb(${c[0]},${c[1]},${c[2]}) ${((i / 255) * 100).toFixed(1)}%`;
  });
  return `linear-gradient(90deg, ${stops.join(", ")})`;
}

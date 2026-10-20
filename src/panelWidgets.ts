import { type Distribution, type IsothermKind, type Param, type Range, type Unit } from "./api";
import { h } from "./dom";
import { type Doc, state } from "./state";
import { deltaFromKelvin, inputValue, symbol, temp, toKelvin } from "./units";

export const EMISSIVITY_PRESETS: [string, number][] = [
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

export interface ParamField {
  key: Param;
  label: string;
  kind: "temperature" | "ratio" | "percent" | "metres";
  min: number;
  max: number;
  step: number;
}

export const PARAM_FIELDS: ParamField[] = [
  { key: "emissivity", label: "Emissivity", kind: "ratio", min: 0.01, max: 1, step: 0.01 },
  { key: "reflectedK", label: "Reflected temp.", kind: "temperature", min: -100, max: 2000, step: 0.1 },
  { key: "distanceM", label: "Distance", kind: "metres", min: 0, max: 10000, step: 0.1 },
  { key: "humidity", label: "Rel. humidity", kind: "percent", min: 0, max: 100, step: 1 },
  { key: "atmosphereK", label: "Air temp.", kind: "temperature", min: -100, max: 200, step: 0.1 },
  { key: "windowK", label: "IR window temp.", kind: "temperature", min: -100, max: 500, step: 0.1 },
  { key: "windowTransmission", label: "IR window trans.", kind: "ratio", min: 0.01, max: 1, step: 0.01 },
];

export const DISTRIBUTIONS: [Distribution, string][] = [
  ["temperatureLinear", "Temperature linear"],
  ["signalLinear", "Signal linear"],
  ["histogram", "Histogram equalised"],
  ["detail", "Detail enhanced"],
];

export function fieldValue(f: ParamField, v: number | null, unit: Unit): string {
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

export function parseField(f: ParamField, text: string, unit: Unit): number | null {
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

export function unitLabel(f: ParamField, unit: Unit): string {
  return f.kind === "temperature" ? symbol(unit) : f.kind === "percent" ? "%" : f.kind === "metres" ? "m" : "";
}

export function num(text: string): number | null {
  const n = Number(text.replace(",", "."));
  return text.trim() === "" || !Number.isFinite(n) ? null : n;
}

/** A number input for a temperature (kelvin in, kelvin out) in the current unit. */
export function tempInput(k: number | null, onSet: (k: number | null) => void, label: string): HTMLInputElement {
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
export function deltaInput(dk: number, onSet: (dk: number) => void, label: string): HTMLInputElement {
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

export function section(id: string, title: string, body: Node, extra?: Node): HTMLElement {
  return h("section", { class: "panel-section", id: `section-${id}` }, h("header", {}, h("h2", {}, title), extra ?? null), body);
}

// Text must match crates/kelvinlens-core/src/report/prepare.rs isotherm_text (not shared; see sprint-outline 3.3).
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

export function kindName(kind: string): string {
  return ({ spot: "Spot", box: "Box", ellipse: "Ellipse", line: "Line", polygon: "Polygon", polyline: "Polyline", wand: "Magic wand" } as Record<string, string>)[kind] ?? kind;
}

export function coverageText(c: number | null | undefined): string {
  return c == null ? "" : `${(c * 100).toFixed(1)} %`;
}

export function alarmOn(doc: Doc, i: number): boolean {
  const m = doc.measurements[i];
  const s = doc.stats[i];
  if (!m?.alarm || !s) return false;
  const v = s[m.alarm.stat];
  return (m.alarm.above != null && v > m.alarm.above) || (m.alarm.below != null && v < m.alarm.below);
}

export function resultText(doc: Doc, i: number, unit: Unit): string {
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

export function imageStats(doc: Doc, unit: Unit): HTMLElement {
  const r = doc.dataRange;
  return h("p", { class: "muted small" }, r ? `Whole image: ${temp(r[0], unit)} to ${temp(r[1], unit)}` : "No valid temperatures with these parameters.");
}

/** Range that skips the hottest and coldest 1 % of pixels. */
export function narrowed(doc: Doc): Range | null {
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

// Application state. Rust owns the temperatures; this keeps what the UI shows and edits.
import type { ImageInfo, Measurement, Palette, Params, Range, Shape, ShapeKind, Stats, Unit } from "./api";

export type Tool = "select" | ShapeKind;
export type View = "thermal" | "photo" | "blend";

export interface Doc {
  info: ImageInfo;
  /** Current temperatures (kelvin), row-major; NaN where there is no solution. */
  kelvin: Float32Array;
  /** Finite min/max of `kelvin`. */
  dataRange: Range | null;
  photo: ImageBitmap | null;
  measurements: Measurement[];
  stats: (Stats | null)[];
  notes: string;
  palette: string;
  /** Display range; null means automatic (the data range). */
  range: Range | null;
  params: Params;
  /** Unsaved edits since opening or the last save. */
  dirty: boolean;
  includeInReport: boolean;
}

export const state = {
  docs: [] as Doc[],
  current: -1,
  unit: "C" as Unit,
  tool: "select" as Tool,
  view: "thermal" as View,
  blend: 0.65,
  smooth: true,
  showExtremes: true,
  selected: null as number | null,
  palettes: [] as Palette[],
  defaultPalette: "iron",
};

export function currentDoc(): Doc | null {
  return state.docs[state.current] ?? null;
}

export function displayRange(doc: Doc): Range {
  return doc.range ?? doc.dataRange ?? [273.15, 373.15];
}

export function palette(id: string): Palette {
  return state.palettes.find((p) => p.id === id) ?? state.palettes[0]!;
}

const PREFIX: Record<ShapeKind, string> = { spot: "Sp", box: "Bx", ellipse: "El", line: "Li" };

/** Next free label like Sp3 for the shape's kind. */
export function nextLabel(doc: Doc, kind: ShapeKind): string {
  const prefix = PREFIX[kind];
  const used = new Set(doc.measurements.map((m) => m.label));
  for (let i = 1; ; i++) if (!used.has(`${prefix}${i}`)) return `${prefix}${i}`;
}

export function finiteRange(data: Float32Array): Range | null {
  let lo = Infinity;
  let hi = -Infinity;
  for (const v of data) {
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  return Number.isFinite(lo) && Number.isFinite(hi) ? [lo, hi] : null;
}

export function shapeCopy(s: Shape): Shape {
  return { ...s };
}

// Settings that persist between sessions (best effort; storage can be unavailable).
const SETTINGS_KEY = "kelvinlens.settings.v1";

export interface Settings {
  unit: Unit;
  defaultPalette: string;
  smooth: boolean;
  showExtremes: boolean;
  inspector: string;
  paper: "a4" | "us-letter" | null;
}

export function loadSettings(): Partial<Settings> {
  try {
    return JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") as Partial<Settings>;
  } catch {
    return {};
  }
}

export function saveSettings(patch: Partial<Settings>): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ ...loadSettings(), ...patch }));
  } catch {
    // Not fatal: settings just won't persist.
  }
}

// Application state. Rust owns the temperatures; this keeps what the UI shows and edits.
import type {
  Annotation,
  Distribution,
  Finding,
  Formula,
  FormulaResult,
  ImageInfo,
  InspectSettings,
  Isotherm,
  Look,
  Measurement,
  Palette,
  Params,
  Presentation,
  Pt,
  Range,
  ShapeKind,
  Stats,
  Template,
  Unit,
} from "./api";

export type Tool = "select" | ShapeKind | "text" | "sketch" | "crop";
export type View = "thermal" | "photo" | "blend" | "msx" | "fusion";

export interface VideoState {
  frame: number;
  playing: boolean;
  /** High-sensitivity (gas) view: difference from the average of the previous frames. */
  hsm: boolean;
  hsmReference: number;
  /** Half-width of the HSM colour scale in kelvin. */
  hsmSpan: number;
  binning: boolean;
  hsmData: Float32Array | null;
  /** In and out frames for exports. */
  first: number;
  last: number;
}

export interface Doc {
  info: ImageInfo;
  /** Current temperatures (kelvin), row-major; NaN where there is no solution. */
  kelvin: Float32Array;
  /** Finite min/max of `kelvin`. */
  dataRange: Range | null;
  photo: ImageBitmap | null;
  measurements: Measurement[];
  stats: (Stats | null)[];
  /** Outline loops of magic-wand measurements (empty for other tools). */
  outlines: Pt[][][];
  /** Share of each area measurement inside the isotherms (null for none). */
  coverage: (number | null)[];
  notes: string;
  palette: string;
  /** Display range; null means automatic (the data range). */
  range: Range | null;
  distribution: Distribution;
  isotherms: Isotherm[];
  formulas: Formula[];
  formulaResults: FormulaResult[];
  annotations: Annotation[];
  findings: Finding[];
  asset: string;
  fields: Record<string, string>;
  presentation: Presentation;
  params: Params;
  /** Unsaved edits since opening or the last save. */
  dirty: boolean;
  includeInReport: boolean;
  video: VideoState | null;
}

export const DEFAULT_INSPECT: InspectSettings = {
  mode: "hot",
  referenceK: null,
  basis: "similar",
  minDelta: 3,
  minPixels: 4,
  maxFindings: 12,
  insideK: null,
  outsideK: null,
};

export const state = {
  docs: [] as Doc[],
  current: -1,
  unit: "C" as Unit,
  tool: "select" as Tool,
  view: "thermal" as View,
  blend: 0.65,
  msxStrength: 0.6,
  fusionRange: null as Range | null,
  smooth: true,
  showExtremes: true,
  selected: null as number | null,
  palettes: [] as Palette[],
  customPalettes: [] as Palette[],
  builtinTemplates: [] as Template[],
  customTemplates: [] as Template[],
  defaultPalette: "iron",
  wandTolerance: 1.0,
  sketchColor: [250, 204, 21] as [number, number, number],
  inspect: { ...DEFAULT_INSPECT } as InspectSettings,
};

export function currentDoc(): Doc | null {
  return state.docs[state.current] ?? null;
}

export function displayRange(doc: Doc): Range {
  return doc.range ?? doc.dataRange ?? [273.15, 373.15];
}

export const CAMERA_PALETTE = "camera";

/** Built-in, custom and (for this image) the camera's own palette. */
export function palettesFor(doc: Doc | null): Palette[] {
  const out = [...state.palettes, ...state.customPalettes];
  if (doc?.info.embeddedPalette) out.unshift({ id: CAMERA_PALETTE, name: "Camera palette", colors: doc.info.embeddedPalette });
  return out;
}

export function palette(id: string, doc: Doc | null = currentDoc()): Palette {
  return palettesFor(doc).find((p) => p.id === id) ?? state.palettes.find((p) => p.id === state.defaultPalette) ?? state.palettes[0]!;
}

export function lookOf(doc: Doc): Look {
  return { palette: palette(doc.palette, doc), range: displayRange(doc), distribution: doc.distribution, isotherms: doc.isotherms };
}

export function allTemplates(): Template[] {
  return [...state.builtinTemplates, ...state.customTemplates];
}

const PREFIX: Record<ShapeKind, string> = { spot: "Sp", box: "Bx", ellipse: "El", line: "Li", polygon: "Po", polyline: "Pl", wand: "Wa" };

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

export function isArea(kind: ShapeKind): boolean {
  return kind === "box" || kind === "ellipse" || kind === "polygon" || kind === "wand";
}

export function isPath(kind: ShapeKind): boolean {
  return kind === "line" || kind === "polyline";
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
  customPalettes: Palette[];
  customTemplates: Template[];
  templateId: string;
  reportFormat: "pdf" | "html";
  wandTolerance: number;
  msxStrength: number;
  inspect: InspectSettings;
  exportOverlay: ExportOptions;
}

export interface ExportOptions {
  format: "png" | "jpg";
  scale: number;
  colourBar: boolean;
  labels: boolean;
  fileName: boolean;
  dateTime: boolean;
  parameters: boolean;
  extremes: boolean;
  customText: string;
  logo: string | null;
  logoCorner: "top-left" | "top-right" | "bottom-left" | "bottom-right";
}

export const DEFAULT_EXPORT: ExportOptions = {
  format: "png",
  scale: 0,
  colourBar: true,
  labels: true,
  fileName: true,
  dateTime: false,
  parameters: false,
  extremes: false,
  customText: "",
  logo: null,
  logoCorner: "top-right",
};

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

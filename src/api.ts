// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { FUSE_HEADER_BYTES, parseFuseHeader } from "./fuseWire";

export type Kelvin = number;
export type Range = [Kelvin, Kelvin];
export type Unit = "C" | "F" | "K";
export type Pt = [number, number];

export interface Params {
  emissivity: number | null;
  reflectedK: number | null;
  atmosphereK: number | null;
  distanceM: number | null;
  humidity: number | null;
  windowK: number | null;
  windowTransmission: number | null;
}
export type Param = keyof Params;

export type Shape =
  | { kind: "spot"; x: number; y: number }
  | { kind: "box"; x: number; y: number; w: number; h: number }
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number }
  | { kind: "polygon"; points: Pt[] }
  | { kind: "polyline"; points: Pt[] }
  | { kind: "wand"; x: number; y: number; tolerance: number };
export type ShapeKind = Shape["kind"];

export type Stat = "min" | "max" | "avg";

export interface Alarm {
  stat: Stat;
  above: Kelvin | null;
  below: Kelvin | null;
}

export interface Measurement {
  label: string;
  shape: Shape;
  alarm?: Alarm | null;
}

export interface Stats {
  min: Kelvin;
  max: Kelvin;
  avg: Kelvin;
  stdDev: number;
  minAt: [number, number];
  maxAt: [number, number];
  count: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Distribution = "temperatureLinear" | "signalLinear" | "histogram" | "detail";

export type IsothermKind =
  | { kind: "above"; t: Kelvin }
  | { kind: "below"; t: Kelvin }
  | { kind: "interval"; lo: Kelvin; hi: Kelvin }
  | { kind: "condensation"; airK: Kelvin; rh: number; limit: number }
  | { kind: "insulation"; insideK: Kelvin; outsideK: Kelvin; index: number };

export type Isotherm = IsothermKind & { color: [number, number, number]; opacity: number };

export interface Palette {
  id: string;
  name: string;
  colors: [number, number, number][];
}

export interface Look {
  palette: Palette;
  range: Range;
  distribution: Distribution;
  isotherms: Isotherm[];
}

export type Fusion = { mode: "msx"; strength: number } | { mode: "interval"; lo: Kelvin; hi: Kelvin };

export type ResultKind = "temperature" | "difference" | "number";

export interface Formula {
  label: string;
  expression: string;
  kind: ResultKind;
}

export interface FormulaResult {
  value: number | null;
  error: string | null;
}

export type Annotation =
  | { kind: "text"; x: number; y: number; text: string }
  | { kind: "sketch"; points: Pt[]; color: [number, number, number] };

export interface Presentation {
  rotation: number;
  crop: [number, number, number, number] | null;
  /** Manual thermal-on-photo alignment: shift x, y (photo pixels) and scale. */
  align?: [number, number, number] | null;
}

export type SeverityLevel = 1 | 2 | 3 | 4;

export interface Severity {
  level: SeverityLevel;
  label: string;
  action: string;
}

export interface Finding {
  label: string;
  hot: boolean;
  area: Shape;
  pixels: number;
  extremeK: Kelvin;
  at: [number, number];
  referenceK: Kelvin;
  deltaK: number;
  thermalIndex: number | null;
  severity: Severity;
}

export type InspectMode = "hot" | "cold" | "both";
export type Basis = "similar" | "ambient";

export interface InspectSettings {
  mode: InspectMode;
  referenceK: Kelvin | null;
  basis: Basis;
  minDelta: number;
  minPixels: number;
  maxFindings: number;
  insideK: Kelvin | null;
  outsideK: Kelvin | null;
}

export interface Sidecar {
  version?: number;
  params: Params | null;
  measurements: Measurement[];
  notes: string;
  palette: string | null;
  range: Range | null;
  distribution?: Distribution | null;
  isotherms?: Isotherm[];
  formulas?: Formula[];
  annotations?: Annotation[];
  findings?: Finding[];
  asset?: string;
  fields?: Record<string, string>;
  presentation?: Presentation;
}

export interface Camera {
  description: string | null;
  serialNumber: string | null;
  dateTime: string | null;
  latitude: number | null;
  longitude: number | null;
  altitude: number | null;
  heading: number | null;
  lens: string | null;
  fieldOfView: number | null;
  focalLength: number | null;
  gimbal: [number, number, number] | null;
  relativeAltitude: number | null;
}

export interface VideoInfo {
  frames: number;
  width: number;
  height: number;
  compressed: boolean;
  times: number[];
  frameRate: number | null;
}

/** Rust `thermogram::Format`, as serde writes it. */
export type ImageFormat = "flir" | "fluke" | "hti" | "irg" | "infiray" | "png" | "tiff" | "video" | "panorama";

export interface ImageInfo {
  id: number;
  path: string;
  name: string;
  format: ImageFormat;
  formatLabel: string;
  width: number;
  height: number;
  params: Params;
  fileParams: Params;
  editable: Param[];
  camera: Camera;
  embeddedMeasurements: Measurement[];
  embeddedRange: Range | null;
  embeddedPalette: [number, number, number][] | null;
  range: Range | null;
  photoSize: [number, number] | null;
  pip: { source: Rect; destination: Rect } | null;
  flirJpeg: boolean;
  video: VideoInfo | null;
  sidecar: Sidecar | null;
  sidecarError: string | null;
}

export interface Recomputed {
  params: Params;
  range: Range | null;
}

export interface ProfilePoint {
  d: number;
  k: Kelvin;
}

export interface Composite {
  image: ImageData;
  /** Thermal source rectangle [x, y, w, h] and where it lands in composite pixels. */
  source: [number, number, number, number];
  destination: [number, number, number, number];
}

export type Paper = "a4" | "us-letter";
export type ReportFormat = "pdf" | "html";
export type ImageSection =
  | "isotherms" | "measurements" | "formulas" | "findings" | "electrical" | "building"
  | "hvac" | "profiles" | "fields" | "notes" | "parameters" | "details";
export type TemplateKind = "general" | "electrical" | "building" | "hvac";

export interface Template {
  id: string;
  name: string;
  kind: TemplateKind;
  company: string;
  accent: string;
  logo: string | null;
  coverFields: string[];
  imageFields: string[];
  sections: { summary: boolean; contents: boolean; findingsOverview: boolean; trends: boolean; signature: boolean };
  imageSections: ImageSection[];
  imagesPerPage: number;
  disclaimer: string;
  footer: string;
}

export interface Conditions {
  insideK: Kelvin | null;
  outsideK: Kelvin | null;
  humidity: number | null;
  ambientK: Kelvin | null;
  basis: Basis;
}

export interface ReportInfo {
  title: string;
  site: string;
  inspector: string;
  date: string;
  summary: string;
  paper: Paper;
  unit: Unit;
  includePhotos: boolean;
  template: Template;
  fields: Record<string, string>;
  conditions: Conditions;
  format: ReportFormat;
}

export interface ReportItem {
  id: number;
  look: Look;
  measurements: Measurement[];
  notes: string;
  formulas: Formula[];
  annotations: Annotation[];
  findings: Finding[];
  fields: Record<string, string>;
  asset: string;
  presentation: Presentation;
}

export interface TriageRow {
  path: string;
  name: string;
  error: string | null;
  minK: Kelvin | null;
  maxK: Kelvin | null;
  flagged: boolean;
  dateTime: string | null;
}

export interface StitchResult {
  info: ImageInfo;
  offsets: [number, number][];
  scores: number[];
}

export interface TriageSettings {
  aboveK: Kelvin | null;
  belowK: Kelvin | null;
}

export interface MapItem {
  id: number;
  description: string;
}

export type ErrorKind = "invalidFile" | "invalidInput" | "unsupported" | "notFound" | "io" | "internal";
/** What a rejected command call carries (Rust `CmdError`, src-tauri/src/cmd_error.rs). */
export interface CmdError {
  kind: ErrorKind;
  message: string;
}

export interface Series {
  rows: (Stats | null)[][];
  times: number[];
}

function rgbaImage(buf: ArrayBuffer, width: number, height: number, offset = 0): ImageData {
  return new ImageData(new Uint8ClampedArray(buf, offset, width * height * 4), width, height);
}

export const api = {
  openImage: (path: string) => invoke<ImageInfo>("open_image", { path }),
  closeImage: (id: number) => invoke<void>("close_image", { id }),
  async thermalData(id: number): Promise<Float32Array> {
    const buf = await invoke<ArrayBuffer>("thermal_data", { id });
    return new Float32Array(buf);
  },
  async photo(id: number): Promise<ImageBitmap> {
    const buf = await invoke<ArrayBuffer>("photo_jpeg", { id });
    return createImageBitmap(new Blob([buf], { type: "image/jpeg" }));
  },
  setParams: (id: number, params: Params) => invoke<Recomputed>("set_params", { id, params }),
  resetParams: (id: number) => invoke<Recomputed>("reset_params", { id }),
  measure: (id: number, shapes: Shape[]) => invoke<(Stats | null)[]>("measure", { id, shapes }),
  outlines: (id: number, shapes: Shape[]) => invoke<Pt[][][]>("outlines", { id, shapes }),
  profile: (id: number, shape: Shape) => invoke<ProfilePoint[]>("profile", { id, shape }),
  coverage: (id: number, shapes: Shape[], isotherms: Isotherm[]) => invoke<(number | null)[]>("coverage", { id, shapes, isotherms }),
  evaluateFormulas: (id: number, measurements: Measurement[], formulas: Formula[], unit: Unit) =>
    invoke<FormulaResult[]>("evaluate_formulas", { id, measurements, formulas, unit }),
  async render(id: number, look: Look, width: number, height: number): Promise<ImageData> {
    const buf = await invoke<ArrayBuffer>("render", { id, look });
    return rgbaImage(buf, width, height);
  },
  async fuse(id: number, look: Look, fusion: Fusion, maxWidth: number, align: [number, number, number] | null = null): Promise<Composite> {
    const buf = await invoke<ArrayBuffer>("fuse", { id, look, fusion, maxWidth, align });
    const head = parseFuseHeader(buf);
    return { image: rgbaImage(buf, head.width, head.height, FUSE_HEADER_BYTES), source: head.source, destination: head.destination };
  },
  inspect: (id: number, settings: InspectSettings) => invoke<Finding[]>("inspect", { id, settings }),
  triage: (paths: string[], settings: TriageSettings) => invoke<TriageRow[]>("triage", { paths, settings }),
  stitch: (ids: number[], offsets: ([number, number] | null)[]) => invoke<StitchResult>("stitch", { ids, offsets }),
  exportThermalTiff: (id: number, path: string) => invoke<void>("export_thermal_tiff", { id, path }),
  saveFlirCopy: (id: number, path: string) => invoke<void>("save_flir_copy", { id, path }),
  selectFrame: (id: number, index: number) => invoke<Recomputed>("select_frame", { id, index }),
  videoSeries: (id: number, first: number, last: number, measurements: Measurement[]) =>
    invoke<Series>("video_series", { id, first, last, measurements }),
  async hsm(id: number, index: number, reference: number, binning: boolean): Promise<Float32Array> {
    const buf = await invoke<ArrayBuffer>("hsm", { id, index, reference, binning });
    return new Float32Array(buf);
  },
  exportSegment: (id: number, first: number, last: number, path: string) => invoke<number>("export_segment", { id, first, last, path }),
  aviReset: () => invoke<void>("avi_reset"),
  aviAddFrame: (jpeg: Uint8Array) => invoke<number>("avi_add_frame", jpeg),
  aviFinish: (path: string, width: number, height: number, fps: number) => invoke<number>("avi_finish", { path, width, height, fps }),
  listFolder: (path: string) => invoke<string[]>("list_folder", { path }),
  isFolder: (path: string) => invoke<boolean>("is_folder", { path }),
  palettes: () => invoke<Palette[]>("palettes"),
  templates: () => invoke<Template[]>("templates"),
  exportCsv: (id: number, path: string, unit: Unit) => invoke<void>("export_csv", { id, path, unit }),
  async saveBytes(path: string, bytes: Uint8Array): Promise<void> {
    await invoke<number>("stage_bytes", bytes);
    await invoke<void>("write_staged", { path });
  },
  writeText: (path: string, text: string) => invoke<void>("write_text", { path, text }),
  readText: (path: string) => invoke<string>("read_text", { path }),
  readImageDataUrl: (path: string) => invoke<string>("read_image_data_url", { path }),
  exportMap: (items: MapItem[], title: string, path: string) =>
    invoke<number>("export_map", { items, title, path }),
  saveSidecar: (id: number, edits: Sidecar) => invoke<string>("save_sidecar", { id, edits }),
  makeReport: (info: ReportInfo, items: ReportItem[], path: string) => invoke<void>("make_report", { info, items, path }),
  initialPaths: () => invoke<string[]>("initial_paths"),
};

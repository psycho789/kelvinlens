// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export type Kelvin = number;
export type Range = [Kelvin, Kelvin];
export type Unit = "C" | "F" | "K";

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
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number };
export type ShapeKind = Shape["kind"];

export interface Measurement {
  label: string;
  shape: Shape;
}

export interface Stats {
  min: Kelvin;
  max: Kelvin;
  avg: Kelvin;
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

export interface Sidecar {
  version?: number;
  params: Params | null;
  measurements: Measurement[];
  notes: string;
  palette: string | null;
  range: Range | null;
}

export interface ImageInfo {
  id: number;
  path: string;
  name: string;
  format: string;
  formatLabel: string;
  width: number;
  height: number;
  params: Params;
  fileParams: Params;
  editable: Param[];
  camera: {
    description: string | null;
    serialNumber: string | null;
    dateTime: string | null;
    latitude: number | null;
    longitude: number | null;
  };
  embeddedMeasurements: Measurement[];
  embeddedRange: Range | null;
  range: Range | null;
  photoSize: [number, number] | null;
  pip: { source: Rect; destination: Rect } | null;
  sidecar: Sidecar | null;
  sidecarError: string | null;
}

export interface Palette {
  id: string;
  name: string;
  colors: [number, number, number][];
}

export interface Recomputed {
  params: Params;
  range: Range | null;
}

export type Paper = "a4" | "us-letter";

export interface ReportInfo {
  title: string;
  site: string;
  inspector: string;
  date: string;
  summary: string;
  paper: Paper;
  unit: Unit;
  includePhotos: boolean;
}

export interface ReportItem {
  id: number;
  palette: string;
  range: Range;
  measurements: Measurement[];
  notes: string;
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
  listFolder: (path: string) => invoke<string[]>("list_folder", { path }),
  isFolder: (path: string) => invoke<boolean>("is_folder", { path }),
  palettes: () => invoke<Palette[]>("palettes"),
  exportCsv: (id: number, path: string, unit: Unit) => invoke<void>("export_csv", { id, path, unit }),
  async saveBytes(path: string, bytes: Uint8Array): Promise<void> {
    await invoke<number>("stage_bytes", bytes);
    await invoke<void>("write_staged", { path });
  },
  saveSidecar: (id: number, edits: Sidecar) => invoke<string>("save_sidecar", { id, edits }),
  makeReport: (info: ReportInfo, items: ReportItem[], path: string) =>
    invoke<void>("make_report", { info, items, path }),
  initialPaths: () => invoke<string[]>("initial_paths"),
};

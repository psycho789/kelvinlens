import { refreshAnalysis } from "./analysis";
import { type Params, api } from "./api";
import { selectDoc } from "./docs";
import { errorText, h, toast } from "./dom";
import { encode, exportOptions, renderExport } from "./exportImage";
import { modal } from "./modal";
import { applyParams } from "./params";
import { basename, siblingPath, stem } from "./paths";
import { pickers } from "./pickers";
import { type SettingsFile, parseSettingsFile } from "./settingsFile";
import { type Doc, currentDoc, state } from "./state";
import { markDirty, renderFiles, updateToolbar } from "./status";
import { symbol } from "./units";

// ---------------------------------------------------------------------------- saving & exporting

export function sidecarOf(doc: Doc) {
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

export async function saveDoc(doc: Doc): Promise<boolean> {
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

export async function saveEdits(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  if (await saveDoc(doc)) toast(`Saved edits to ${doc.info.name}.kelvinlens.json`);
}

export async function exportImage(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const opts = await exportOptions();
  if (!opts) return;
  const ext = opts.format === "png" ? "png" : "jpg";
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-thermal.${ext}`), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  try {
    const canvas = await renderExport(doc, state.unit, state.smooth, opts);
    await api.saveBytes(path, await encode(canvas, opts.format));
    toast(`Exported ${basename(path)} (${canvas.width} × ${canvas.height})`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

export async function exportCsv(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-temperatures-${state.unit}.csv`), filters: [{ name: "CSV", extensions: ["csv"] }] });
  if (!path) return;
  try {
    await api.exportCsv(doc.info.id, path, state.unit);
    toast(`Exported ${basename(path)} (${symbol(state.unit)}, one row per image row)`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

export async function exportTiff(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-radiometric.tiff`), filters: [{ name: "Radiometric TIFF (kelvin)", extensions: ["tiff", "tif"] }] });
  if (!path) return;
  try {
    await api.exportThermalTiff(doc.info.id, path);
    toast(`Saved ${basename(path)}: 32-bit temperatures in kelvin, opens in Kelvinlens and Blackbody`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}

export async function exportFlirCopy(): Promise<void> {
  const doc = currentDoc();
  if (!doc?.info.flirJpeg) return;
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-kelvinlens.jpg`), filters: [{ name: "FLIR radiometric JPEG", extensions: ["jpg"] }] });
  if (!path) return;
  try {
    await api.saveFlirCopy(doc.info.id, path);
    toast(`Saved ${basename(path)} with the current parameters written into it`);
  } catch (e) {
    toast(`Export failed: ${errorText(e)}`, "error");
  }
}


export async function exportSettings(): Promise<void> {
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
  const path = await pickers.save({ defaultPath: siblingPath(doc.info.path, `${stem(doc.info.name)}-settings.json`), filters: [{ name: "Kelvinlens settings", extensions: ["json"] }] });
  if (!path) return;
  try {
    await api.writeText(path, JSON.stringify(data, null, 2) + "\n");
    toast(`Saved ${basename(path)}`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

export async function importSettings(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  const path = await pickers.open({ filters: [{ name: "Kelvinlens settings", extensions: ["json"] }] });
  if (typeof path !== "string") return;
  let data: Partial<SettingsFile>;
  try {
    data = parseSettingsFile(await api.readText(path));
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


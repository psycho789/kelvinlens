import { refreshAnalysis } from "./analysis";
import { type ImageInfo, api } from "./api";
import { $, errorText, toast } from "./dom";
import { basename } from "./paths";
import { pickers } from "./pickers";
import { type Doc, currentDoc, finiteRange, nextLabel, palette, state } from "./state";
import { renderFiles, setBusy, showReadout, updateToolbar, updateZoomLabel } from "./status";
import { panel, refreshView, viewer } from "./ui";

// ---------------------------------------------------------------------------- opening files

export async function openPaths(paths: string[]): Promise<void> {
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

export async function loadDoc(info: ImageInfo): Promise<Doc> {
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

export function selectDoc(index: number): void {
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

export async function closeDoc(index: number): Promise<void> {
  const doc = state.docs[index];
  if (!doc) return;
  if (doc.dirty && !(await pickers.ask(`Close ${doc.info.name} without saving your edits?`, { title: "Unsaved edits", kind: "warning" }))) return;
  if (doc.video) doc.video.playing = false;
  state.docs.splice(index, 1);
  void api.closeImage(doc.info.id);
  const next = state.current > index ? state.current - 1 : Math.min(state.current, state.docs.length - 1);
  selectDoc(next);
}

export async function pickFiles(): Promise<void> {
  const picked = await pickers.open({
    multiple: true,
    filters: [
      { name: "Thermal images and videos", extensions: ["jpg", "jpeg", "JPG", "is2", "IS2", "irg", "IRG", "tif", "tiff", "png", "seq", "SEQ", "csq", "CSQ"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  if (picked) await openPaths(Array.isArray(picked) ? picked : [picked]);
}

export async function pickFolder(): Promise<void> {
  const picked = await pickers.open({ directory: true });
  if (typeof picked === "string") await openPaths([picked]);
}


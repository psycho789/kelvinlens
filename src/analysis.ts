import { panoramaDialog, triageDialog } from "./analysisDialogs";
import { api } from "./api";
import { loadDoc, openPaths, selectDoc } from "./docs";
import { errorText, perFrame, toast } from "./dom";
import { alarmOn } from "./panel";
import { type Doc, currentDoc, isArea, saveSettings, state } from "./state";
import { markDirty, renderFiles, setBusy } from "./status";
import { panel, viewer } from "./ui";

// ---------------------------------------------------------------------------- analysis

/** Measurement statistics, wand outlines, isotherm coverage and formulas for the doc. */
export async function refreshAnalysis(doc: Doc, render = true): Promise<void> {
  const id = doc.info.id;
  const shapes = doc.measurements.map((m) => m.shape);
  try {
    const [stats, outlines, coverage, formulas] = await Promise.all([
      api.measure(id, shapes),
      shapes.some((s) => s.kind === "wand") ? api.outlines(id, shapes) : Promise.resolve(shapes.map(() => [])),
      doc.isotherms.length && shapes.some((s) => isArea(s.kind)) ? api.coverage(id, shapes, doc.isotherms) : Promise.resolve(shapes.map(() => null)),
      doc.formulas.length ? api.evaluateFormulas(id, doc.measurements, doc.formulas, state.unit) : Promise.resolve([]),
    ]);
    doc.stats = stats;
    doc.outlines = outlines;
    doc.coverage = coverage;
    doc.formulaResults = formulas;
  } catch (e) {
    toast(errorText(e), "error");
  }
  if (!render) return;
  if (doc === currentDoc()) {
    updateAlarms(doc);
    panel.renderMeasurements(doc);
    panel.renderFormulas(doc);
  }
  renderFiles();
}

export const measureSoon = perFrame((doc: Doc) => void refreshAnalysis(doc));

export function updateAlarms(doc: Doc): void {
  viewer.alarms = doc.measurements.map((_, i) => alarmOn(doc, i));
  viewer.draw();
}

export async function runInspect(): Promise<void> {
  const doc = currentDoc();
  if (!doc) return;
  try {
    doc.findings = await api.inspect(doc.info.id, state.inspect);
    saveSettings({ inspect: state.inspect });
    markDirty(doc);
    viewer.draw();
    panel.renderInspect(doc);
    toast(doc.findings.length ? `${doc.findings.length} finding${doc.findings.length > 1 ? "s" : ""}` : "No anomalies at these settings.");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---------------------------------------------------------------------------- measurements

export function deleteMeasurement(index: number): void {
  const doc = currentDoc();
  if (!doc || !doc.measurements[index]) return;
  doc.measurements.splice(index, 1);
  doc.stats.splice(index, 1);
  doc.outlines.splice(index, 1);
  doc.coverage.splice(index, 1);
  state.selected = null;
  viewer.selected = null;
  markDirty(doc);
  viewer.draw();
  void refreshAnalysis(doc);
}

export async function triage(): Promise<void> {
  const paths = await triageDialog();
  if (paths.length) await openPaths(paths);
}

export async function panorama(): Promise<void> {
  const req = await panoramaDialog();
  if (!req) return;
  setBusy("Stitching…");
  try {
    const r = await api.stitch(req.ids, req.offsets);
    const doc = await loadDoc(r.info);
    doc.dirty = true;
    state.docs.push(doc);
    selectDoc(state.docs.length - 1);
    const weak = r.scores.slice(1).some((s) => Number.isFinite(s) && s < 0.8);
    toast(
      `Panorama ${r.info.width} × ${r.info.height} from ${req.ids.length} images (offsets ${r.offsets.map((o) => `${o[0]},${o[1]}`).join(" · ")}). ${weak ? "Some images matched weakly; check the seams or place them by hand. " : ""}Save it with Export ▸ Radiometric TIFF.`,
      weak ? "error" : "info",
    );
  } catch (e) {
    toast(`Panorama failed: ${errorText(e)}`, "error");
  } finally {
    setBusy("");
  }
}


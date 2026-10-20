import { refreshAnalysis } from "./analysis";
import { type Params, api } from "./api";
import { errorText, toast } from "./dom";
import { type Doc, currentDoc, finiteRange, state } from "./state";
import { markDirty } from "./status";
import { panel, refreshView, viewer } from "./ui";
import { updateHsm } from "./video";

// ---------------------------------------------------------------------------- parameters

export async function reloadTemperatures(doc: Doc, params: Params): Promise<void> {
  doc.params = params;
  doc.kelvin = await api.thermalData(doc.info.id);
  doc.dataRange = finiteRange(doc.kelvin);
  await refreshAnalysis(doc, false);
  if (doc === currentDoc()) {
    viewer.dataChanged();
    refreshView();
    panel.renderDisplay(doc);
    panel.renderParams(doc);
    panel.renderMeasurements(doc);
    panel.renderFormulas(doc);
  }
}

export async function applyParams(doc: Doc, params: Params): Promise<void> {
  try {
    const r = await api.setParams(doc.info.id, params);
    markDirty(doc);
    await reloadTemperatures(doc, r.params);
    if (doc.video?.hsm) await updateHsm(doc);
  } catch (e) {
    toast(errorText(e), "error");
    panel.renderParams(doc);
  }
}

export async function resetParams(doc: Doc): Promise<void> {
  try {
    const r = await api.resetParams(doc.info.id);
    markDirty(doc);
    await reloadTemperatures(doc, r.params);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

export async function applyParamsToAll(): Promise<void> {
  const source = currentDoc();
  if (!source) return;
  let applied = 0;
  for (const doc of state.docs) {
    if (doc === source || doc.info.editable.length === 0) continue;
    const params: Params = { ...doc.params };
    for (const key of doc.info.editable) if (source.info.editable.includes(key)) params[key] = source.params[key];
    try {
      const r = await api.setParams(doc.info.id, params);
      markDirty(doc);
      await reloadTemperatures(doc, r.params);
      applied++;
    } catch (e) {
      toast(`${doc.info.name}: ${errorText(e)}`, "error");
    }
  }
  toast(applied ? `Applied to ${applied} other image${applied > 1 ? "s" : ""}.` : "No other image supports these parameters.");
}


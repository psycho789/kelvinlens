import { api } from "./api";
import { errorText, toast } from "./dom";
import { basename, siblingPath, stem } from "./paths";
import { pickers } from "./pickers";
import { rapidInfo, reportDialog } from "./reportDialog";
import { type Doc, currentDoc, lookOf, state } from "./state";
import { setBusy } from "./status";
import { templateEditor } from "./templateEditor";

export function reportItems(docs: Doc[]) {
  return docs.map((d) => ({
    id: d.info.id,
    look: lookOf(d),
    measurements: d.measurements,
    notes: d.notes,
    formulas: d.formulas,
    annotations: d.annotations,
    findings: d.findings,
    fields: d.fields,
    asset: d.asset,
    presentation: d.presentation,
  }));
}

export async function createReport(): Promise<void> {
  if (!currentDoc()) return;
  const choice = await reportDialog(templateEditor);
  if (!choice) return;
  if (choice.docs.length === 0) {
    toast("Select at least one image for the report.", "error");
    return;
  }
  const first = choice.docs[0]!;
  const ext = choice.info.format === "html" ? "html" : "pdf";
  const name = (choice.info.title || "report").replace(/[\\/:*?"<>|]+/g, "-");
  const path = await pickers.save({ defaultPath: siblingPath(first.info.path, `${name}.${ext}`), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  await writeReport(choice.info, choice.docs, path);
}

export async function writeReport(info: Parameters<typeof api.makeReport>[0], docs: Doc[], path: string): Promise<void> {
  setBusy("Creating report…");
  try {
    await api.makeReport(info, reportItems(docs), path);
    toast(`Report saved: ${basename(path)}`);
  } catch (e) {
    toast(`Report failed: ${errorText(e)}`, "error");
  } finally {
    setBusy("");
  }
}

/** One click: the last report settings and template, every open image, saved next to the first. */
export async function rapidReport(): Promise<void> {
  const docs = state.docs.filter((d) => d.includeInReport);
  if (docs.length === 0) return;
  const info = rapidInfo();
  const ext = info.format === "html" ? "html" : "pdf";
  const first = docs[0]!;
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
  const path = siblingPath(first.info.path, `${stem(first.info.name)}-report-${stamp}.${ext}`);
  await writeReport(info, docs, path);
}


// Batch processing: apply display settings, parameters and measurements from the current image to
// many images, and write outputs for all of them in one go.
import { pickers } from "./pickers";
import { api, type Distribution, type Params } from "./api";
import { errorText, h, toast } from "./dom";
import { encode, renderExport } from "./exportImage";
import { field, modal, progress } from "./modal";
import { DEFAULT_EXPORT, displayRange, loadSettings, palettesFor, state, type Doc } from "./state";
import { fromKelvin, symbol, temp } from "./units";

export interface BatchContext {
  applyParams(doc: Doc, params: Params): Promise<void>;
  /** Re-measure and re-evaluate after measurements or isotherms change. */
  refreshAnalysis(doc: Doc): Promise<void>;
  saveEdits(doc: Doc): Promise<boolean>;
  /** Writes a video's segment as AVI; returns the frames written. */
  writeVideo(doc: Doc, path: string): Promise<number>;
  markDirty(doc: Doc): void;
  redraw(): void;
}

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

function stem(doc: Doc): string {
  return doc.info.name.replace(/\.[^.]+$/, "");
}

export async function batchDialog(ctx: BatchContext): Promise<void> {
  const source = state.docs[state.current];
  if (!source || state.docs.length < 1) return;
  const opt = {
    palette: "keep",
    distribution: "keep",
    range: "keep",
    isotherms: "keep",
    rotation: "keep",
    params: "keep",
    measurements: "keep",
    formulas: "keep",
    annotations: "keep",
    notes: "",
    asset: "",
    out: { save: true, png: false, jpg: false, csv: false, flir: false, tiff: false, avi: false, kml: false, table: false },
  };
  const sel = (key: keyof typeof opt, options: [string, string][]) =>
    h("select", { onchange: (e: Event) => ((opt as Record<string, unknown>)[key] = (e.target as HTMLSelectElement).value) }, options.map(([v, l]) => h("option", { value: v }, l)));
  const boxes = state.docs.map((d) => ({ d, box: h("input", { type: "checkbox", checked: true }) }));
  const out = (key: keyof typeof opt.out, label: string, title = "") =>
    h("label", { class: "check", title }, h("input", { type: "checkbox", checked: opt.out[key], onchange: (e: Event) => (opt.out[key] = (e.target as HTMLInputElement).checked) }), label);
  const palettes = palettesFor(source).filter((p) => p.id !== "camera");

  const choice = await modal(
    "Batch processing",
    [
      h("p", { class: "muted small" }, `Settings are taken from the current image, ${source.info.name}.`),
      h("fieldset", {}, h("legend", {}, "Images"), boxes.map(({ d, box }) => h("label", { class: "check" }, box, d.info.name))),
      h(
        "div",
        { class: "grid2" },
        field("Palette", sel("palette", [["keep", "Keep each image's"], ["copy", "Same as current"], ...palettes.map((p) => [`id:${p.id}`, p.name] as [string, string])])),
        field("Colour distribution", sel("distribution", [["keep", "Keep"], ["copy", "Same as current"], ["temperatureLinear", "Temperature linear"], ["signalLinear", "Signal linear"], ["histogram", "Histogram equalised"], ["detail", "Detail enhanced"]])),
        field("Scale (range)", sel("range", [["keep", "Keep"], ["copy", `Same as current (${temp(displayRange(source)[0], state.unit)} – ${temp(displayRange(source)[1], state.unit)})`], ["auto", "Automatic per image"]])),
        field("Isotherms and alarms", sel("isotherms", [["keep", "Keep"], ["copy", "Copy from current"], ["clear", "Clear"]])),
        field("Rotation and crop", sel("rotation", [["keep", "Keep"], ["copy", "Copy from current"], ["clear", "Reset"]])),
        field("Parameters", sel("params", [["keep", "Keep"], ["all", "All parameters from current"], ["emissivity", "Emissivity only"], ["distance", "Emissivity and distance"], ["reset", "Reset to each file's"]])),
        field("Measurements", sel("measurements", [["keep", "Keep"], ["replace", "Replace with current's"], ["add", "Add current's"], ["clear", "Clear all"]])),
        field("Formulas", sel("formulas", [["keep", "Keep"], ["copy", "Copy from current"]])),
        field("Notes and sketches on the image", sel("annotations", [["keep", "Keep"], ["add", "Add current's"], ["clear", "Clear"]])),
        field("Asset tag", h("input", { type: "text", placeholder: "Leave empty to keep", oninput: (e: Event) => (opt.asset = (e.target as HTMLInputElement).value) })),
        field("Add to notes", h("input", { type: "text", placeholder: "Text appended to each image's notes", oninput: (e: Event) => (opt.notes = (e.target as HTMLInputElement).value) })),
      ),
      h(
        "fieldset",
        { class: "plain" },
        h("legend", {}, "Outputs"),
        h(
          "div",
          { class: "checks wrap" },
          out("save", "Save edits"),
          out("png", "PNG images"),
          out("jpg", "JPEG images"),
          out("csv", `Temperature CSV (${symbol(state.unit)})`),
          out("flir", "Radiometric JPEG copies (FLIR)", "A copy of each FLIR file with the new parameters written in"),
          out("tiff", "Radiometric TIFF"),
          out("avi", "AVI (videos)", "Each video's segment with palette and measurements"),
          out("kml", "Map (KML)"),
          out("table", "Measurement table (CSV)"),
        ),
      ),
    ],
    [
      { label: "Cancel", value: "cancel" },
      { label: "Run…", value: "run", primary: true },
    ],
    { wide: true },
  );
  if (choice !== "run") return;
  const docs = boxes.filter((b) => b.box.checked).map((b) => b.d);
  if (docs.length === 0) return;
  const needsFolder = Object.entries(opt.out).some(([k, on]) => on && k !== "save");
  let folder: string | null = null;
  if (needsFolder) {
    const picked = await pickers.open({ directory: true, title: "Folder for the outputs" });
    if (typeof picked !== "string") return;
    folder = picked;
  }

  const p = progress("Batch processing");
  const failures: string[] = [];
  let written = 0;
  const exportOpts = { ...DEFAULT_EXPORT, ...(loadSettings().exportOverlay ?? {}) };
  for (const [n, doc] of docs.entries()) {
    if (p.cancelled()) break;
    p.set(n, docs.length, doc.info.name);
    try {
      if (doc !== source) {
        if (opt.palette === "copy") doc.palette = source.palette;
        else if (opt.palette.startsWith("id:")) doc.palette = opt.palette.slice(3);
        if (opt.distribution === "copy") doc.distribution = source.distribution;
        else if (opt.distribution !== "keep") doc.distribution = opt.distribution as Distribution;
        if (opt.range === "copy") doc.range = [...displayRange(source)];
        if (opt.isotherms === "copy") doc.isotherms = structuredClone(source.isotherms);
        if (opt.rotation === "copy") doc.presentation = structuredClone(source.presentation);
        if (opt.measurements === "replace") doc.measurements = structuredClone(source.measurements);
        if (opt.measurements === "add") doc.measurements.push(...structuredClone(source.measurements).filter((m) => !doc.measurements.some((x) => x.label === m.label)));
        if (opt.formulas === "copy") doc.formulas = structuredClone(source.formulas);
        if (opt.annotations === "add") doc.annotations.push(...structuredClone(source.annotations));
      } else {
        // Explicit choices apply to the current image too.
        if (opt.palette.startsWith("id:")) doc.palette = opt.palette.slice(3);
        if (!["keep", "copy"].includes(opt.distribution)) doc.distribution = opt.distribution as Distribution;
      }
      if (opt.range === "auto") doc.range = null;
      if (opt.isotherms === "clear") doc.isotherms = [];
      if (opt.rotation === "clear") doc.presentation = { rotation: 0, crop: null };
      if (opt.measurements === "clear") doc.measurements = [];
      if (opt.annotations === "clear") doc.annotations = [];
      if (opt.asset.trim()) doc.asset = opt.asset.trim();
      if (opt.notes.trim()) doc.notes = doc.notes ? `${doc.notes}\n${opt.notes.trim()}` : opt.notes.trim();
      // Crop rectangles beyond a smaller image make no sense there.
      const c = doc.presentation.crop;
      if (c && (c[0] + c[2] > doc.info.width || c[1] + c[3] > doc.info.height)) doc.presentation = { ...doc.presentation, crop: null };

      if (opt.params !== "keep" && doc.info.editable.length > 0) {
        if (opt.params === "reset") {
          await ctx.applyParams(doc, doc.info.fileParams);
        } else if (doc !== source) {
          const keys = opt.params === "all" ? source.info.editable : opt.params === "emissivity" ? ["emissivity" as const] : ["emissivity" as const, "distanceM" as const];
          const next: Params = { ...doc.params };
          for (const k of keys) if (doc.info.editable.includes(k) && source.params[k] != null) next[k] = source.params[k];
          await ctx.applyParams(doc, next);
        }
      }
      ctx.markDirty(doc);
      await ctx.refreshAnalysis(doc);

      if (opt.out.save && doc.info.format !== "panorama") {
        if (!(await ctx.saveEdits(doc))) failures.push(`${doc.info.name} (edits not saved)`);
      }
      if (folder) {
        const base = join(folder, stem(doc));
        if (opt.out.png || opt.out.jpg) {
          const canvas = await renderExport(doc, state.unit, state.smooth, exportOpts);
          if (opt.out.png) {
            await api.saveBytes(`${base}-thermal.png`, await encode(canvas, "png"));
            written++;
          }
          if (opt.out.jpg) {
            await api.saveBytes(`${base}-thermal.jpg`, await encode(canvas, "jpg"));
            written++;
          }
        }
        if (opt.out.csv) {
          await api.exportCsv(doc.info.id, `${base}-temperatures-${state.unit}.csv`, state.unit);
          written++;
        }
        if (opt.out.flir && doc.info.flirJpeg) {
          await api.saveFlirCopy(doc.info.id, `${base}-kelvinlens.jpg`);
          written++;
        }
        if (opt.out.tiff) {
          await api.exportThermalTiff(doc.info.id, `${base}-radiometric.tiff`);
          written++;
        }
        if (opt.out.avi && doc.video) {
          p.set(n, docs.length, `${doc.info.name}: rendering video…`);
          if ((await ctx.writeVideo(doc, `${base}.avi`)) > 0) written++;
        }
      }
    } catch (e) {
      failures.push(`${doc.info.name} (${errorText(e)})`);
    }
  }
  if (folder && !p.cancelled()) {
    try {
      if (opt.out.kml) {
        const withGps = docs.filter((d) => d.info.camera.latitude != null);
        if (withGps.length) {
          await api.exportMap(withGps.map((d) => ({ id: d.info.id, description: d.dataRange ? `max ${temp(d.dataRange[1], state.unit)}` : "" })), "Thermal images", join(folder, "thermal-images.kml"));
          written++;
        } else failures.push("map (no image has a GPS position)");
      }
      if (opt.out.table) {
        const labels = [...new Set(docs.flatMap((d) => d.measurements.map((m) => m.label)))];
        const cell = (s: string) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
        let text = ["image", "captured", "asset", ...labels.flatMap((l) => [`${l} max`, `${l} min`, `${l} avg`])].map(cell).join(",") + "\n";
        for (const d of docs) {
          const cells = [cell(d.info.name), cell(d.info.camera.dateTime ?? ""), cell(d.asset)];
          for (const l of labels) {
            const i = d.measurements.findIndex((m) => m.label === l);
            const s = i >= 0 ? d.stats[i] : null;
            cells.push(...(s ? [s.max, s.min, s.avg].map((k) => fromKelvin(k, state.unit).toFixed(2)) : ["", "", ""]));
          }
          text += cells.join(",") + "\n";
        }
        await api.writeText(join(folder, `measurements-${state.unit}.csv`), text);
        written++;
      }
    } catch (e) {
      failures.push(errorText(e));
    }
  }
  p.close();
  ctx.redraw();
  const done = `Batch finished: ${docs.length} image${docs.length > 1 ? "s" : ""}${written ? `, ${written} file${written > 1 ? "s" : ""} written` : ""}.`;
  if (failures.length) toast(`${done} Problems: ${failures.slice(0, 3).join("; ")}${failures.length > 3 ? " …" : ""}`, "error");
  else toast(done);
}

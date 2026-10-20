import { refreshAnalysis } from "./analysis";
import { type Unit } from "./api";
import { closeDoc, selectDoc } from "./docs";
import { $, h } from "./dom";

import { type Doc, type Tool, currentDoc, displayRange, palette, saveSettings, state } from "./state";
import { panel, viewer } from "./ui";
import { temp } from "./units";
import { viewerLabel } from "./a11y";

// ---------------------------------------------------------------------------- readout & status

export function showReadout(pos: [number, number] | null): void {
  const el = $("#readout");
  const doc = currentDoc();
  if (!doc || !pos) {
    el.textContent = "";
    return;
  }
  const x = Math.floor(pos[0]);
  const y = Math.floor(pos[1]);
  const i = y * doc.info.width + x;
  if (doc.video?.hsm && doc.video.hsmData) {
    const d = doc.video.hsmData[i]!;
    el.textContent = `Δ ${Number.isFinite(d) ? d.toFixed(3) : "–"} K   x ${x}  y ${y}`;
    return;
  }
  el.textContent = `${temp(doc.kelvin[i], state.unit, 2)}   x ${x}  y ${y}`;
}

export function setBusy(text: string): void {
  $("#busy").textContent = text;
  document.body.classList.toggle("busy", text !== "");
}

export function updateZoomLabel(): void {
  $("#zoom-fit").textContent = currentDoc() ? `${viewer.zoomPercent()}%` : "100%";
}

export function markDirty(doc: Doc): void {
  if (!doc.dirty) {
    doc.dirty = true;
    renderFiles();
    updateToolbar();
  }
}

export function renderFiles(): void {
  const list = $("#files");
  $("#count").textContent = state.docs.length ? String(state.docs.length) : "";
  list.replaceChildren(
    ...state.docs.map((doc, i) =>
      h(
        "li",
        { class: i === state.current ? "active" : "", title: doc.info.path, onclick: () => selectDoc(i) },
        h("canvas", { class: "thumb", width: 48, height: 36 }),
        h(
          "div",
          { class: "file-text" },
          h("span", { class: "file-name" }, doc.dirty ? h("span", { class: "dirty", title: "Unsaved edits" }, "●") : null, doc.info.name),
          h(
            "span",
            { class: "file-meta" },
            `${doc.info.video ? `VIDEO · ${doc.info.video.frames} frames` : doc.info.format.toUpperCase()} · ${doc.measurements.length} meas.${doc.findings.length ? ` · ${doc.findings.length} findings` : ""}${doc.asset ? ` · ${doc.asset}` : ""}`,
          ),
        ),
        h(
          "button",
          {
            class: "icon close",
            title: "Close image",
            "aria-label": `Close ${doc.info.name}`,
            onclick: (e: Event) => {
              e.stopPropagation();
              void closeDoc(i);
            },
          },
          "×",
        ),
      ),
    ),
  );
  list.querySelectorAll<HTMLCanvasElement>("canvas.thumb").forEach((c, i) => drawThumb(c, state.docs[i]!));
}

export function drawThumb(canvas: HTMLCanvasElement, doc: Doc): void {
  const { width: w, height: hh } = doc.info;
  const tmp = document.createElement("canvas");
  tmp.width = w;
  tmp.height = hh;
  const pal = palette(doc.palette, doc).colors;
  const range = displayRange(doc);
  const img = new ImageData(w, hh);
  const span = range[1] - range[0] || 1;
  const n = pal.length;
  for (let i = 0; i < w * hh; i++) {
    const v = doc.kelvin[i]!;
    const c = Number.isNaN(v) ? [128, 128, 128] : pal[Math.min(n - 1, Math.max(0, Math.floor(((v - range[0]) / span) * n)))]!;
    img.data.set([c[0]!, c[1]!, c[2]!, 255], i * 4);
  }
  tmp.getContext("2d")!.putImageData(img, 0, 0);
  const ctx = canvas.getContext("2d")!;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const s = Math.min(canvas.width / w, canvas.height / hh);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(tmp, (canvas.width - w * s) / 2, (canvas.height - hh * s) / 2, w * s, hh * s);
}

export const TOOL_HINTS: Partial<Record<Tool, string>> = {
  spot: "Click to place a spot. With the keyboard: Tab to the image, press Enter, then the arrow keys move it (Shift for 10 pixels).",
  polygon: "Click points; double-click, Enter or click the first point to finish. Backspace removes the last point, Esc cancels.",
  polyline: "Click points; double-click or Enter to finish. Backspace removes the last point, Esc cancels.",
  wand: "Click a pixel: the area grows over connected pixels within the tolerance (Measurements section).",
  text: "Click where the note goes, type, then Enter.",
  sketch: "Drag to draw on the image.",
  crop: "Drag the area to keep. Esc cancels.",
};

export function updateToolbar(): void {
  const doc = currentDoc();
  for (const id of ["save", "batch"]) ($(`#${id}`) as HTMLButtonElement).disabled = !doc;
  ($("#save") as HTMLButtonElement).classList.toggle("attention", !!doc?.dirty);
  document.querySelectorAll<HTMLButtonElement>("button.tool").forEach((b) => {
    const active = b.dataset.tool === state.tool;
    b.classList.toggle("active", active);
    b.setAttribute("aria-pressed", String(active));
    b.disabled = !doc;
  });
  document.querySelectorAll<HTMLButtonElement>("[data-unit]").forEach((b) => {
    const active = b.dataset.unit === state.unit;
    b.classList.toggle("active", active);
    b.setAttribute("aria-pressed", String(active));
  });
  const hint = $("#tool-hint");
  const text = doc ? TOOL_HINTS[state.tool] : undefined;
  hint.hidden = !text;
  hint.textContent = text ?? "";
}

export function setTool(tool: Tool): void {
  state.tool = tool;
  viewer.tool = tool;
  updateToolbar();
}

export function setUnit(unit: Unit): void {
  state.unit = unit;
  saveSettings({ unit });
  updateToolbar();
  const doc = currentDoc();
  viewer.canvas.setAttribute("aria-label", viewerLabel(doc, unit));
  if (doc?.formulas.length) void refreshAnalysis(doc);
  panel.renderAll(doc);
}


// "Export image": the thermal image (cropped and rotated as shown) with measurements, findings,
// notes, an optional text overlay and logo, and a colour scale, as PNG or JPEG bytes.
import { pickers } from "./pickers";
import { api } from "./api";
import { errorText, h, toast } from "./dom";
import { field, modal } from "./modal";
import { drawAnnotations, drawFindings, drawMeasurements } from "./overlays";
import { alarmOn } from "./panel";
import { currentTemplate } from "./reportDialog";
import { DEFAULT_EXPORT, displayRange, loadSettings, lookOf, saveSettings, type Doc, type ExportOptions } from "./state";
import { temp } from "./units";
import type { Unit } from "./api";

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("Could not load the logo"));
    img.src = src;
  });
}

/** Renders the export to a canvas. */
export async function renderExport(doc: Doc, unit: Unit, smooth: boolean, opts: ExportOptions): Promise<HTMLCanvasElement> {
  const { width: w, height: h0 } = doc.info;
  const look = lookOf(doc);
  const thermalData = await api.render(doc.info.id, look, w, h0);
  const thermal = document.createElement("canvas");
  thermal.width = w;
  thermal.height = h0;
  thermal.getContext("2d")!.putImageData(thermalData, 0, 0);

  const [cx, cy, cw, ch] = doc.presentation.crop ?? [0, 0, w, h0];
  const a = (doc.presentation.rotation * Math.PI) / 180;
  const bw = Math.abs(cw * Math.cos(a)) + Math.abs(ch * Math.sin(a));
  const bh = Math.abs(cw * Math.sin(a)) + Math.abs(ch * Math.cos(a));
  // Auto: at least 1280 px wide so small sensors (80×60) still export legibly.
  const k = opts.scale > 0 ? opts.scale : Math.max(1, Math.ceil(1280 / bw));
  const outW = Math.round(bw * k);
  const imgH = Math.round(bh * k);
  const u = outW / 640;
  const footer = opts.colourBar ? Math.round(46 * u) : 0;

  const canvas = document.createElement("canvas");
  canvas.width = outW;
  canvas.height = imgH + footer;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#0f131a";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  const [mx, my] = [cx + cw / 2, cy + ch / 2];
  const toPx = (x: number, y: number): [number, number] => {
    const dx = (x - mx) * k;
    const dy = (y - my) * k;
    return [outW / 2 + dx * Math.cos(a) - dy * Math.sin(a), imgH / 2 + dx * Math.sin(a) + dy * Math.cos(a)];
  };
  ctx.save();
  ctx.translate(outW / 2, imgH / 2);
  ctx.rotate(a);
  ctx.scale(k, k);
  ctx.translate(-mx, -my);
  ctx.beginPath();
  ctx.rect(cx, cy, cw, ch);
  ctx.clip();
  ctx.imageSmoothingEnabled = smooth;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(thermal, 0, 0, w, h0);
  ctx.restore();

  const style = { scale: u * 1.2, selected: null, labels: opts.labels, rotation: a, pixel: k };
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, outW, imgH);
  ctx.clip();
  drawFindings(ctx, doc.findings, toPx, style);
  drawMeasurements(ctx, doc.measurements, toPx, style, doc.outlines, doc.measurements.map((_, i) => alarmOn(doc, i)));
  drawAnnotations(ctx, doc.annotations, toPx, style);
  ctx.restore();

  // Text overlay, top-left.
  const lines: string[] = [];
  if (opts.fileName) lines.push(doc.info.name + (doc.video ? ` · frame ${doc.video.frame + 1}` : ""));
  if (opts.dateTime && doc.info.camera.dateTime) lines.push(doc.info.camera.dateTime);
  if (opts.parameters) {
    const p = doc.params;
    const parts = [
      p.emissivity != null ? `ε ${p.emissivity.toFixed(2)}` : "",
      p.reflectedK != null ? `T refl ${temp(p.reflectedK, unit)}` : "",
      p.distanceM != null ? `d ${p.distanceM.toFixed(1)} m` : "",
    ].filter(Boolean);
    if (parts.length) lines.push(parts.join(" · "));
  }
  if (opts.extremes && doc.dataRange) lines.push(`Max ${temp(doc.dataRange[1], unit)} · Min ${temp(doc.dataRange[0], unit)}`);
  if (opts.customText.trim()) lines.push(...opts.customText.trim().split("\n"));
  if (lines.length) {
    const size = 12 * u;
    ctx.font = `600 ${size}px system-ui, -apple-system, "Segoe UI", sans-serif`;
    const width = Math.max(...lines.map((l) => ctx.measureText(l).width)) + 14 * u;
    ctx.fillStyle = "rgba(10,14,22,0.68)";
    ctx.beginPath();
    ctx.roundRect(8 * u, 8 * u, width, lines.length * size * 1.35 + 10 * u, 5 * u);
    ctx.fill();
    ctx.fillStyle = "#fff";
    ctx.textBaseline = "top";
    ctx.textAlign = "left";
    lines.forEach((l, i) => ctx.fillText(l, 15 * u, 13 * u + i * size * 1.35));
  }

  if (opts.logo) {
    try {
      const logo = await loadImage(opts.logo);
      const lw = Math.min(outW * 0.18, logo.width);
      const lh = (logo.height / logo.width) * lw;
      const m = 10 * u;
      const x = opts.logoCorner.endsWith("left") ? m : outW - lw - m;
      const y = opts.logoCorner.startsWith("top") ? m + (lines.length && opts.logoCorner === "top-left" ? lines.length * 16 * u + 16 * u : 0) : imgH - lh - m;
      ctx.drawImage(logo, x, y, lw, lh);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  if (opts.colourBar) {
    const range = displayRange(doc);
    const top = imgH;
    ctx.fillStyle = "#0f131a";
    ctx.fillRect(0, top, outW, footer);
    const barX = 14 * u;
    const barW = outW - 28 * u;
    const bar = ctx.createLinearGradient(barX, 0, barX + barW, 0);
    const n = look.palette.colors.length;
    look.palette.colors.forEach((c, i) => bar.addColorStop(i / (n - 1), `rgb(${c[0]},${c[1]},${c[2]})`));
    ctx.fillStyle = bar;
    ctx.fillRect(barX, top + 8 * u, barW, 12 * u);
    ctx.fillStyle = "#e5e7eb";
    ctx.font = `500 ${11 * u}px system-ui, -apple-system, "Segoe UI", sans-serif`;
    ctx.textBaseline = "top";
    ctx.textAlign = "left";
    ctx.fillText(temp(range[0], unit), barX, top + 25 * u);
    ctx.textAlign = "right";
    ctx.fillText(temp(range[1], unit), barX + barW, top + 25 * u);
    ctx.textAlign = "center";
    ctx.fillStyle = "#9ca3af";
    ctx.fillText(`${doc.info.name} · ${look.palette.name}`, outW / 2, top + 25 * u);
  }
  return canvas;
}

export async function encode(canvas: HTMLCanvasElement, format: "png" | "jpg"): Promise<Uint8Array> {
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("Could not encode the image"))), format === "png" ? "image/png" : "image/jpeg", 0.92),
  );
  return new Uint8Array(await blob.arrayBuffer());
}

/** PNG bytes with the default options (kept for automated checks). */
export async function exportPng(doc: Doc, unit: Unit, smooth: boolean): Promise<Uint8Array> {
  return encode(await renderExport(doc, unit, smooth, { ...DEFAULT_EXPORT, format: "png" }), "png");
}

/** The export options dialog; resolves to the chosen options or null. */
export async function exportOptions(): Promise<ExportOptions | null> {
  const o: ExportOptions = { ...DEFAULT_EXPORT, ...(loadSettings().exportOverlay ?? {}) };
  const check = (key: "colourBar" | "labels" | "fileName" | "dateTime" | "parameters" | "extremes", label: string) =>
    h("label", { class: "check" }, h("input", { type: "checkbox", checked: o[key], onchange: (e: Event) => (o[key] = (e.target as HTMLInputElement).checked) }), label);
  const logoState = h("span", { class: "muted small" }, o.logo ? "Logo chosen" : "No logo");
  const result = await modal(
    "Export image",
    [
      h(
        "div",
        { class: "grid2" },
        field("Format", h("select", { onchange: (e: Event) => (o.format = (e.target as HTMLSelectElement).value as "png" | "jpg") }, h("option", { value: "png", selected: o.format === "png" }, "PNG"), h("option", { value: "jpg", selected: o.format === "jpg" }, "JPEG"))),
        field(
          "Size",
          h(
            "select",
            { onchange: (e: Event) => (o.scale = Number((e.target as HTMLSelectElement).value)) },
            [
              [0, "Automatic (≥ 1280 px)"],
              [1, "Native (1 px per pixel)"],
              [2, "2×"],
              [4, "4×"],
              [8, "8×"],
            ].map(([v, l]) => h("option", { value: String(v), selected: o.scale === v }, String(l))),
          ),
        ),
      ),
      h("fieldset", { class: "plain" }, h("legend", {}, "Show"), h("div", { class: "checks wrap" }, check("colourBar", "Colour scale"), check("labels", "Labels"), check("fileName", "File name"), check("dateTime", "Capture time"), check("parameters", "Emissivity and distance"), check("extremes", "Max and min"))),
      field("Custom text", h("textarea", { rows: 2, value: o.customText, placeholder: "Company, job number…", oninput: (e: Event) => (o.customText = (e.target as HTMLTextAreaElement).value) })),
      h(
        "div",
        { class: "row" },
        h("span", { class: "label wide" }, "Logo"),
        logoState,
        h(
          "button",
          {
            type: "button",
            onclick: async () => {
              const path = await pickers.open({ filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }] });
              if (typeof path !== "string") return;
              try {
                o.logo = await api.readImageDataUrl(path);
                logoState.textContent = "Logo chosen";
              } catch (e) {
                toast(errorText(e), "error");
              }
            },
          },
          "Choose…",
        ),
        h(
          "button",
          {
            type: "button",
            onclick: () => {
              o.logo = currentTemplate().logo;
              logoState.textContent = o.logo ? "Template logo" : "The template has no logo";
            },
          },
          "From template",
        ),
        h("button", { type: "button", onclick: () => ((o.logo = null), (logoState.textContent = "No logo")) }, "None"),
        h(
          "select",
          { "aria-label": "Logo corner", onchange: (e: Event) => (o.logoCorner = (e.target as HTMLSelectElement).value as ExportOptions["logoCorner"]) },
          (["top-right", "top-left", "bottom-right", "bottom-left"] as const).map((c) => h("option", { value: c, selected: o.logoCorner === c }, c.replace("-", " "))),
        ),
      ),
    ],
    [
      { label: "Cancel", value: "cancel" },
      { label: "Export…", value: "ok", primary: true },
    ],
  );
  if (result !== "ok") return null;
  saveSettings({ exportOverlay: o });
  return o;
}

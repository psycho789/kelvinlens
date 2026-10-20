import type { Panel } from "../panel";
import { type Distribution, type Range } from "../api";
import { fill, h } from "../dom";
import { DISTRIBUTIONS, gradientCss, narrowed, num, tempInput } from "../panelWidgets";
import { CAMERA_PALETTE, type Doc, type View, displayRange, palette, palettesFor, state } from "../state";
import { inputValue, symbol, toKelvin } from "../units";

export function renderDisplay(this: Panel, doc: Doc | null): void {
  const el = this.els.display;
  if (!doc) {
    fill(el, h("p", { class: "muted" }, "Open an image to begin."));
    return;
  }
  if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") {
    this.drawHistogram(doc);
    return;
  }
  const unit = state.unit;
  const range = displayRange(doc);
  const pal = palette(doc.palette, doc);
  const all = palettesFor(doc);

  const option = (p: { id: string; name: string }) => h("option", { value: p.id, selected: p.id === pal.id }, p.name);
  const select = h(
    "select",
    {
      id: "palette",
      onchange: (e: Event) => {
        const v = (e.target as HTMLSelectElement).value;
        if (v === "__edit") {
          (e.target as HTMLSelectElement).value = pal.id;
          this.on.editPalettes();
        } else this.on.setPalette(v);
      },
    },
    all.some((p) => p.id === CAMERA_PALETTE) ? h("optgroup", { label: "From the camera" }, option(all.find((p) => p.id === CAMERA_PALETTE)!)) : null,
    h("optgroup", { label: "Built in" }, state.palettes.map(option)),
    state.customPalettes.length ? h("optgroup", { label: "Custom" }, state.customPalettes.map(option)) : null,
    h("option", { value: "__edit" }, "Edit custom palettes…"),
  );
  const swatch = h("div", { class: "swatch" });
  swatch.style.background = gradientCss(pal.colors);

  const rangeInput = (i: 0 | 1) =>
    h("input", {
      type: "number",
      step: "0.1",
      class: "num",
      value: inputValue(range[i], unit),
      "aria-label": i === 0 ? "Range minimum" : "Range maximum",
      onchange: (e: Event) => {
        const v = Number((e.target as HTMLInputElement).value);
        if (!Number.isFinite(v)) return;
        const next: Range = [...range];
        next[i] = toKelvin(v, unit);
        if (next[0] < next[1]) this.on.setRange(next);
        else this.renderDisplay(doc);
      },
    });

  const hasPhoto = !!doc.photo;
  const views: [View, string, string][] = [
    ["thermal", "Thermal", "Thermal image only"],
    ["photo", "Photo", "The camera's photo"],
    ["blend", "Blend", "Thermal over the photo"],
    ["msx", "Edges", "Thermal with the photo's edges embossed (like FLIR MSX)"],
    ["fusion", "Fusion", "The photo with thermal colours only in a temperature interval"],
  ];
  const viewButtons = h(
    "div",
    { class: "segmented wrap", role: "group", "aria-label": "View" },
    views.map(([v, label, title]) =>
      h(
        "button",
        {
          class: state.view === v ? "active" : "",
          disabled: v !== "thermal" && !hasPhoto,
          title: v !== "thermal" && !hasPhoto ? "This file has no photo" : title,
          onclick: () => this.on.setView(v),
        },
        label,
      ),
    ),
  );

  const fusion = state.fusionRange ?? [range[0] + (range[1] - range[0]) * 0.6, range[1]];
  const rotation = doc.presentation.rotation;

  fill(
    el,
    h("div", { class: "row" }, h("label", { for: "palette" }, "Palette"), select),
    swatch,
    h("canvas", { class: "histogram", width: 280, height: 56, title: "Temperature distribution; the shaded band is the display range" }),
    h("div", { class: "row range-row" }, rangeInput(0), h("span", { class: "muted" }, "to"), rangeInput(1), h("span", { class: "unit" }, symbol(unit))),
    h(
      "div",
      { class: "row buttons" },
      h("button", { class: doc.range ? "" : "active", onclick: () => this.on.setRange(null), title: "Fit the range to the image" }, "Auto"),
      doc.info.embeddedRange ? h("button", { onclick: () => this.on.setRange(doc.info.embeddedRange), title: "The range the camera displayed" }, "Camera") : null,
      h("button", { onclick: () => this.on.setRange(narrowed(doc)), title: "Ignore the hottest and coldest 1 % of pixels" }, "Contrast"),
    ),
    h(
      "div",
      { class: "row" },
      h("label", { for: "distribution" }, "Colours"),
      h(
        "select",
        { id: "distribution", title: "How temperatures are spread over the palette", onchange: (e: Event) => this.on.setDistribution((e.target as HTMLSelectElement).value as Distribution) },
        DISTRIBUTIONS.map(([d, name]) => h("option", { value: d, selected: d === doc.distribution }, name)),
      ),
    ),
    h("div", { class: "row" }, h("span", { class: "label" }, "View"), viewButtons),
    state.view === "blend" && hasPhoto
      ? h(
          "div",
          { class: "row" },
          h("label", { for: "blend" }, "Opacity"),
          h("input", { id: "blend", type: "range", min: "0", max: "1", step: "0.01", value: String(state.blend), oninput: (e: Event) => this.on.setBlend(Number((e.target as HTMLInputElement).value)) }),
        )
      : null,
    state.view === "msx" && hasPhoto
      ? h(
          "div",
          { class: "row" },
          h("label", { for: "msx" }, "Edges"),
          h("input", { id: "msx", type: "range", min: "0", max: "1", step: "0.05", value: String(state.msxStrength), onchange: (e: Event) => this.on.setMsxStrength(Number((e.target as HTMLInputElement).value)) }),
        )
      : null,
    state.view === "fusion" && hasPhoto
      ? h(
          "div",
          { class: "row range-row" },
          h("span", { class: "label" }, "Show"),
          tempInput(fusion[0], (k) => k != null && this.on.setFusionRange([k, fusion[1]]), "Fusion from"),
          h("span", { class: "muted" }, "to"),
          tempInput(fusion[1], (k) => k != null && this.on.setFusionRange([fusion[0], k]), "Fusion to"),
          h("span", { class: "unit" }, symbol(unit)),
        )
      : null,
    hasPhoto && state.view !== "thermal" && state.view !== "photo" ? this.alignRow(doc) : null,
    h(
      "div",
      { class: "row buttons" },
      h("span", { class: "label" }, "Rotate"),
      h("button", { title: "Rotate 90° left", "aria-label": "Rotate left", onclick: () => this.on.setRotation(rotation - 90) }, "⟲"),
      h("button", { title: "Rotate 90° right", "aria-label": "Rotate right", onclick: () => this.on.setRotation(rotation + 90) }, "⟳"),
      h("input", {
        type: "number",
        class: "num small-num",
        step: "1",
        value: String(Math.round(rotation * 10) / 10),
        title: "Rotation in degrees (any angle)",
        "aria-label": "Rotation in degrees",
        onchange: (e: Event) => {
          const v = num((e.target as HTMLInputElement).value);
          if (v != null) this.on.setRotation(v);
        },
      }),
      h("span", { class: "unit" }, "°"),
    ),
    h(
      "div",
      { class: "row buttons" },
      h("span", { class: "label" }, "Crop"),
      h("button", { onclick: () => this.on.startCrop(), title: "Drag a rectangle on the image (thermal view, exports and reports)" }, doc.presentation.crop ? "Re-crop" : "Crop…"),
      doc.presentation.crop ? h("button", { onclick: () => this.on.clearCrop() }, "Clear") : null,
    ),
    h(
      "div",
      { class: "row checks" },
      h("label", {}, h("input", { type: "checkbox", checked: state.smooth, onchange: (e: Event) => this.on.setSmooth((e.target as HTMLInputElement).checked) }), "Smooth"),
      h(
        "label",
        {},
        h("input", { type: "checkbox", checked: state.showExtremes, onchange: (e: Event) => this.on.setShowExtremes((e.target as HTMLInputElement).checked) }),
        "Hot/cold spots",
      ),
    ),
  );
  this.drawHistogram(doc);
}

/** Nudges the thermal image on the photo when the camera's alignment is off. */
export function alignRow(this: Panel, doc: Doc): HTMLElement {
  const [dx, dy, s] = doc.presentation.align ?? [0, 0, 1];
  const step = Math.max(1, Math.round((doc.photo?.width ?? 640) / 320));
  const set = (a: [number, number, number]) => this.on.setAlign([Math.round(a[0]), Math.round(a[1]), Math.round(a[2] * 1000) / 1000]);
  const b = (label: string, title: string, a: [number, number, number]) => h("button", { title, "aria-label": title, onclick: () => set(a) }, label);
  return h(
    "div",
    { class: "row buttons align-row" },
    h("span", { class: "label", title: "Move or scale the thermal image on the photo if they don't line up" }, "Align"),
    b("←", "Move left", [dx - step, dy, s]),
    b("→", "Move right", [dx + step, dy, s]),
    b("↑", "Move up", [dx, dy - step, s]),
    b("↓", "Move down", [dx, dy + step, s]),
    b("−", "Smaller", [dx, dy, s * 0.99]),
    b("+", "Larger", [dx, dy, s * 1.01]),
    doc.presentation.align ? h("button", { title: "Back to the camera's alignment", onclick: () => this.on.setAlign(null) }, "Reset") : null,
  );
}

export function drawHistogram(this: Panel, doc: Doc): void {
  const canvas = this.els.display.querySelector<HTMLCanvasElement>("canvas.histogram");
  if (!canvas || !doc.dataRange) return;
  const ctx = canvas.getContext("2d")!;
  const [lo, hi] = doc.dataRange;
  const bins = new Array<number>(70).fill(0);
  const span = hi - lo || 1;
  for (const v of doc.kelvin) {
    if (Number.isNaN(v)) continue;
    bins[Math.min(bins.length - 1, Math.floor(((v - lo) / span) * bins.length))]! += 1;
  }
  const peak = Math.max(...bins, 1);
  const { width: w, height: hh } = canvas;
  ctx.clearRect(0, 0, w, hh);
  const [r0, r1] = displayRange(doc);
  const x = (k: number) => ((k - lo) / span) * w;
  ctx.fillStyle = "rgba(56,189,248,0.14)";
  ctx.fillRect(Math.max(0, x(r0)), 0, Math.min(w, x(r1)) - Math.max(0, x(r0)), hh);
  const pal = palette(doc.palette, doc).colors;
  const bw = w / bins.length;
  bins.forEach((n, i) => {
    const k = lo + ((i + 0.5) / bins.length) * span;
    const t = Math.min(255, Math.max(0, Math.floor(((k - r0) / (r1 - r0 || 1)) * 256)));
    const c = pal[Math.min(pal.length - 1, Math.floor((t * pal.length) / 256))]!;
    ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
    const bh = Math.max(n > 0 ? 1 : 0, (Math.sqrt(n) / Math.sqrt(peak)) * (hh - 4));
    ctx.fillRect(i * bw + 0.5, hh - bh, bw - 1, bh);
  });
}

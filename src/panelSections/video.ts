import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { deltaInput } from "../panelWidgets";
import { type Doc, state } from "../state";
import { symbol } from "../units";

export function renderVideo(this: Panel, doc: Doc | null): void {
  const v = doc?.video;
  this.videoSection.hidden = !v;
  if (!doc || !v) return;
  const el = this.els.video;
  const info = doc.info.video!;
  const n = info.frames;
  const t = info.times[v.frame] ?? 0;
  const existing = el.querySelector<HTMLInputElement>("input.frame-slider");
  if (existing && el.contains(document.activeElement) && document.activeElement === existing) {
    el.querySelector(".frame-label")!.textContent = `Frame ${v.frame + 1} / ${n} · ${t.toFixed(2)} s`;
    return;
  }
  fill(
    el,
    h("input", {
      type: "range",
      class: "frame-slider",
      min: "0",
      max: String(n - 1),
      step: "1",
      value: String(v.frame),
      "aria-label": "Frame",
      oninput: (e: Event) => this.on.seek(Number((e.target as HTMLInputElement).value)),
    }),
    h("div", { class: "row frame-row" },
      h("span", { class: "frame-label" }, `Frame ${v.frame + 1} / ${n} · ${t.toFixed(2)} s`),
      h("span", { class: "spacer" }),
      h("button", { title: "First frame", "aria-label": "First frame", onclick: () => this.on.seek(0) }, "⏮"),
      h("button", { title: "Previous frame (,)", "aria-label": "Previous frame", onclick: () => this.on.seek(Math.max(0, v.frame - 1)) }, "◀"),
      h("button", { class: "play", title: "Play / pause (K)", "aria-label": v.playing ? "Pause" : "Play", onclick: () => this.on.togglePlay() }, v.playing ? "⏸" : "▶"),
      h("button", { title: "Next frame (.)", "aria-label": "Next frame", onclick: () => this.on.seek(Math.min(n - 1, v.frame + 1)) }, "▶|"),
      h("button", { title: "Last frame", "aria-label": "Last frame", onclick: () => this.on.seek(n - 1) }, "⏭"),
    ),
    h(
      "div",
      { class: "row buttons" },
      h("span", { class: "label" }, "Segment"),
      h("button", { title: "Set the segment start to this frame", onclick: () => this.on.setInOut(v.frame, Math.max(v.frame, v.last)) }, `In ${v.first + 1}`),
      h("button", { title: "Set the segment end to this frame", onclick: () => this.on.setInOut(Math.min(v.first, v.frame), v.frame) }, `Out ${v.last + 1}`),
      h("span", { class: "muted small" }, info.frameRate ? `${info.frameRate.toFixed(1)} fps` : ""),
    ),
    h(
      "div",
      { class: "row buttons wrap" },
      h("button", { onclick: () => this.on.plotSeries(), title: "Plot every measurement across the segment" }, "Plot over time"),
      h("button", { onclick: () => this.on.exportVideo(), title: "Save the segment as an AVI video with palette and measurements" }, "Export AVI…"),
      h("button", { onclick: () => this.on.exportSegment(), title: "Save the segment as a radiometric SEQ file" }, "Save SEQ…"),
    ),
    h(
      "div",
      { class: "row checks" },
      h("label", { title: "High-sensitivity mode: shows what changes between frames, such as gas plumes" }, h("input", { type: "checkbox", checked: v.hsm, onchange: (e: Event) => this.on.setHsm({ hsm: (e.target as HTMLInputElement).checked }) }), "High sensitivity (gas)"),
      h("label", { title: "Average 2×2 pixel blocks to reduce noise" }, h("input", { type: "checkbox", checked: v.binning, onchange: (e: Event) => this.on.setHsm({ binning: (e.target as HTMLInputElement).checked }) }), "Binning"),
    ),
    v.hsm
      ? h(
          "div",
          { class: "row" },
          h("span", { class: "label" }, "Reference"),
          h("input", { type: "number", class: "num small-num", min: "1", max: "60", value: String(v.hsmReference), title: "Frames averaged as the background", onchange: (e: Event) => this.on.setHsm({ hsmReference: Math.max(1, Math.min(60, Number((e.target as HTMLInputElement).value) || 8)) }) }),
          h("span", { class: "muted small" }, "frames, ±"),
          deltaInput(v.hsmSpan, (dk) => this.on.setHsm({ hsmSpan: Math.max(0.01, dk) }), "HSM sensitivity"),
          h("span", { class: "unit" }, state.unit === "K" ? "K" : symbol(state.unit)),
        )
      : null,
  );
}

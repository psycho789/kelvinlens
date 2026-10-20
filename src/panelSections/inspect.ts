import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { severityColor } from "../overlays";
import { deltaInput, tempInput } from "../panelWidgets";
import { type Doc, state } from "../state";
import { delta, symbol } from "../units";

export function renderInspect(this: Panel, doc: Doc | null): void {
  const el = this.els.inspect;
  if (!doc) {
    fill(el);
    return;
  }
  if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
  const s = state.inspect;
  const unit = state.unit;
  const set = (patch: Partial<typeof s>) => {
    Object.assign(s, patch);
    this.renderInspect(doc);
  };
  fill(
    el,
    h(
      "div",
      { class: "row" },
      h("span", { class: "label" }, "Find"),
      h(
        "select",
        { "aria-label": "What to find", onchange: (e: Event) => set({ mode: (e.target as HTMLSelectElement).value as typeof s.mode }) },
        h("option", { value: "hot", selected: s.mode === "hot" }, "Hot spots (electrical, NETA)"),
        h("option", { value: "cold", selected: s.mode === "cold" }, "Cold spots (building)"),
        h("option", { value: "both", selected: s.mode === "both" }, "Both"),
      ),
    ),
    h(
      "div",
      { class: "row" },
      h("span", { class: "label" }, "Min ΔT"),
      deltaInput(s.minDelta, (dk) => set({ minDelta: dk }), "Smallest difference"),
      h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)),
      h("span", { class: "muted small" }, "size ≥"),
      h("input", { type: "number", class: "num small-num", min: "1", value: String(s.minPixels), "aria-label": "Smallest size in pixels", onchange: (e: Event) => set({ minPixels: Math.max(1, Number((e.target as HTMLInputElement).value) || 1) }) }),
      h("span", { class: "muted small" }, "px"),
    ),
    h(
      "div",
      { class: "row" },
      h("span", { class: "label" }, "Reference"),
      tempInput(s.referenceK, (k) => set({ referenceK: k }), "Reference temperature (empty: image median)"),
      h("span", { class: "muted small" }, s.referenceK == null ? "median" : symbol(unit)),
      s.mode !== "cold"
        ? h(
            "select",
            { "aria-label": "NETA basis", title: "NETA MTS Table 100.18 basis", onchange: (e: Event) => set({ basis: (e.target as HTMLSelectElement).value as typeof s.basis }) },
            h("option", { value: "similar", selected: s.basis === "similar" }, "vs similar"),
            h("option", { value: "ambient", selected: s.basis === "ambient" }, "vs ambient"),
          )
        : null,
    ),
    s.mode !== "hot"
      ? h(
          "div",
          { class: "row" },
          h("span", { class: "label" }, "In / out"),
          tempInput(s.insideK, (k) => set({ insideK: k }), "Indoor temperature"),
          tempInput(s.outsideK, (k) => set({ outsideK: k }), "Outdoor temperature"),
          h("span", { class: "unit" }, symbol(unit)),
        )
      : null,
    h(
      "div",
      { class: "row buttons" },
      h("button", { class: "primary", onclick: () => this.on.runInspect() }, "Inspect"),
      doc.findings.length ? h("button", { onclick: () => this.on.findingsToMeasurements(), title: "Add a box measurement for each finding" }, "Add as boxes") : null,
      doc.findings.length ? h("button", { onclick: () => this.on.clearFindings() }, "Clear") : null,
    ),
    doc.findings.length
      ? h(
          "table",
          { class: "measurements findings" },
          h("tbody", {}, doc.findings.map((f) =>
            h(
              "tr",
              { title: f.severity.action },
              h("td", {}, h("i", { class: "dot", style: `background:${severityColor(f.severity.level)}` }), f.label),
              h("td", { class: "n" }, delta(f.deltaK, unit)),
              h("td", {}, f.severity.label),
            ),
          )),
        )
      : null,
  );
}

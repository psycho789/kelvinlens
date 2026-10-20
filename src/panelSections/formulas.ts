import type { Panel } from "../panel";
import { type Formula, type ResultKind } from "../api";
import { fill, h } from "../dom";
import { resultText } from "../panelWidgets";
import { type Doc, state } from "../state";

export function renderFormulas(this: Panel, doc: Doc | null): void {
  const el = this.els.formulas;
  if (!doc) {
    fill(el);
    return;
  }
  const unit = state.unit;
  const focused = el.contains(document.activeElement);
  const rows = el.querySelectorAll<HTMLElement>(".formula");
  if (focused && rows.length === doc.formulas.length) {
    rows.forEach((row, i) => (row.querySelector(".formula-result")!.textContent = resultText(doc, i, unit)));
    return;
  }
  const update = (i: number, patch: Partial<Formula>) => {
    const next = doc.formulas.map((f, k) => (k === i ? { ...f, ...patch } : f));
    this.on.setFormulas(next);
  };
  const labels = doc.measurements.map((m) => m.label);
  const ref = (l: string) => (/^[\p{L}_][\p{L}\p{N}_]*$/u.test(l) ? l : `[${l}]`);
  const presets: [string, () => Formula | null][] = [
    ["ΔT of maxima (first two)", () => (labels.length >= 2 ? { label: "dT", expression: `${ref(labels[0]!)}.max - ${ref(labels[1]!)}.max`, kind: "difference" } : null)],
    ["Hottest of all", () => (labels.length ? { label: "Hottest", expression: `max(${labels.map((l) => `${ref(l)}.max`).join(", ")})`, kind: "temperature" } : null)],
    ["Average of all", () => (labels.length ? { label: "Mean", expression: `avg(${labels.map((l) => `${ref(l)}.avg`).join(", ")})`, kind: "temperature" } : null)],
    ["Rise over a reference temperature", () => (labels.length ? { label: "Rise", expression: `${ref(labels[0]!)}.max - 20`, kind: "difference" } : null)],
    ["Thermal index (inside 20, outside 0)", () => (labels.length ? { label: "TI", expression: `(${ref(labels[0]!)}.min - 0) / (20 - 0) * 100`, kind: "number" } : null)],
    ["Empty formula", () => ({ label: `F${doc.formulas.length + 1}`, expression: "", kind: "temperature" })],
  ];
  fill(
    el,
    doc.formulas.length === 0 ? h("p", { class: "muted small" }, "Combine measurement results, e.g. Bx1.max - Sp1. Statistics: .max .min .avg .sd .count; functions abs, min, max, avg, sqrt, round.") : null,
    doc.formulas.map((f, i) =>
      h(
        "div",
        { class: "formula" },
        h("input", { class: "label-input", value: f.label, "aria-label": "Formula label", onchange: (e: Event) => update(i, { label: (e.target as HTMLInputElement).value.trim() }) }),
        h("input", { type: "text", class: "expr", value: f.expression, spellcheck: false, placeholder: "Bx1.max - Sp1", "aria-label": "Formula", onchange: (e: Event) => update(i, { expression: (e.target as HTMLInputElement).value }) }),
        h(
          "select",
          { "aria-label": "Result type", title: "How the result is shown", onchange: (e: Event) => update(i, { kind: (e.target as HTMLSelectElement).value as ResultKind }) },
          ([["temperature", "°"], ["difference", "Δ"], ["number", "#"]] as const).map(([k, l]) => h("option", { value: k, selected: f.kind === k }, l)),
        ),
        h("button", { class: "icon", title: "Delete formula", "aria-label": `Delete ${f.label}`, onclick: () => this.on.setFormulas(doc.formulas.filter((_, k) => k !== i)) }, "×"),
        h("div", { class: `formula-result ${doc.formulaResults[i]?.error ? "error" : ""}` }, resultText(doc, i, unit)),
      ),
    ),
    h(
      "select",
      {
        class: "presets",
        "aria-label": "Add a formula",
        onchange: (e: Event) => {
          const sel = e.target as HTMLSelectElement;
          const make = presets[Number(sel.value)]?.[1];
          sel.value = "";
          const f = make?.();
          if (f) this.on.setFormulas([...doc.formulas, f]);
        },
      },
      h("option", { value: "" }, "Add a formula…"),
      presets.map(([name], i) => h("option", { value: String(i) }, name)),
    ),
  );
}

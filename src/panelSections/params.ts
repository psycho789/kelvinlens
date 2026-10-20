import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { EMISSIVITY_PRESETS, PARAM_FIELDS, fieldValue, parseField, unitLabel } from "../panelWidgets";
import { type Doc, state } from "../state";
import { fromKelvin, toKelvin } from "../units";

export function renderParams(this: Panel, doc: Doc | null): void {
  const el = this.els.params;
  if (!doc) {
    fill(el);
    return;
  }
  if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
  if (doc.info.editable.length === 0) {
    fill(el, h("p", { class: "muted" }, `${doc.info.formatLabel} files store final temperatures, so their parameters cannot be changed.`));
    return;
  }
  const unit = state.unit;
  const rows = PARAM_FIELDS.filter((f) => doc.info.editable.includes(f.key)).map((f) => {
    const fileValue = doc.info.fileParams[f.key];
    const current = doc.params[f.key];
    const changed = fieldValue(f, current, unit) !== fieldValue(f, fileValue, unit);
    const input = h("input", {
      type: "number",
      class: "num",
      id: `param-${f.key}`,
      step: String(f.step),
      min: String(f.kind === "temperature" ? fromKelvin(toKelvin(f.min, "C"), unit) : f.min),
      max: String(f.kind === "temperature" ? fromKelvin(toKelvin(f.max, "C"), unit) : f.max),
      value: fieldValue(f, current, unit),
      onchange: (e: Event) => {
        const v = parseField(f, (e.target as HTMLInputElement).value, unit);
        if (v == null) return;
        this.on.setParams({ ...doc.params, [f.key]: v });
      },
    });
    return h(
      "div",
      { class: `param ${changed ? "changed" : ""}` },
      h("label", { for: `param-${f.key}` }, f.label),
      input,
      h("span", { class: "unit" }, unitLabel(f, unit)),
      h("span", { class: "file-value", title: "Value stored in the file" }, changed ? `file ${fieldValue(f, fileValue, unit)}` : ""),
    );
  });

  const presets = h(
    "select",
    {
      class: "presets",
      "aria-label": "Emissivity presets",
      onchange: (e: Event) => {
        const v = Number((e.target as HTMLSelectElement).value);
        if (v > 0) this.on.setParams({ ...doc.params, emissivity: v });
      },
    },
    h("option", { value: "" }, "Typical emissivities…"),
    EMISSIVITY_PRESETS.map(([name, v]) => h("option", { value: String(v) }, `${name} — ${v.toFixed(2)}`)),
  );

  const anyChanged = PARAM_FIELDS.some((f) => doc.params[f.key] !== doc.info.fileParams[f.key]);
  fill(
    el,
    h("div", { class: "params" }, rows),
    doc.info.editable.includes("emissivity") ? presets : null,
    h(
      "div",
      { class: "row buttons" },
      h("button", { disabled: !anyChanged, onclick: () => this.on.resetParams() }, "Reset to file"),
      state.docs.length > 1
        ? h("button", { onclick: () => this.on.applyParamsToAll(), title: "Use these values for every open image of a format that supports them" }, "Apply to all images")
        : null,
    ),
  );
}

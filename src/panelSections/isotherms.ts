import type { Panel } from "../panel";
import { type Isotherm } from "../api";
import { hexRgb, rgbHex } from "../color";
import { fill, h } from "../dom";
import { isothermText, num, tempInput } from "../panelWidgets";
import { type Doc, displayRange, state } from "../state";
import { symbol } from "../units";

export function renderIsotherms(this: Panel, doc: Doc | null): void {
  const el = this.els.isotherms;
  if (!doc) {
    fill(el);
    return;
  }
  if (el.contains(document.activeElement) && document.activeElement?.tagName === "INPUT") return;
  const unit = state.unit;
  const range = displayRange(doc);
  const update = (i: number, iso: Isotherm) => this.on.setIsotherms(doc.isotherms.map((x, k) => (k === i ? iso : x)));
  const air = doc.params.atmosphereK ?? 293.15;
  const make: Record<string, () => Isotherm> = {
    above: () => ({ kind: "above", t: range[0] + (range[1] - range[0]) * 0.8, color: [255, 40, 40], opacity: 1 }),
    below: () => ({ kind: "below", t: range[0] + (range[1] - range[0]) * 0.2, color: [40, 120, 255], opacity: 1 }),
    interval: () => ({ kind: "interval", lo: range[0] + (range[1] - range[0]) * 0.4, hi: range[0] + (range[1] - range[0]) * 0.6, color: [60, 220, 90], opacity: 1 }),
    condensation: () => ({ kind: "condensation", airK: air, rh: doc.params.humidity ?? 0.5, limit: 0.8, color: [30, 144, 255], opacity: 0.85 }),
    insulation: () => ({ kind: "insulation", insideK: 293.15, outsideK: 273.15, index: 0.7, color: [0, 200, 255], opacity: 0.85 }),
  };
  const pct = (v: number, set: (v: number) => void, label: string) =>
    h("input", {
      type: "number",
      class: "num small-num",
      min: "0",
      max: "100",
      step: "1",
      value: String(Math.round(v * 100)),
      "aria-label": label,
      onchange: (e: Event) => {
        const n = num((e.target as HTMLInputElement).value);
        if (n != null && n >= 0 && n <= 100) set(n / 100);
      },
    });
  const rows = doc.isotherms.map((iso, i) => {
    const values: (HTMLElement | string)[] = [];
    switch (iso.kind) {
      case "above":
      case "below":
        values.push(tempInput(iso.t, (k) => k != null && update(i, { ...iso, t: k }), "Isotherm temperature"), symbol(unit));
        break;
      case "interval":
        values.push(
          tempInput(iso.lo, (k) => k != null && update(i, { ...iso, lo: k }), "From"),
          "–",
          tempInput(iso.hi, (k) => k != null && update(i, { ...iso, hi: k }), "To"),
          symbol(unit),
        );
        break;
      case "condensation":
        values.push(
          "air",
          tempInput(iso.airK, (k) => k != null && update(i, { ...iso, airK: k }), "Air temperature"),
          "RH",
          pct(iso.rh, (v) => update(i, { ...iso, rh: v }), "Air relative humidity"),
          "% limit",
          pct(iso.limit, (v) => update(i, { ...iso, limit: Math.max(0.01, v) }), "Surface humidity limit"),
          "%",
        );
        break;
      case "insulation":
        values.push(
          "in",
          tempInput(iso.insideK, (k) => k != null && update(i, { ...iso, insideK: k }), "Indoor temperature"),
          "out",
          tempInput(iso.outsideK, (k) => k != null && update(i, { ...iso, outsideK: k }), "Outdoor temperature"),
          "index <",
          pct(iso.index, (v) => update(i, { ...iso, index: v }), "Thermal index limit"),
          "%",
        );
        break;
    }
    return h(
      "div",
      { class: "isotherm" },
      h("div", { class: "row" },
        h("input", { type: "color", value: rgbHex(iso.color), "aria-label": "Isotherm colour", onchange: (e: Event) => update(i, { ...iso, color: hexRgb((e.target as HTMLInputElement).value) }) }),
        h("span", { class: "iso-name" }, isothermText(iso, unit)),
        h("span", { class: "spacer" }),
        h("input", { type: "range", class: "opacity", min: "0.1", max: "1", step: "0.05", value: String(iso.opacity), title: "Opacity", "aria-label": "Opacity", onchange: (e: Event) => update(i, { ...iso, opacity: Number((e.target as HTMLInputElement).value) }) }),
        h("button", { class: "icon", title: "Remove", "aria-label": "Remove isotherm", onclick: () => this.on.setIsotherms(doc.isotherms.filter((_, k) => k !== i)) }, "×"),
      ),
      h("div", { class: "row iso-values" }, values.map((v) => (typeof v === "string" ? h("span", { class: "muted small" }, v) : v))),
    );
  });
  fill(
    el,
    rows,
    h(
      "select",
      {
        class: "presets",
        "aria-label": "Add an isotherm",
        onchange: (e: Event) => {
          const sel = e.target as HTMLSelectElement;
          const f = make[sel.value];
          sel.value = "";
          if (f) this.on.setIsotherms([...doc.isotherms, f()]);
        },
      },
      h("option", { value: "" }, "Add an isotherm or alarm…"),
      h("option", { value: "above" }, "Above a temperature"),
      h("option", { value: "below" }, "Below a temperature"),
      h("option", { value: "interval" }, "Interval"),
      h("option", { value: "condensation" }, "Condensation / mould (humidity)"),
      h("option", { value: "insulation" }, "Insulation (thermal index)"),
    ),
  );
}

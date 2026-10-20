import type { Panel } from "../panel";
import { type Alarm, type Stats, api } from "../api";
import { SERIES_COLORS, lineChart } from "../charts";
import { fill, h } from "../dom";
import { alarmOn, coverageText, deltaInput, imageStats, kindName, tempInput } from "../panelWidgets";
import { type Doc, isArea, isPath, state } from "../state";
import { delta, fromKelvin, symbol, temp } from "../units";

export function renderMeasurements(this: Panel, doc: Doc | null): void {
  const el = this.els.measurements;
  if (!doc) {
    fill(el);
    return;
  }
  const unit = state.unit;
  const focused = el.contains(document.activeElement);
  const existing = el.querySelectorAll("tbody tr.m-row");
  const showCoverage = doc.isotherms.length > 0 && doc.measurements.some((m) => isArea(m.shape.kind));
  if (focused && existing.length === doc.measurements.length) {
    // Keep the input being edited; refresh numbers only.
    existing.forEach((tr, i) => {
      const s = doc.stats[i] ?? null;
      const cells = tr.querySelectorAll("td.n");
      cells[0]!.textContent = temp(s?.min, unit);
      cells[1]!.textContent = temp(s?.max, unit);
      cells[2]!.textContent = temp(s?.avg, unit);
      if (showCoverage && cells[3]) cells[3].textContent = coverageText(doc.coverage[i]);
      tr.classList.toggle("selected", state.selected === i);
      tr.classList.toggle("alarm", alarmOn(doc, i));
    });
    this.renderMeasurementDetail(doc);
    return;
  }
  if (doc.measurements.length === 0) {
    fill(
      el,
      h("p", { class: "muted" }, "Pick a tool in the toolbar and draw on the image. Polygon and polyline: click points, double-click or Enter to finish."),
      h("div", { class: "row" }, h("span", { class: "label" }, "Wand ±"), deltaInput(state.wandTolerance, (dk) => (state.wandTolerance = dk), "Magic wand tolerance"), h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit))),
      imageStats(doc, unit),
      h("div", { class: "measurement-detail" }),
    );
    return;
  }
  const rows = doc.measurements.map((m, i) => {
    const s = doc.stats[i] ?? null;
    return h(
      "tr",
      { class: `m-row ${state.selected === i ? "selected" : ""} ${alarmOn(doc, i) ? "alarm" : ""}`, onclick: () => this.on.select(i) },
      h(
        "td",
        {},
        h("input", {
          class: "label-input",
          value: m.label,
          "aria-label": "Measurement label",
          onchange: (e: Event) => this.on.renameMeasurement(i, (e.target as HTMLInputElement).value.trim() || m.label),
        }),
      ),
      h("td", { class: "n" }, temp(s?.min, unit)),
      h("td", { class: "n" }, temp(s?.max, unit)),
      h("td", { class: "n" }, temp(s?.avg, unit)),
      showCoverage ? h("td", { class: "n", title: "Share of the area inside the isotherms" }, coverageText(doc.coverage[i])) : null,
      h(
        "td",
        {},
        h(
          "button",
          {
            class: "icon",
            title: "Delete measurement",
            "aria-label": `Delete ${m.label}`,
            onclick: (e: Event) => {
              e.stopPropagation();
              this.on.deleteMeasurement(i);
            },
          },
          "×",
        ),
      ),
    );
  });
  fill(
    el,
    h(
      "table",
      { class: "measurements" },
      h("thead", {}, h("tr", {}, h("th", {}, "Label"), h("th", {}, "Min"), h("th", {}, "Max"), h("th", {}, "Avg"), showCoverage ? h("th", { title: "Share inside the isotherms" }, "Iso %") : null, h("th", {}))),
      h("tbody", {}, rows),
    ),
    doc.measurements.length >= 2 ? this.deltaT(doc) : null,
    h("div", { class: "row" }, h("span", { class: "label" }, "Wand ±"), deltaInput(state.wandTolerance, (dk) => (state.wandTolerance = dk), "New magic wand tolerance"), h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)), h("span", { class: "muted small" }, "for new wands")),
    imageStats(doc, unit),
    h("div", { class: "measurement-detail" }),
  );
  this.renderMeasurementDetail(doc);
}

/** Alarm, wand tolerance and profile for the selected measurement. */
export function renderMeasurementDetail(this: Panel, doc: Doc): void {
  const host = this.els.measurements.querySelector<HTMLElement>(".measurement-detail");
  if (!host) return;
  const i = state.selected;
  const m = i === null ? null : doc.measurements[i];
  if (i === null || !m) {
    fill(host);
    this.profileFor = "";
    return;
  }
  if (host.contains(document.activeElement)) return;
  const unit = state.unit;
  const alarm: Alarm = m.alarm ?? { stat: "max", above: null, below: null };
  const status = alarmOn(doc, i) ? h("span", { class: "alarm-badge" }, "ALARM") : m.alarm && (m.alarm.above != null || m.alarm.below != null) ? h("span", { class: "muted small" }, "OK") : null;
  const parts: (HTMLElement | null)[] = [
    h("div", { class: "detail-title" }, `${m.label} · ${kindName(m.shape.kind)}`),
    h(
      "div",
      { class: "row alarm-row" },
      h("span", { class: "label" }, "Alarm"),
      h(
        "select",
        { "aria-label": "Alarm statistic", onchange: (e: Event) => this.on.setAlarm(i, { ...alarm, stat: (e.target as HTMLSelectElement).value as Alarm["stat"] }) },
        (["max", "min", "avg"] as const).map((s) => h("option", { value: s, selected: alarm.stat === s }, s)),
      ),
      h("span", { class: "muted small" }, ">"),
      tempInput(alarm.above, (k) => this.on.setAlarm(i, { ...alarm, above: k }), "Alarm above"),
      h("span", { class: "muted small" }, "<"),
      tempInput(alarm.below, (k) => this.on.setAlarm(i, { ...alarm, below: k }), "Alarm below"),
      status,
    ),
  ];
  if (m.shape.kind === "wand") {
    parts.push(
      h(
        "div",
        { class: "row" },
        h("span", { class: "label" }, "Tolerance"),
        deltaInput(m.shape.tolerance, (dk) => this.on.setWandTolerance(i, dk), "Wand tolerance"),
        h("span", { class: "unit" }, unit === "K" ? "K" : symbol(unit)),
        h("span", { class: "muted small" }, `${doc.stats[i]?.count ?? 0} px`),
      ),
    );
  }
  const profile = isPath(m.shape.kind) ? h("div", { class: "profile" }, h("p", { class: "muted small" }, "Loading profile…")) : null;
  parts.push(profile);
  fill(host, parts);
  if (profile) {
    const key = `${doc.info.id}|${JSON.stringify(m.shape)}|${doc.kelvin.length}|${doc.stats[i]?.avg}|${unit}`;
    this.profileFor = key;
    void api.profile(doc.info.id, m.shape).then((points) => {
      if (this.profileFor !== key) return;
      fill(
        profile,
        lineChart([{ name: m.label, color: SERIES_COLORS[0]!, points: points.map((p) => [p.d, fromKelvin(p.k, unit)] as [number, number]) }], {
          xLabel: "Distance (px)",
          yLabel: symbol(unit),
          height: 110,
        }),
      );
    });
  }
}

export function deltaT(this: Panel, doc: Doc): HTMLElement {
  const n = doc.measurements.length;
  if (this.deltaA >= n) this.deltaA = 0;
  if (this.deltaB >= n) this.deltaB = Math.min(1, n - 1);
  const unit = state.unit;
  const pick = (value: number, set: (v: number) => void) =>
    h(
      "select",
      { onchange: (e: Event) => (set(Number((e.target as HTMLSelectElement).value)), this.renderMeasurements(doc)) },
      doc.measurements.map((m, i) => h("option", { value: String(i), selected: i === value }, m.label)),
    );
  const a: Stats | null = doc.stats[this.deltaA] ?? null;
  const b: Stats | null = doc.stats[this.deltaB] ?? null;
  return h(
    "div",
    { class: "delta" },
    h("span", { class: "label" }, "ΔT"),
    pick(this.deltaA, (v) => (this.deltaA = v)),
    h("span", { class: "muted" }, "−"),
    pick(this.deltaB, (v) => (this.deltaB = v)),
    h(
      "span",
      { class: "delta-values" },
      h("span", { title: "Difference of maxima" }, `max ${a && b ? delta(a.max - b.max, unit) : "–"}`),
      h("span", { title: "Difference of averages" }, `avg ${a && b ? delta(a.avg - b.avg, unit) : "–"}`),
    ),
  );
}

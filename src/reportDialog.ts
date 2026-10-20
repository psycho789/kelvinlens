// The "Create report" dialog: template, report fields, site conditions, format and images.
import type { Conditions, Paper, ReportFormat, ReportInfo, Template } from "./api";
import { h } from "./dom";
import { parseLastReport, type LastReport } from "./settings";
import { allTemplates, loadSettings, saveSettings, state, type Doc } from "./state";
import { inputValue, symbol, toKelvin } from "./units";

export interface ReportChoice {
  info: ReportInfo;
  docs: Doc[];
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function defaultPaper(): Paper {
  const saved = loadSettings().paper;
  if (saved) return saved;
  // US and Canada use Letter; nearly everyone else A4.
  return /-(US|CA)$/i.test(navigator.language) ? "us-letter" : "a4";
}

export function currentTemplate(): Template {
  const all = allTemplates();
  return all.find((t) => t.id === loadSettings().templateId) ?? all[0]!;
}

const LAST_KEY = "kelvinlens.lastReport.v1";

export function lastReport(): LastReport {
  const fallback: LastReport = { title: "Thermal inspection report", site: "", fields: {}, conditions: { insideK: null, outsideK: null, humidity: null, ambientK: null, basis: "similar" }, includePhotos: true };
  try {
    return parseLastReport(JSON.parse(localStorage.getItem(LAST_KEY) ?? "null"), fallback);
  } catch {
    return fallback;
  }
}

function rememberReport(r: LastReport): void {
  try {
    localStorage.setItem(LAST_KEY, JSON.stringify(r));
  } catch {
    // Not fatal.
  }
}

/** Report info for Rapid report: the last settings, today's date, the current template. */
export function rapidInfo(): ReportInfo {
  const last = lastReport();
  const s = loadSettings();
  return {
    title: last.title,
    site: last.site,
    inspector: s.inspector ?? "",
    date: today(),
    summary: "",
    paper: defaultPaper(),
    unit: state.unit,
    includePhotos: last.includePhotos,
    template: currentTemplate(),
    fields: last.fields,
    conditions: last.conditions,
    format: s.reportFormat ?? "pdf",
  };
}

export function reportDialog(onEditTemplates: () => Promise<void>): Promise<ReportChoice | null> {
  return new Promise((resolve) => {
    const settings = loadSettings();
    const last = lastReport();
    const fieldEl = (label: string, input: HTMLElement) => h("label", { class: "field" }, h("span", {}, label), input);
    const title = h("input", { type: "text", value: last.title, required: true });
    const site = h("input", { type: "text", value: last.site, placeholder: "Address, building, equipment" });
    const inspector = h("input", { type: "text", value: settings.inspector ?? "" });
    const date = h("input", { type: "date", value: today() });
    const summary = h("textarea", { rows: 3, placeholder: "Overall findings (optional)" });
    const paper = h(
      "select",
      {},
      h("option", { value: "a4", selected: defaultPaper() === "a4" }, "A4"),
      h("option", { value: "us-letter", selected: defaultPaper() === "us-letter" }, "US Letter"),
    );
    const format = h(
      "select",
      {},
      h("option", { value: "pdf", selected: (settings.reportFormat ?? "pdf") === "pdf" }, "PDF"),
      h("option", { value: "html", selected: settings.reportFormat === "html" }, "HTML (web page)"),
    );
    const photos = h("input", { type: "checkbox", checked: last.includePhotos });
    const templateSelect = h("select", {});
    const fillTemplates = () => {
      const current = currentTemplate().id;
      templateSelect.replaceChildren(...allTemplates().map((t) => h("option", { value: t.id, selected: t.id === current }, t.name)));
    };
    fillTemplates();

    // Template-dependent parts: cover fields and site conditions.
    const extra = h("div", {});
    const coverInputs = new Map<string, HTMLInputElement>();
    const cond = { ...last.conditions };
    const renderExtra = () => {
      const t = allTemplates().find((x) => x.id === templateSelect.value) ?? currentTemplate();
      coverInputs.clear();
      const inputs = t.coverFields.map((f) => {
        const input = h("input", { type: "text", value: last.fields[f] ?? "" });
        coverInputs.set(f, input);
        return fieldEl(f, input);
      });
      const tempField = (label: string, key: "insideK" | "outsideK" | "ambientK") =>
        fieldEl(
          `${label} (${symbol(state.unit)})`,
          h("input", {
            type: "number",
            step: "0.1",
            value: inputValue(cond[key], state.unit),
            onchange: (e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              cond[key] = v.trim() === "" ? null : toKelvin(Number(v), state.unit);
            },
          }),
        );
      const conditions: HTMLElement[] = [];
      if (t.kind === "building") {
        conditions.push(
          tempField("Indoor temperature", "insideK"),
          tempField("Outdoor temperature", "outsideK"),
          fieldEl(
            "Indoor relative humidity (%)",
            h("input", {
              type: "number",
              min: "0",
              max: "100",
              value: cond.humidity == null ? "" : String(Math.round(cond.humidity * 100)),
              onchange: (e: Event) => {
                const v = (e.target as HTMLInputElement).value;
                cond.humidity = v.trim() === "" ? null : Math.min(1, Math.max(0, Number(v) / 100));
              },
            }),
          ),
        );
      }
      if (t.kind === "electrical") {
        const basis = h(
          "select",
          { onchange: (e: Event) => (cond.basis = (e.target as HTMLSelectElement).value as Conditions["basis"]) },
          h("option", { value: "similar", selected: cond.basis === "similar" }, "Similar components (coolest measurement)"),
          h("option", { value: "ambient", selected: cond.basis === "ambient" }, "Ambient air temperature"),
        );
        conditions.push(fieldEl("Severity basis (NETA)", basis), tempField("Ambient temperature", "ambientK"));
      }
      extra.replaceChildren(
        inputs.length ? h("div", { class: "grid2" }, inputs) : "",
        conditions.length ? h("fieldset", { class: "plain" }, h("legend", {}, "Conditions"), h("div", { class: "grid2" }, conditions)) : "",
      );
    };
    templateSelect.addEventListener("change", renderExtra);
    renderExtra();

    const boxes = state.docs.map((doc) => {
      const box = h("input", { type: "checkbox", checked: doc.includeInReport });
      return { doc, box, row: h("label", { class: "check" }, box, `${doc.info.name}`, h("span", { class: "muted" }, ` · ${doc.measurements.length} measurements${doc.findings.length ? ` · ${doc.findings.length} findings` : ""}`)) };
    });

    const dialog = h(
      "dialog",
      { class: "report-dialog" },
      h(
        "form",
        { method: "dialog" },
        h("h2", {}, "Create report"),
        h(
          "div",
          { class: "row" },
          h("span", { class: "label wide" }, "Template"),
          templateSelect,
          h(
            "button",
            {
              type: "button",
              onclick: async () => {
                await onEditTemplates();
                fillTemplates();
                renderExtra();
              },
            },
            "Edit templates…",
          ),
        ),
        h("div", { class: "grid2" }, fieldEl("Title", title), fieldEl("Date", date), fieldEl("Site", site), fieldEl("Inspector", inspector)),
        extra,
        fieldEl("Summary", summary),
        h("div", { class: "grid2" }, fieldEl("Paper", paper), fieldEl("Format", format)),
        h("label", { class: "check" }, photos, "Include camera photos"),
        h("fieldset", {}, h("legend", {}, "Images"), boxes.map((b) => b.row)),
        h(
          "div",
          { class: "dialog-buttons" },
          h("button", { value: "cancel", formnovalidate: true }, "Cancel"),
          h("button", { value: "create", class: "primary" }, "Create…"),
        ),
      ),
    );

    dialog.addEventListener("close", () => {
      const ok = dialog.returnValue === "create";
      dialog.remove();
      if (!ok) return resolve(null);
      const docs = boxes.filter((b) => b.box.checked).map((b) => b.doc);
      for (const b of boxes) b.doc.includeInReport = b.box.checked;
      const template = allTemplates().find((x) => x.id === templateSelect.value) ?? currentTemplate();
      const fields: Record<string, string> = { ...last.fields };
      coverInputs.forEach((input, name) => (fields[name] = input.value));
      saveSettings({ inspector: inspector.value.trim(), paper: paper.value as Paper, templateId: template.id, reportFormat: format.value as ReportFormat });
      rememberReport({ title: title.value.trim(), site: site.value, fields, conditions: cond, includePhotos: photos.checked });
      resolve({
        info: {
          title: title.value.trim(),
          site: site.value,
          inspector: inspector.value,
          date: date.value,
          summary: summary.value,
          paper: paper.value as Paper,
          unit: state.unit,
          includePhotos: photos.checked,
          template,
          fields,
          conditions: cond,
          format: format.value as ReportFormat,
        },
        docs,
      });
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}

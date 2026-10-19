// The "Create report" dialog. Returns the report fields and the images to include.
import type { Paper, ReportInfo } from "./api";
import { h } from "./dom";
import { loadSettings, saveSettings, state, type Doc } from "./state";

export interface ReportChoice {
  info: ReportInfo;
  docs: Doc[];
}

function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function defaultPaper(): Paper {
  const saved = loadSettings().paper;
  if (saved) return saved;
  // US and Canada use Letter; nearly everyone else A4.
  return /-(US|CA)$/i.test(navigator.language) ? "us-letter" : "a4";
}

export function reportDialog(): Promise<ReportChoice | null> {
  return new Promise((resolve) => {
    const settings = loadSettings();
    const field = (label: string, input: HTMLElement) => h("label", { class: "field" }, h("span", {}, label), input);
    const title = h("input", { type: "text", value: "Thermal inspection report", required: true });
    const site = h("input", { type: "text", placeholder: "Address, building, equipment" });
    const inspector = h("input", { type: "text", value: settings.inspector ?? "" });
    const date = h("input", { type: "date", value: today() });
    const summary = h("textarea", { rows: 3, placeholder: "Overall findings (optional)" });
    const paper = h(
      "select",
      {},
      h("option", { value: "a4", selected: defaultPaper() === "a4" }, "A4"),
      h("option", { value: "us-letter", selected: defaultPaper() === "us-letter" }, "US Letter"),
    );
    const photos = h("input", { type: "checkbox", checked: true });
    const boxes = state.docs.map((doc) => {
      const box = h("input", { type: "checkbox", checked: doc.includeInReport });
      return { doc, box, row: h("label", { class: "check" }, box, `${doc.info.name}`, h("span", { class: "muted" }, ` · ${doc.measurements.length} measurements`)) };
    });

    const dialog = h(
      "dialog",
      { class: "report-dialog" },
      h(
        "form",
        { method: "dialog" },
        h("h2", {}, "Create PDF report"),
        h("div", { class: "grid2" }, field("Title", title), field("Date", date), field("Site", site), field("Inspector", inspector)),
        field("Summary", summary),
        h("div", { class: "grid2" }, field("Paper", paper), h("label", { class: "check" }, photos, "Include camera photos")),
        h("fieldset", {}, h("legend", {}, "Images"), boxes.map((b) => b.row)),
        h(
          "div",
          { class: "dialog-buttons" },
          h("button", { value: "cancel", formnovalidate: true }, "Cancel"),
          h("button", { value: "create", class: "primary" }, "Create PDF…"),
        ),
      ),
    );

    dialog.addEventListener("close", () => {
      const ok = dialog.returnValue === "create";
      dialog.remove();
      if (!ok) return resolve(null);
      const docs = boxes.filter((b) => b.box.checked).map((b) => b.doc);
      for (const b of boxes) b.doc.includeInReport = b.box.checked;
      saveSettings({ inspector: inspector.value.trim(), paper: paper.value as Paper });
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
        },
        docs,
      });
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}

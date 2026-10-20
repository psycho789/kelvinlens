import type { Panel } from "../panel";
import { fill, h } from "../dom";
import { type Doc, allTemplates, loadSettings, state } from "../state";

export function renderInspection(this: Panel, doc: Doc | null): void {
  const el = this.els.inspection;
  if (!doc) {
    fill(el);
    return;
  }
  if (el.contains(document.activeElement)) return;
  const template = allTemplates().find((t) => t.id === loadSettings().templateId) ?? allTemplates()[0];
  const fieldNames = [...new Set([...(template?.imageFields ?? []), ...Object.keys(doc.fields)])];
  const assets = [...new Set(state.docs.map((d) => d.asset).filter(Boolean))];
  fill(
    el,
    h(
      "div",
      { class: "row" },
      h("label", { for: "asset" }, "Asset"),
      h("input", { id: "asset", type: "text", list: "assets", value: doc.asset, placeholder: "e.g. Panel B, breaker 3", title: "Images with the same asset form a trend over visits", onchange: (e: Event) => this.on.setAsset((e.target as HTMLInputElement).value.trim()) }),
      h("datalist", { id: "assets" }, assets.map((a) => h("option", { value: a }))),
    ),
    fieldNames.map((name) =>
      h(
        "div",
        { class: "row" },
        h("label", { title: name }, name),
        h("input", { type: "text", value: doc.fields[name] ?? "", "aria-label": name, onchange: (e: Event) => this.on.setField(name, (e.target as HTMLInputElement).value) }),
      ),
    ),
    template ? h("p", { class: "muted small" }, `Fields from the "${template.name}" report template.`) : null,
    doc.annotations.length
      ? h(
          "div",
          { class: "annotations" },
          h("div", { class: "detail-title" }, "Notes and sketches on the image"),
          doc.annotations.map((a, i) =>
            h(
              "div",
              { class: "row" },
              a.kind === "text"
                ? h("input", { type: "text", value: a.text, "aria-label": "Note text", onchange: (e: Event) => this.on.setAnnotationText(i, (e.target as HTMLInputElement).value) })
                : h("span", { class: "muted" }, `Sketch, ${a.points.length} points`),
              h("button", { class: "icon", title: "Remove", "aria-label": "Remove", onclick: () => this.on.deleteAnnotation(i) }, "×"),
            ),
          ),
        )
      : h("p", { class: "muted small" }, "Use the Note and Sketch tools to annotate the image."),
  );
}

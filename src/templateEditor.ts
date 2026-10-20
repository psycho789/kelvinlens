// Report template editor: duplicate a built-in template and change its branding, fields and
// sections. Custom templates are kept in the app's settings and can be exported and imported.
import { pickers } from "./pickers";
import { api, type Template, type TemplateKind } from "./api";
import { errorText, fill, h, toast } from "./dom";
import { field } from "./modal";
import { allTemplates, loadSettings, saveSettings, state } from "./state";

const SECTION_NAMES: Record<string, string> = {
  isotherms: "Isotherms and alarms",
  measurements: "Measurements",
  formulas: "Formulas",
  findings: "Findings (auto-inspect)",
  electrical: "Temperature rise (NETA)",
  building: "Insulation and moisture",
  hvac: "Temperature split (HVAC)",
  profiles: "Profiles",
  fields: "Inspection details",
  notes: "Notes",
  parameters: "Parameters",
  details: "Image details",
};

function persist(): void {
  saveSettings({ customTemplates: state.customTemplates });
}

function uniqueId(base: string): string {
  const ids = new Set(allTemplates().map((t) => t.id));
  let id = base;
  for (let i = 2; ids.has(id); i++) id = `${base}-${i}`;
  return id;
}

export function templateEditor(): Promise<void> {
  return new Promise((resolve) => {
    let selected = loadSettings().templateId ?? allTemplates()[0]!.id;
    const list = h("ul", { class: "template-list" });
    const form = h("div", { class: "template-form" });

    const builtin = (t: Template) => state.builtinTemplates.some((b) => b.id === t.id);
    const current = () => allTemplates().find((t) => t.id === selected) ?? allTemplates()[0]!;

    const update = (patch: Partial<Template>) => {
      const t = current();
      if (builtin(t)) return;
      Object.assign(t, patch);
      persist();
      renderList();
    };

    const duplicate = () => {
      const t = structuredClone(current());
      t.id = uniqueId(`custom-${t.id.replace(/^custom-/, "")}`);
      t.name = `${t.name} (copy)`;
      state.customTemplates.push(t);
      persist();
      selected = t.id;
      render();
    };

    const renderList = () => {
      fill(
        list,
        allTemplates().map((t) =>
          h(
            "li",
            {
              class: t.id === selected ? "active" : "",
              onclick: () => {
                selected = t.id;
                render();
              },
            },
            t.name,
            builtin(t) ? h("span", { class: "muted small" }, " · built in") : null,
          ),
        ),
      );
    };

    const renderForm = () => {
      const t = current();
      const ro = builtin(t);
      const text = (value: string, set: (v: string) => void, placeholder = "") =>
        h("input", { type: "text", value, placeholder, disabled: ro, onchange: (e: Event) => set((e.target as HTMLInputElement).value) });
      const lines = (value: string[], set: (v: string[]) => void, placeholder: string) =>
        h("textarea", {
          rows: 3,
          disabled: ro,
          placeholder,
          value: value.join("\n"),
          onchange: (e: Event) => set((e.target as HTMLTextAreaElement).value.split("\n").map((s) => s.trim()).filter(Boolean)),
        });
      const order = [...t.imageSections, ...Object.keys(SECTION_NAMES).filter((s) => !t.imageSections.includes(s))];
      const sectionRows = order.map((name) => {
        const on = t.imageSections.includes(name);
        const move = (d: number) => {
          const arr = [...t.imageSections];
          const i = arr.indexOf(name);
          const j = i + d;
          if (i < 0 || j < 0 || j >= arr.length) return;
          [arr[i], arr[j]] = [arr[j]!, arr[i]!];
          update({ imageSections: arr });
          renderForm();
        };
        return h(
          "div",
          { class: "row section-row" },
          h(
            "label",
            { class: "check" },
            h("input", {
              type: "checkbox",
              checked: on,
              disabled: ro,
              onchange: (e: Event) => {
                const checked = (e.target as HTMLInputElement).checked;
                update({ imageSections: checked ? [...t.imageSections, name] : t.imageSections.filter((s) => s !== name) });
                renderForm();
              },
            }),
            SECTION_NAMES[name] ?? name,
          ),
          h("span", { class: "spacer" }),
          on && !ro ? h("button", { type: "button", class: "icon", title: "Move up", onclick: () => move(-1) }, "↑") : null,
          on && !ro ? h("button", { type: "button", class: "icon", title: "Move down", onclick: () => move(1) }, "↓") : null,
        );
      });
      const check = (key: keyof Template["sections"], label: string) =>
        h(
          "label",
          { class: "check" },
          h("input", { type: "checkbox", checked: t.sections[key], disabled: ro, onchange: (e: Event) => update({ sections: { ...t.sections, [key]: (e.target as HTMLInputElement).checked } }) }),
          label,
        );
      fill(
        form,
        ro ? h("p", { class: "notice" }, "Built-in templates can't be changed. Duplicate this one to make your own.") : null,
        h("div", { class: "grid2" }, field("Name", text(t.name, (v) => update({ name: v.trim() || t.name }))), field("Trade", h(
          "select",
          { disabled: ro, onchange: (e: Event) => update({ kind: (e.target as HTMLSelectElement).value as TemplateKind }) },
          (["general", "electrical", "building", "hvac"] as const).map((k) => h("option", { value: k, selected: t.kind === k }, { general: "General", electrical: "Electrical (NETA)", building: "Building / moisture", hvac: "HVAC" }[k])),
        ))),
        h("div", { class: "grid2" },
          field("Company", text(t.company, (v) => update({ company: v }), "Shown in the header")),
          field("Accent colour", h("input", { type: "color", value: t.accent, disabled: ro, onchange: (e: Event) => update({ accent: (e.target as HTMLInputElement).value }) })),
        ),
        h(
          "div",
          { class: "row" },
          h("span", { class: "label wide" }, "Logo"),
          t.logo ? h("img", { src: t.logo, class: "logo-preview", alt: "Logo" }) : h("span", { class: "muted small" }, "None"),
          h(
            "button",
            {
              type: "button",
              disabled: ro,
              onclick: async () => {
                const path = await pickers.open({ filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }] });
                if (typeof path !== "string") return;
                try {
                  update({ logo: await api.readImageDataUrl(path) });
                  renderForm();
                } catch (e) {
                  toast(errorText(e), "error");
                }
              },
            },
            "Choose…",
          ),
          t.logo && !ro ? h("button", { type: "button", onclick: () => (update({ logo: null }), renderForm()) }, "Remove") : null,
        ),
        h("div", { class: "grid2" },
          field("Cover fields (one per line)", lines(t.coverFields, (v) => update({ coverFields: v }), "Customer\nJob number")),
          field("Fields per image (one per line)", lines(t.imageFields, (v) => update({ imageFields: v }), "Component\nLoad %")),
        ),
        h("fieldset", { class: "plain" }, h("legend", {}, "Report sections"), h("div", { class: "checks wrap" }, check("summary", "Summary"), check("contents", "Contents"), check("findingsOverview", "Findings overview"), check("trends", "Trends"), check("signature", "Signature"))),
        h("fieldset", { class: "plain" }, h("legend", {}, "For each image, in this order"), sectionRows),
        h("div", { class: "grid2" },
          field("Images per page", h("select", { disabled: ro, onchange: (e: Event) => update({ imagesPerPage: Number((e.target as HTMLSelectElement).value) }) }, h("option", { value: "1", selected: t.imagesPerPage !== 2 }, "1"), h("option", { value: "2", selected: t.imagesPerPage === 2 }, "2"))),
          field("Footer text", text(t.footer, (v) => update({ footer: v }))),
        ),
        field("Disclaimer (empty: the standard accuracy note)", h("textarea", { rows: 2, disabled: ro, value: t.disclaimer, onchange: (e: Event) => update({ disclaimer: (e.target as HTMLTextAreaElement).value }) })),
      );
    };

    const render = () => {
      renderList();
      renderForm();
      deleteButton.disabled = builtin(current());
      exportButton.disabled = false;
    };

    const deleteButton = h(
      "button",
      {
        type: "button",
        onclick: () => {
          const t = current();
          if (builtin(t)) return;
          state.customTemplates = state.customTemplates.filter((x) => x.id !== t.id);
          persist();
          selected = allTemplates()[0]!.id;
          render();
        },
      },
      "Delete",
    );
    const exportButton = h(
      "button",
      {
        type: "button",
        onclick: async () => {
          const t = current();
          const path = await pickers.save({ defaultPath: `${t.name.replace(/[\\/:*?"<>|]+/g, "-")}.kelvinlens-template.json`, filters: [{ name: "Template", extensions: ["json"] }] });
          if (!path) return;
          try {
            await api.writeText(path, JSON.stringify({ kelvinlensTemplate: 1, template: t }, null, 2) + "\n");
            toast("Template exported");
          } catch (e) {
            toast(errorText(e), "error");
          }
        },
      },
      "Export…",
    );
    const importButton = h(
      "button",
      {
        type: "button",
        onclick: async () => {
          const path = await pickers.open({ filters: [{ name: "Template", extensions: ["json"] }] });
          if (typeof path !== "string") return;
          try {
            const data = JSON.parse(await api.readText(path)) as { template?: Partial<Template> };
            const base = structuredClone(state.builtinTemplates[0]!);
            const t: Template = { ...base, ...(data.template ?? {}), sections: { ...base.sections, ...(data.template?.sections ?? {}) } } as Template;
            if (typeof t.name !== "string" || !Array.isArray(t.imageSections)) throw new Error("Not a Kelvinlens template");
            t.id = uniqueId(`custom-${(t.id || "imported").replace(/^custom-/, "")}`);
            state.customTemplates.push(t);
            persist();
            selected = t.id;
            render();
            toast(`Imported "${t.name}"`);
          } catch (e) {
            toast(`Could not import: ${errorText(e)}`, "error");
          }
        },
      },
      "Import…",
    );

    const dialog = h(
      "dialog",
      { class: "report-dialog wide" },
      h(
        "form",
        { method: "dialog" },
        h("h2", {}, "Report templates"),
        h("div", { class: "template-editor" }, h("div", {}, list, h("div", { class: "row buttons wrap" }, h("button", { type: "button", onclick: duplicate }, "Duplicate"), deleteButton, importButton, exportButton)), form),
        h(
          "div",
          { class: "dialog-buttons" },
          h(
            "button",
            {
              value: "use",
              onclick: () => saveSettings({ templateId: selected }),
            },
            "Use this template",
          ),
          h("button", { value: "done", class: "primary" }, "Done"),
        ),
      ),
    );
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve();
    });
    document.body.append(dialog);
    render();
    dialog.showModal();
  });
}

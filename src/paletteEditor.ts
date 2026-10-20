// Custom palette editor: colour stops along the scale, interpolated to 256 colours the same way
// as the built-in palettes (palette.rs `stops_to_colors`).
import type { Palette } from "./api";
import { fill, h } from "./dom";
import { field } from "./modal";
import { gradientCss } from "./panel";
import { saveSettings, state } from "./state";

type Stop = [number, [number, number, number]];

export function stopsToColors(stops: Stop[]): [number, number, number][] {
  const s = [...stops].sort((a, b) => a[0] - b[0]);
  if (s.length === 0) return Array.from({ length: 256 }, () => [128, 128, 128]);
  if (s.length === 1) return Array.from({ length: 256 }, () => s[0]![1]);
  return Array.from({ length: 256 }, (_, i) => {
    const t = i / 255;
    let upper = s.findIndex(([at]) => at >= t);
    if (upper < 0) upper = s.length - 1;
    upper = Math.max(1, upper);
    const [a, ca] = s[upper - 1]!;
    const [b, cb] = s[upper]!;
    const f = b > a ? Math.min(1, Math.max(0, (t - a) / (b - a))) : 0;
    return [0, 1, 2].map((c) => Math.round(ca[c]! + (cb[c]! - ca[c]!) * f)) as [number, number, number];
  });
}

/** Stops stored with a custom palette so it can be edited again. */
type CustomPalette = Palette & { stops?: Stop[] };

function hex(c: [number, number, number]): string {
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function rgb(s: string): [number, number, number] {
  const n = parseInt(s.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function paletteEditor(): Promise<string | null> {
  return new Promise((resolve) => {
    const palettes = state.customPalettes as CustomPalette[];
    let selected: string | null = palettes[0]?.id ?? null;
    const list = h("ul", { class: "template-list" });
    const form = h("div", { class: "template-form" });

    const persist = () => saveSettings({ customPalettes: state.customPalettes });
    const current = () => palettes.find((p) => p.id === selected) ?? null;

    const create = () => {
      let n = 1;
      while (palettes.some((p) => p.id === `custom-${n}`)) n++;
      const stops: Stop[] = [
        [0, [10, 10, 40]],
        [0.5, [200, 40, 120]],
        [1, [255, 240, 180]],
      ];
      const p: CustomPalette = { id: `custom-${n}`, name: `Custom ${n}`, colors: stopsToColors(stops), stops };
      palettes.push(p);
      selected = p.id;
      persist();
      render();
    };

    const render = () => {
      fill(
        list,
        palettes.map((p) =>
          h(
            "li",
            { class: p.id === selected ? "active" : "", onclick: () => ((selected = p.id), render()) },
            h("span", { class: "mini-swatch", style: `background:${gradientCss(p.colors)}` }),
            p.name,
          ),
        ),
        palettes.length === 0 ? h("li", { class: "muted" }, "No custom palettes yet") : null,
      );
      const p = current();
      if (!p) {
        fill(form, h("p", { class: "muted" }, "Create a palette from colour stops; it appears under Custom in the palette menu."));
        return;
      }
      const stops: Stop[] = p.stops ?? [
        [0, p.colors[0]!],
        [1, p.colors[255]!],
      ];
      const apply = (next: Stop[]) => {
        p.stops = next.sort((a, b) => a[0] - b[0]);
        p.colors = stopsToColors(p.stops);
        persist();
        render();
      };
      fill(
        form,
        field("Name", h("input", { type: "text", value: p.name, onchange: (e: Event) => ((p.name = (e.target as HTMLInputElement).value.trim() || p.name), persist(), render()) })),
        h("div", { class: "swatch big", style: `background:${gradientCss(p.colors)}` }),
        h("div", { class: "detail-title" }, "Colour stops (0 % is the cold end)"),
        stops.map(([at, c], i) =>
          h(
            "div",
            { class: "row" },
            h("input", { type: "color", value: hex(c), "aria-label": "Stop colour", onchange: (e: Event) => apply(stops.map((s, k) => (k === i ? [s[0], rgb((e.target as HTMLInputElement).value)] : s))) }),
            h("input", {
              type: "number",
              class: "num small-num",
              min: "0",
              max: "100",
              value: String(Math.round(at * 100)),
              "aria-label": "Stop position in percent",
              onchange: (e: Event) => {
                const v = Math.min(100, Math.max(0, Number((e.target as HTMLInputElement).value) || 0)) / 100;
                apply(stops.map((s, k) => (k === i ? [v, s[1]] : s)));
              },
            }),
            h("span", { class: "muted small" }, "%"),
            stops.length > 2 ? h("button", { type: "button", class: "icon", title: "Remove stop", onclick: () => apply(stops.filter((_, k) => k !== i)) }, "×") : null,
          ),
        ),
        h(
          "div",
          { class: "row buttons" },
          h(
            "button",
            {
              type: "button",
              onclick: () => {
                // Add a stop halfway along the widest gap.
                let best = 0;
                for (let i = 1; i < stops.length; i++) if (stops[i]![0] - stops[i - 1]![0] > stops[best + 1]![0] - stops[best]![0]) best = i - 1;
                const at = (stops[best]![0] + stops[best + 1]![0]) / 2;
                apply([...stops, [at, p.colors[Math.round(at * 255)]!]]);
              },
            },
            "Add stop",
          ),
          h("button", { type: "button", onclick: () => apply(stops.map(([at, c]) => [1 - at, c] as Stop)) }, "Reverse"),
          h(
            "button",
            {
              type: "button",
              onclick: () => {
                palettes.splice(palettes.indexOf(p), 1);
                selected = palettes[0]?.id ?? null;
                persist();
                render();
              },
            },
            "Delete palette",
          ),
        ),
      );
    };

    const dialog = h(
      "dialog",
      { class: "report-dialog wide" },
      h(
        "form",
        { method: "dialog" },
        h("h2", {}, "Custom palettes"),
        h("div", { class: "template-editor" }, h("div", {}, list, h("div", { class: "row buttons" }, h("button", { type: "button", onclick: create }, "New palette"))), form),
        h("div", { class: "dialog-buttons" }, h("button", { value: "use" }, "Use selected"), h("button", { value: "done", class: "primary" }, "Done")),
      ),
    );
    dialog.addEventListener("close", () => {
      const v = dialog.returnValue;
      dialog.remove();
      resolve(v === "use" ? selected : null);
    });
    document.body.append(dialog);
    render();
    dialog.showModal();
  });
}

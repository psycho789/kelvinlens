import { panorama, runInspect, triage } from "./analysis";
import { mapDialog, trendDialog } from "./analysisDialogs";
import { selectDoc } from "./docs";
import { $, h } from "./dom";
import { reliefDialog } from "./relief3d";
import { createReport, rapidReport } from "./reports";
import { exportCsv, exportFlirCopy, exportImage, exportSettings, exportTiff, importSettings } from "./saveExport";
import { type Tool, currentDoc } from "./state";
import { templateEditor } from "./templateEditor";
import { panel } from "./ui";
import { UNITS, symbol } from "./units";

export const TOOLS: [Tool, string, string, string][] = [
  ["select", "Select", "V", "M4 3l14 8-6 1.6L9.5 19z"],
  ["spot", "Spot", "S", "M12 5v4M12 15v4M5 12h4M15 12h4M12 12m-2.2 0a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0"],
  ["box", "Box", "B", "M5 6h14v12H5z"],
  ["ellipse", "Ellipse", "E", "M12 6c4.4 0 8 2.7 8 6s-3.6 6-8 6-8-2.7-8-6 3.6-6 8-6z"],
  ["line", "Line", "L", "M5 18L19 6"],
  ["polygon", "Polygon", "P", "M6 7l9-3 5 8-6 8-9-4z"],
  ["polyline", "Polyline", "Y", "M4 17l5-8 5 6 6-9"],
  ["wand", "Wand", "W", "M5 19L15 9M15 4v2M19 8h2M17.5 5.5l1.5-1.5M13 7l4 4"],
  ["text", "Note", "N", "M5 5h14v10H10l-4 4v-4H5z"],
  ["sketch", "Sketch", "K", "M4 18c3-6 5 2 8-4s4-6 8-8"],
];

export function icon(path: string): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", path);
  svg.append(p);
  return svg;
}

export type MenuItem = [label: string, action: () => void, enabled?: () => boolean] | "-";

/** Closes every open toolbar menu and resets its button state. */
export function closeMenus(): void {
  document.querySelectorAll(".menu.open").forEach((m) => {
    m.classList.remove("open");
    m.querySelector(".menu-button")?.setAttribute("aria-expanded", "false");
  });
}

/** A toolbar button with a drop-down list of actions. */
export function menu(id: string, label: string, items: MenuItem[], primary = false): HTMLElement {
  const list = h("div", { class: "menu-list" });
  const button = h("button", { id, class: `menu-button ${primary ? "primary" : ""}`, "aria-expanded": "false" }, `${label} ▾`);
  const wrap = h("div", { class: "menu" }, button, list);
  button.addEventListener("click", (e) => {
    e.stopPropagation();
    const opening = !wrap.classList.contains("open");
    closeMenus();
    if (!opening) return;
    list.replaceChildren(
      ...items.map((it) =>
        it === "-"
          ? h("hr")
          : h(
              "button",
              {
                disabled: it[2] ? !it[2]() : false,
                onclick: () => {
                  closeMenus();
                  it[1]();
                },
              },
              it[0],
            ),
      ),
    );
    wrap.classList.add("open");
    button.setAttribute("aria-expanded", "true");
  });
  return wrap;
}

const hasDoc = () => !!currentDoc();

export function buildLayout(): void {
  document.addEventListener("click", closeMenus);
  const app = $("#app");
  app.append(
    h(
      "header",
      { class: "toolbar" },
      h("div", { class: "brand" }, h("img", { src: "/icon.png", alt: "" }), h("span", {}, "Kelvinlens")),
      h(
        "div",
        { class: "group" },
        h("button", { id: "open", title: "Open images or videos (Ctrl/⌘ O)" }, "Open"),
        h("button", { id: "open-folder", title: "Open every image in a folder (Ctrl/⌘ Shift O)" }, "Folder"),
      ),
      h(
        "div",
        { class: "group tools", role: "toolbar", "aria-label": "Tools" },
        TOOLS.map(([tool, label, key, path]) =>
          h("button", { class: "tool", "data-tool": tool, "aria-pressed": "false", title: `${label} (${key})`, "aria-label": label }, icon(path), h("span", {}, label)),
        ),
      ),
      h("div", { class: "spacer" }),
      h(
        "div",
        { class: "group" },
        menu("menu-analyze", "Analyze", [
          ["Auto-inspect this image", () => void runInspect(), hasDoc],
          ["Hotspot triage…", () => void triage()],
          ["Panorama…", () => void panorama()],
          ["Trends…", () => void trendDialog()],
          ["Map of positions…", () => void mapDialog((i) => selectDoc(i))],
          ["3D relief…", () => void (currentDoc() && reliefDialog(currentDoc()!)), hasDoc],
        ]),
        h("button", { id: "batch", title: "Apply settings and export many images at once" }, "Batch…"),
        h("button", { id: "save", title: "Save edits next to the image (Ctrl/⌘ S)" }, "Save"),
        menu("menu-export", "Export", [
          ["Image (PNG/JPEG)…", () => void exportImage(), hasDoc],
          ["Temperatures (CSV)…", () => void exportCsv(), hasDoc],
          ["Radiometric TIFF…", () => void exportTiff(), hasDoc],
          ["FLIR copy with these parameters…", () => void exportFlirCopy(), () => !!currentDoc()?.info.flirJpeg],
          ["Map (KML/GeoJSON)…", () => void mapDialog((i) => selectDoc(i))],
          "-",
          ["Export settings (JSON)…", () => void exportSettings(), hasDoc],
          ["Import settings (JSON)…", () => void importSettings(), hasDoc],
        ]),
        menu(
          "menu-report",
          "Report",
          [
            ["Create report…", () => void createReport(), hasDoc],
            ["Rapid report (one click)", () => void rapidReport(), hasDoc],
            "-",
            ["Edit templates…", () => void templateEditor().then(() => panel.renderInspection(currentDoc()))],
          ],
          true,
        ),
      ),
      h("div", { class: "segmented units", role: "group", "aria-label": "Temperature unit" }, UNITS.map((u) => h("button", { "data-unit": u }, symbol(u)))),
    ),
    h("aside", { class: "sidebar" }, h("div", { class: "sidebar-head" }, h("h2", {}, "Images"), h("span", { id: "count", class: "muted" })), h("ul", { id: "files", class: "files" })),
    h(
      "main",
      { class: "stage" },
      h("div", { id: "viewer", class: "viewer" }),
      h(
        "div",
        { id: "empty", class: "empty" },
        h("img", { src: "/icon.png", alt: "", class: "empty-icon" }),
        h("h1", {}, "Open a thermal image"),
        h("p", {}, "Drop files or a folder here, or use Open. Everything stays on this computer."),
        h("div", { class: "row buttons" }, h("button", { class: "primary", id: "empty-open" }, "Open images…"), h("button", { id: "empty-folder" }, "Open folder…")),
        h("p", { class: "muted small" }, "FLIR radiometric JPEG and SEQ/CSQ video · DJI Zenmuse XT2/XTR · Fluke IS2 · InfiRay P2 Pro JPEG · InfiRay/Topdon IRG · HTI · 16-bit PNG and TIFF"),
      ),
      h("div", { id: "drop", class: "drop-overlay" }, h("div", {}, "Drop to open")),
      h("div", { id: "tool-hint", class: "tool-hint", hidden: true }),
      h(
        "div",
        { class: "zoombar" },
        h("button", { id: "zoom-out", title: "Zoom out (−)", "aria-label": "Zoom out" }, "−"),
        h("button", { id: "zoom-fit", title: "Fit to window (F)" }, "100%"),
        h("button", { id: "zoom-in", title: "Zoom in (+)", "aria-label": "Zoom in" }, "+"),
      ),
    ),
    h("aside", { id: "panel", class: "panel" }),
    h("footer", { class: "statusbar" }, h("span", { id: "readout", class: "readout" }), h("span", { id: "status-file", class: "muted" }), h("span", { class: "spacer" }), h("span", { id: "busy", class: "muted" })),
  );
}

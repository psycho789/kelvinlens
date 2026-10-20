// Analysis windows: hotspot triage over a folder, panoramas, trends across visits, a map of
// image positions, and measurements over video time.
import { pickers } from "./pickers";
import { api, type Series, type Stats, type TriageRow } from "./api";
import { SERIES_COLORS, lineChart } from "./charts";
import { errorText, fill, h, toast } from "./dom";
import { modal } from "./modal";
import { state, type Doc } from "./state";
import { fromKelvin, inputValue, symbol, temp, toKelvin } from "./units";

function csvCell(s: string): string {
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

async function saveText(defaultPath: string, name: string, ext: string, text: string): Promise<void> {
  const path = await pickers.save({ defaultPath, filters: [{ name, extensions: [ext] }] });
  if (!path) return;
  try {
    await api.writeText(path, text);
    toast(`Saved ${path.split(/[\\/]/).pop()}`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---------------------------------------------------------------------------- triage

/** Scans a folder (or the open images) and lists the hottest; returns paths to open. */
export async function triageDialog(): Promise<string[]> {
  const unit = state.unit;
  let paths: string[] = [];
  let rows: TriageRow[] = [];
  let sortBy: "max" | "min" | "name" = "max";
  const threshold = h("input", { type: "number", step: "0.1", class: "num", value: inputValue(toKelvin(60, "C"), unit) });
  const mode = h("select", {}, h("option", { value: "above" }, "hotter than"), h("option", { value: "below" }, "colder than"));
  const source = h("p", { class: "muted small" }, "Choose a folder, or scan the images already open.");
  const results = h("div", { class: "triage-results" });

  const render = () => {
    const sorted = [...rows].sort((a, b) => (sortBy === "name" ? a.name.localeCompare(b.name) : sortBy === "max" ? (b.maxK ?? -1e9) - (a.maxK ?? -1e9) : (a.minK ?? 1e9) - (b.minK ?? 1e9)));
    const flagged = rows.filter((r) => r.flagged).length;
    fill(
      results,
      rows.length ? h("p", {}, `${flagged} of ${rows.length} image${rows.length > 1 ? "s" : ""} flagged`) : null,
      rows.length
        ? h(
            "table",
            { class: "measurements triage" },
            h(
              "thead",
              {},
              h(
                "tr",
                {},
                h("th", {}, ""),
                h("th", { class: "sortable", onclick: () => ((sortBy = "name"), render()) }, "Image"),
                h("th", { class: "sortable", onclick: () => ((sortBy = "max"), render()) }, "Max ▾"),
                h("th", { class: "sortable", onclick: () => ((sortBy = "min"), render()) }, "Min ▴"),
              ),
            ),
            h(
              "tbody",
              {},
              sorted.map((r) =>
                h(
                  "tr",
                  { class: r.flagged ? "alarm" : "" },
                  h("td", {}, r.flagged ? h("span", { class: "alarm-badge" }, "!") : ""),
                  h("td", { title: r.path }, r.name, r.error ? h("span", { class: "muted small" }, ` · ${r.error}`) : ""),
                  h("td", { class: "n" }, temp(r.maxK, unit)),
                  h("td", { class: "n" }, temp(r.minK, unit)),
                ),
              ),
            ),
          )
        : null,
    );
  };

  const run = async () => {
    const t = toKelvin(Number(threshold.value), unit);
    if (!Number.isFinite(t)) return;
    if (paths.length === 0) paths = state.docs.map((d) => d.info.path);
    fill(results, h("p", { class: "muted" }, "Scanning…"));
    try {
      rows = await api.triage(paths, mode.value === "above" ? { aboveK: t, belowK: null } : { aboveK: null, belowK: t });
    } catch (e) {
      toast(errorText(e), "error");
      rows = [];
    }
    render();
  };

  const choice = await modal(
    "Hotspot triage",
    [
      h(
        "div",
        { class: "row buttons" },
        h(
          "button",
          {
            type: "button",
            onclick: async () => {
              const dir = await pickers.open({ directory: true });
              if (typeof dir !== "string") return;
              paths = [dir];
              source.textContent = dir;
              void run();
            },
          },
          "Choose folder…",
        ),
        h(
          "button",
          {
            type: "button",
            disabled: state.docs.length === 0,
            onclick: () => {
              paths = state.docs.map((d) => d.info.path);
              source.textContent = `${paths.length} open image${paths.length > 1 ? "s" : ""}`;
              void run();
            },
          },
          "Open images",
        ),
      ),
      source,
      h("div", { class: "row" }, h("span", { class: "label wide" }, "Flag images"), mode, threshold, h("span", { class: "unit" }, symbol(unit)), h("button", { type: "button", onclick: () => void run() }, "Scan")),
      results,
    ],
    [
      { label: "Close", value: "close" },
      { label: "Export CSV…", value: "csv" },
      { label: "Open flagged", value: "open", primary: true },
    ],
    { wide: true },
  );
  if (choice === "csv" && rows.length) {
    const text = ["image,path,max,min,flagged,captured", ...rows.map((r) => [csvCell(r.name), csvCell(r.path), r.maxK != null ? fromKelvin(r.maxK, unit).toFixed(2) : "", r.minK != null ? fromKelvin(r.minK, unit).toFixed(2) : "", r.flagged ? "yes" : "no", csvCell(r.dateTime ?? "")].join(","))].join("\n") + "\n";
    await saveText(`triage-${symbol(unit).replace("°", "")}.csv`, "CSV", "csv", text);
    return [];
  }
  return choice === "open" ? rows.filter((r) => r.flagged).map((r) => r.path) : [];
}

// ---------------------------------------------------------------------------- panorama

export interface PanoramaRequest {
  ids: number[];
  offsets: ([number, number] | null)[];
}

/** Picks images and their order for a panorama. */
export async function panoramaDialog(): Promise<PanoramaRequest | null> {
  const candidates = state.docs.filter((d) => !d.video && d.info.format !== "panorama");
  if (candidates.length < 2) {
    toast("Open at least two overlapping images (in order, left to right or top to bottom) to stitch a panorama.", "error");
    return null;
  }
  const order = candidates.map((d) => ({ doc: d, on: true, x: "", y: "" }));
  const list = h("div", { class: "pano-list" });
  const render = () =>
    fill(
      list,
      order.map((o, i) =>
        h(
          "div",
          { class: "row" },
          h("input", { type: "checkbox", checked: o.on, "aria-label": `Use ${o.doc.info.name}`, onchange: (e: Event) => (o.on = (e.target as HTMLInputElement).checked) }),
          h("span", { class: "pano-name", title: o.doc.info.path }, `${o.doc.info.name} (${o.doc.info.width}×${o.doc.info.height})`),
          h("span", { class: "spacer" }),
          h("input", { type: "number", class: "num small-num", placeholder: "auto", value: o.x, title: "Fixed x offset from the first image (pixels); empty to align automatically", "aria-label": "x offset", onchange: (e: Event) => (o.x = (e.target as HTMLInputElement).value) }),
          h("input", { type: "number", class: "num small-num", placeholder: "auto", value: o.y, title: "Fixed y offset", "aria-label": "y offset", onchange: (e: Event) => (o.y = (e.target as HTMLInputElement).value) }),
          h("button", { type: "button", class: "icon", title: "Move up", disabled: i === 0, onclick: () => ([order[i - 1], order[i]] = [order[i]!, order[i - 1]!], render()) }, "↑"),
          h("button", { type: "button", class: "icon", title: "Move down", disabled: i === order.length - 1, onclick: () => ([order[i + 1], order[i]] = [order[i]!, order[i + 1]!], render()) }, "↓"),
        ),
      ),
    );
  render();
  const choice = await modal(
    "Stitch panorama",
    [
      h("p", { class: "muted small" }, "Images are aligned in this order, each to the one before (same camera and lens, about 20 % overlap or more). Give x and y to place an image by hand."),
      list,
    ],
    [
      { label: "Cancel", value: "cancel" },
      { label: "Stitch", value: "ok", primary: true },
    ],
    { wide: true },
  );
  if (choice !== "ok") return null;
  const used = order.filter((o) => o.on);
  if (used.length < 2) {
    toast("Choose at least two images.", "error");
    return null;
  }
  return {
    ids: used.map((o) => o.doc.info.id),
    offsets: used.map((o, i) => (i > 0 && o.x.trim() !== "" && o.y.trim() !== "" ? [Number(o.x), Number(o.y)] : null)),
  };
}

// ---------------------------------------------------------------------------- trends

/** Measurements of images with the same asset tag, over capture time. */
export async function trendDialog(): Promise<void> {
  const unit = state.unit;
  const assets = [...new Set(state.docs.map((d) => d.asset).filter(Boolean))];
  if (assets.length === 0) {
    toast("Give images an asset tag (Inspection section) to follow them across visits.", "error");
    return;
  }
  let asset = state.docs[state.current]?.asset || assets[0]!;
  let stat: "max" | "avg" | "min" = "max";
  const body = h("div", {});
  const rowsFor = () => {
    const docs = state.docs.filter((d) => d.asset === asset).sort((a, b) => (a.info.camera.dateTime ?? "~").localeCompare(b.info.camera.dateTime ?? "~"));
    const labels = [...new Set(docs.flatMap((d) => d.measurements.map((m) => m.label)))];
    return { docs, labels };
  };
  const render = () => {
    const { docs, labels } = rowsFor();
    const value = (d: Doc, l: string) => {
      const i = d.measurements.findIndex((m) => m.label === l);
      const s = i >= 0 ? d.stats[i] : null;
      return s ? s[stat] : null;
    };
    const ticks: [number, string][] = docs.map((d, i) => [i, (d.info.camera.dateTime ?? d.info.name).slice(0, 10)]);
    fill(
      body,
      h(
        "div",
        { class: "row" },
        h("span", { class: "label" }, "Asset"),
        h("select", { onchange: (e: Event) => ((asset = (e.target as HTMLSelectElement).value), render()) }, assets.map((a) => h("option", { value: a, selected: a === asset }, a))),
        h("select", { onchange: (e: Event) => ((stat = (e.target as HTMLSelectElement).value as typeof stat), render()) }, (["max", "avg", "min"] as const).map((s) => h("option", { value: s, selected: s === stat }, s))),
      ),
      docs.length < 2 ? h("p", { class: "muted" }, "Open images from at least two visits with this asset tag to see a trend.") : null,
      lineChart(
        labels.map((l, k) => ({ name: l, color: SERIES_COLORS[k % SERIES_COLORS.length]!, points: docs.map((d, i) => [i, value(d, l) == null ? NaN : fromKelvin(value(d, l)!, unit)] as [number, number]) })),
        { xLabel: "Visit", yLabel: symbol(unit), xTicks: ticks, height: 180, dots: true },
      ),
      h(
        "table",
        { class: "measurements" },
        h("thead", {}, h("tr", {}, h("th", {}, "Image"), h("th", {}, "Captured"), labels.map((l) => h("th", {}, l)))),
        h("tbody", {}, docs.map((d) => h("tr", {}, h("td", {}, d.info.name), h("td", {}, d.info.camera.dateTime ?? "–"), labels.map((l) => h("td", { class: "n" }, temp(value(d, l), unit)))))),
      ),
    );
  };
  render();
  const choice = await modal("Trends", [body], [
    { label: "Export CSV…", value: "csv" },
    { label: "Close", value: "close", primary: true },
  ], { wide: true });
  if (choice === "csv") {
    const { docs, labels } = rowsFor();
    const lines = [["image", "captured", ...labels.map((l) => `${l} ${stat}`)].map(csvCell).join(",")];
    for (const d of docs) {
      lines.push(
        [csvCell(d.info.name), csvCell(d.info.camera.dateTime ?? ""), ...labels.map((l) => {
          const i = d.measurements.findIndex((m) => m.label === l);
          const s = i >= 0 ? d.stats[i] : null;
          return s ? fromKelvin(s[stat], unit).toFixed(2) : "";
        })].join(","),
      );
    }
    await saveText(`trend-${asset.replace(/[\\/:*?"<>|]+/g, "-")}.csv`, "CSV", "csv", lines.join("\n") + "\n");
  }
}

// ---------------------------------------------------------------------------- map

/** Where the open images were taken, plotted without map tiles (the app stays offline). */
export async function mapDialog(select: (index: number) => void): Promise<void> {
  const unit = state.unit;
  const placed = state.docs.map((d, i) => ({ d, i })).filter(({ d }) => d.info.camera.latitude != null && d.info.camera.longitude != null);
  if (placed.length === 0) {
    toast("None of the open images has a GPS position.", "error");
    return;
  }
  const lat = placed.map(({ d }) => d.info.camera.latitude!);
  const lon = placed.map(({ d }) => d.info.camera.longitude!);
  const [la0, la1, lo0, lo1] = [Math.min(...lat), Math.max(...lat), Math.min(...lon), Math.max(...lon)];
  const k = Math.cos((((la0 + la1) / 2) * Math.PI) / 180);
  const spanX = Math.max((lo1 - lo0) * k, 1e-5);
  const spanY = Math.max(la1 - la0, 1e-5);
  const W = 640;
  const H = 400;
  const s = Math.min((W - 80) / spanX, (H - 60) / spanY);
  const X = (lo: number) => W / 2 + ((lo - (lo0 + lo1) / 2) * k) * s;
  const Y = (la: number) => H / 2 - (la - (la0 + la1) / 2) * s;
  const metres = spanX * 111_320;
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.setAttribute("class", "map");
  const add = (tag: string, attrs: Record<string, string | number>, text?: string) => {
    const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
    for (const [a, v] of Object.entries(attrs)) el.setAttribute(a, String(v));
    if (text) el.textContent = text;
    svg.append(el);
    return el;
  };
  add("rect", { x: 0, y: 0, width: W, height: H, class: "map-bg" });
  placed.forEach(({ d, i }) => {
    const x = X(d.info.camera.longitude!);
    const y = Y(d.info.camera.latitude!);
    const hot = d.dataRange ? temp(d.dataRange[1], unit) : "";
    const g = add("circle", { cx: x, cy: y, r: 6, class: "map-dot" });
    g.addEventListener("click", () => select(i));
    add("text", { x: x + 9, y: y + 4, class: "map-label" }, `${d.info.name}${hot ? ` · ${hot}` : ""}`);
    if (d.info.camera.heading != null) {
      const a = (d.info.camera.heading * Math.PI) / 180;
      add("line", { x1: x, y1: y, x2: x + 18 * Math.sin(a), y2: y - 18 * Math.cos(a), class: "map-heading" });
    }
  });
  const extent = placed.length < 2 ? "one position" : `≈ ${metres >= 1000 ? `${(metres / 1000).toFixed(2)} km` : `${metres.toFixed(0)} m`} across`;
  add("text", { x: 10, y: H - 10, class: "map-label muted" }, `${extent} · north up · no map tiles (offline)`);
  const choice = await modal("Image positions", [svg as unknown as HTMLElement, h("p", { class: "muted small" }, "Click a point to open that image. Export KML for Google Earth or GeoJSON for GIS tools.")], [
    { label: "Export KML…", value: "kml" },
    { label: "Export GeoJSON…", value: "geojson" },
    { label: "Close", value: "close", primary: true },
  ], { wide: true });
  if (choice === "kml" || choice === "geojson") {
    const path = await pickers.save({ defaultPath: `thermal-images.${choice}`, filters: [{ name: choice.toUpperCase(), extensions: [choice] }] });
    if (!path) return;
    try {
      const n = await api.exportMap(
        placed.map(({ d }) => ({ id: d.info.id, description: d.dataRange ? `max ${temp(d.dataRange[1], unit)}, min ${temp(d.dataRange[0], unit)}${d.findings.length ? `, ${d.findings.length} findings` : ""}` : "" })),
        "Thermal images",
        path,
      );
      toast(`Saved ${n} position${n > 1 ? "s" : ""}`);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }
}

// ---------------------------------------------------------------------------- video series

export async function seriesDialog(doc: Doc, series: Series, first: number): Promise<void> {
  const unit = state.unit;
  let stat: "max" | "avg" | "min" = "max";
  const body = h("div", {});
  const render = () =>
    fill(
      body,
      h("div", { class: "row" }, h("span", { class: "label" }, "Show"), h("select", { onchange: (e: Event) => ((stat = (e.target as HTMLSelectElement).value as typeof stat), render()) }, (["max", "avg", "min"] as const).map((s) => h("option", { value: s, selected: s === stat }, s)))),
      lineChart(
        doc.measurements.map((m, j) => ({
          name: m.label,
          color: SERIES_COLORS[j % SERIES_COLORS.length]!,
          points: series.rows.map((row, i) => [series.times[i] ?? i, row[j] ? fromKelvin((row[j] as Stats)[stat], unit) : NaN] as [number, number]),
        })),
        { xLabel: "Time (s)", yLabel: symbol(unit), height: 200 },
      ),
      h("p", { class: "muted small" }, `Frames ${first + 1}–${first + series.rows.length}`),
    );
  render();
  const choice = await modal(`${doc.info.name}: measurements over time`, [body], [
    { label: "Export CSV…", value: "csv" },
    { label: "Close", value: "close", primary: true },
  ], { wide: true });
  if (choice === "csv") {
    const labels = doc.measurements.map((m) => m.label);
    let text = ["frame", "time_s", ...labels.flatMap((l) => [`${l} max`, `${l} min`, `${l} avg`])].map(csvCell).join(",") + "\n";
    series.rows.forEach((row, i) => {
      const cells = [String(first + i + 1), (series.times[i] ?? 0).toFixed(3)];
      for (const s of row) cells.push(...(s ? [s.max, s.min, s.avg].map((k) => fromKelvin(k, unit).toFixed(2)) : ["", "", ""]));
      text += cells.join(",") + "\n";
    });
    await saveText(`${doc.info.name.replace(/\.[^.]+$/, "")}-over-time.csv`, "CSV", "csv", text);
  }
}


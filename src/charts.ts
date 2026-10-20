// Small line charts on a canvas: temperature profiles, values over video time, trends.
import { h } from "./dom";

export interface ChartSeries {
  name: string;
  color: string;
  points: [number, number][];
}

export interface ChartOptions {
  xLabel: string;
  yLabel: string;
  /** Labels for x ticks; otherwise numbers. */
  xTicks?: [number, string][];
  height?: number;
  /** Draw a dot at each point (few points, such as visits). */
  dots?: boolean;
}

export const SERIES_COLORS = ["#f87171", "#60a5fa", "#4ade80", "#c084fc", "#fb923c", "#22d3ee", "#f472b6", "#a3e635"];

/** A chart element sized to its container; redraws when resized. */
export function lineChart(series: ChartSeries[], opts: ChartOptions): HTMLElement {
  const canvas = h("canvas", { class: "chart-canvas" });
  const wrap = h(
    "div",
    { class: "chart" },
    canvas,
    h(
      "div",
      { class: "chart-legend" },
      h("span", { class: "muted" }, `${opts.xLabel} · ${opts.yLabel}`),
      series.map((s) => h("span", {}, h("i", { style: `background:${s.color}` }), s.name)),
    ),
  );
  const draw = () => drawChart(canvas, series, opts);
  new ResizeObserver(draw).observe(canvas);
  return wrap;
}

function drawChart(canvas: HTMLCanvasElement, series: ChartSeries[], opts: ChartOptions): void {
  const dpr = window.devicePixelRatio || 1;
  const cssW = canvas.clientWidth || 280;
  const cssH = opts.height ?? 120;
  canvas.style.height = `${cssH}px`;
  canvas.width = Math.round(cssW * dpr);
  canvas.height = Math.round(cssH * dpr);
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  const css = getComputedStyle(document.documentElement);
  const muted = css.getPropertyValue("--muted").trim() || "#888";
  const grid = css.getPropertyValue("--border").trim() || "#333";

  const all = series.flatMap((s) => s.points).filter(([x, y]) => Number.isFinite(x) && Number.isFinite(y));
  if (all.length === 0) {
    ctx.fillStyle = muted;
    ctx.font = "12px system-ui, sans-serif";
    ctx.fillText("No data", 8, 20);
    return;
  }
  let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const [x, y] of all) {
    x0 = Math.min(x0, x);
    x1 = Math.max(x1, x);
    y0 = Math.min(y0, y);
    y1 = Math.max(y1, y);
  }
  if (x1 - x0 < 1e-9) x1 = x0 + 1;
  const pad = Math.max((y1 - y0) * 0.08, 0.25);
  y0 -= pad;
  y1 += pad;
  const left = 40;
  const bottom = 16;
  const pw = cssW - left - 6;
  const ph = cssH - bottom - 6;
  const X = (x: number) => left + ((x - x0) / (x1 - x0)) * pw;
  const Y = (y: number) => 6 + ph - ((y - y0) / (y1 - y0)) * ph;

  ctx.font = "10px system-ui, sans-serif";
  ctx.strokeStyle = grid;
  ctx.lineWidth = 1;
  ctx.fillStyle = muted;
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  for (let i = 0; i <= 4; i++) {
    const v = y0 + ((y1 - y0) * i) / 4;
    const y = Y(v);
    ctx.beginPath();
    ctx.moveTo(left, y);
    ctx.lineTo(left + pw, y);
    ctx.stroke();
    ctx.fillText(v.toFixed(1), left - 4, y);
  }
  ctx.textBaseline = "top";
  const ticks = opts.xTicks ?? [0, 1, 2, 3, 4].map((i) => [x0 + ((x1 - x0) * i) / 4, ""] as [number, string]);
  ticks.forEach(([v, label], i) => {
    ctx.textAlign = i === 0 ? "left" : i === ticks.length - 1 ? "right" : "center";
    ctx.fillText(label || (Math.abs(x1 - x0) >= 10 ? v.toFixed(0) : v.toFixed(1)), X(v), 6 + ph + 3);
  });

  for (const s of series) {
    const pts = s.points.filter(([x, y]) => Number.isFinite(x) && Number.isFinite(y));
    ctx.strokeStyle = s.color;
    ctx.fillStyle = s.color;
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    pts.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(X(x), Y(y)) : ctx.lineTo(X(x), Y(y))));
    ctx.stroke();
    if (opts.dots || pts.length <= 12) {
      for (const [x, y] of pts) {
        ctx.beginPath();
        ctx.arc(X(x), Y(y), 2.6, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }
}

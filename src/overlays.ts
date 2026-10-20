// Drawing measurement tools, findings and annotations over a thermal image. Shapes follow the
// view's rotation; labels stay upright. Used by the viewer and by image export.
import type { Annotation, Finding, Measurement, Pt, Shape } from "./api";

export interface OverlayStyle {
  /** Device pixels per CSS pixel (or the export's scale). */
  scale: number;
  selected: number | null;
  labels: boolean;
  /** View rotation in radians (ellipses turn with it). */
  rotation: number;
  /** Image pixels to canvas pixels, for sizing the ellipse radii. */
  pixel: number;
}

export const ACCENT = "#38bdf8";
const SEVERITY = ["#64748b", "#ca8a04", "#ea580c", "#dc2626"];

export function severityColor(level: number): string {
  return SEVERITY[Math.min(4, Math.max(1, level)) - 1]!;
}

type ToPx = (x: number, y: number) => Pt;

/** Where a shape's label goes (thermal coordinates). */
export function labelPoint(shape: Shape): Pt {
  switch (shape.kind) {
    case "spot":
    case "wand":
      return [shape.x, shape.y];
    case "box":
      return [Math.min(shape.x, shape.x + shape.w), Math.min(shape.y, shape.y + shape.h)];
    case "ellipse":
      return [shape.cx - shape.rx, shape.cy - shape.ry];
    case "line":
      return [shape.x1, shape.y1];
    case "polygon":
    case "polyline":
      return shape.points[0] ?? [0, 0];
  }
}

/** Handles for resizing, in thermal coordinates. */
export function handles(shape: Shape): Pt[] {
  switch (shape.kind) {
    case "spot":
    case "wand":
      return [];
    case "box":
      return [
        [shape.x, shape.y],
        [shape.x + shape.w, shape.y],
        [shape.x, shape.y + shape.h],
        [shape.x + shape.w, shape.y + shape.h],
      ];
    case "ellipse":
      return [
        [shape.cx - shape.rx, shape.cy - shape.ry],
        [shape.cx + shape.rx, shape.cy - shape.ry],
        [shape.cx - shape.rx, shape.cy + shape.ry],
        [shape.cx + shape.rx, shape.cy + shape.ry],
      ];
    case "line":
      return [
        [shape.x1, shape.y1],
        [shape.x2, shape.y2],
      ];
    case "polygon":
    case "polyline":
      return shape.points;
  }
}

function polyPath(ctx: CanvasRenderingContext2D, pts: Pt[], toPx: ToPx, close: boolean) {
  pts.forEach((p, i) => {
    const [x, y] = toPx(p[0], p[1]);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  if (close && pts.length > 2) ctx.closePath();
}

export function traceShape(ctx: CanvasRenderingContext2D, shape: Shape, toPx: ToPx, style: OverlayStyle, outline: Pt[][] = []): void {
  const s = style.scale;
  switch (shape.kind) {
    case "spot":
    case "wand": {
      const [x, y] = toPx(shape.x, shape.y);
      const r = shape.kind === "spot" ? 5 : 3;
      ctx.moveTo(x + r * s, y);
      ctx.arc(x, y, r * s, 0, Math.PI * 2);
      if (shape.kind === "spot") {
        ctx.moveTo(x - 11 * s, y);
        ctx.lineTo(x - 3 * s, y);
        ctx.moveTo(x + 3 * s, y);
        ctx.lineTo(x + 11 * s, y);
        ctx.moveTo(x, y - 11 * s);
        ctx.lineTo(x, y - 3 * s);
        ctx.moveTo(x, y + 3 * s);
        ctx.lineTo(x, y + 11 * s);
      } else {
        for (const loop of outline) polyPath(ctx, loop, toPx, true);
      }
      break;
    }
    case "box": {
      const { x, y, w, h } = shape;
      polyPath(ctx, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], toPx, true);
      break;
    }
    case "ellipse": {
      const [cx, cy] = toPx(shape.cx, shape.cy);
      const rx = Math.max(0.5, Math.abs(shape.rx) * style.pixel);
      const ry = Math.max(0.5, Math.abs(shape.ry) * style.pixel);
      ctx.moveTo(cx + rx * Math.cos(style.rotation), cy + rx * Math.sin(style.rotation));
      ctx.ellipse(cx, cy, rx, ry, style.rotation, 0, Math.PI * 2);
      break;
    }
    case "line":
      polyPath(ctx, [[shape.x1, shape.y1], [shape.x2, shape.y2]], toPx, false);
      break;
    case "polygon":
      polyPath(ctx, shape.points, toPx, true);
      break;
    case "polyline":
      polyPath(ctx, shape.points, toPx, false);
      break;
  }
}

export function drawTag(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, s: number, bg: string, fg: string): void {
  ctx.font = `600 ${11 * s}px system-ui, -apple-system, "Segoe UI", sans-serif`;
  const w = ctx.measureText(text).width + 8 * s;
  ctx.fillStyle = bg;
  ctx.beginPath();
  ctx.roundRect(x, y, w, 16 * s, 3 * s);
  ctx.fill();
  ctx.fillStyle = fg;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  ctx.fillText(text, x + 4 * s, y + 8.5 * s);
}

/** Draws measurement shapes. `outlines[i]` holds the wand outline of measurement `i`. */
export function drawMeasurements(ctx: CanvasRenderingContext2D, measurements: Measurement[], toPx: ToPx, style: OverlayStyle, outlines: Pt[][][] = [], alarms: boolean[] = []): void {
  const s = style.scale;
  measurements.forEach((m, i) => {
    const selected = style.selected === i;
    const alarm = alarms[i] === true;
    for (const pass of [0, 1]) {
      ctx.beginPath();
      ctx.lineWidth = (pass === 0 ? 3.2 : 1.4) * s;
      ctx.strokeStyle = pass === 0 ? "rgba(0,0,0,0.55)" : selected ? ACCENT : alarm ? "#f87171" : "#ffffff";
      traceShape(ctx, m.shape, toPx, style, outlines[i] ?? []);
      ctx.stroke();
    }
    if (selected) {
      for (const [hx, hy] of handles(m.shape).map((p) => toPx(p[0], p[1]))) {
        ctx.fillStyle = "#0b1220";
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1.5 * s;
        ctx.beginPath();
        ctx.rect(hx - 4 * s, hy - 4 * s, 8 * s, 8 * s);
        ctx.fill();
        ctx.stroke();
      }
    }
    if (style.labels && m.label) {
      const [lx, ly] = toPx(...labelPoint(m.shape));
      const off = m.shape.kind === "spot" ? [8 * s, 6 * s] : m.shape.kind === "ellipse" ? [0, -18 * s] : [3 * s, 3 * s];
      drawTag(ctx, alarm ? `${m.label} !` : m.label, lx + off[0]!, ly + off[1]!, s, selected ? ACCENT : alarm ? "#b91c1c" : "rgba(10,14,22,0.72)", selected ? "#06121f" : "#fff");
    }
  });
}

export function drawFindings(ctx: CanvasRenderingContext2D, findings: Finding[], toPx: ToPx, style: OverlayStyle): void {
  const s = style.scale;
  for (const f of findings) {
    if (f.area.kind !== "box") continue;
    const { x, y, w, h } = f.area;
    ctx.beginPath();
    polyPath(ctx, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], toPx, true);
    ctx.setLineDash([5 * s, 3 * s]);
    ctx.lineWidth = 1.8 * s;
    ctx.strokeStyle = severityColor(f.severity.level);
    ctx.stroke();
    ctx.setLineDash([]);
    if (style.labels) {
      const [lx, ly] = toPx(x, y + h);
      drawTag(ctx, f.label, lx + 2 * s, ly + 2 * s, s, severityColor(f.severity.level), "#fff");
    }
  }
}

export function drawAnnotations(ctx: CanvasRenderingContext2D, annotations: Annotation[], toPx: ToPx, style: OverlayStyle): void {
  const s = style.scale;
  for (const a of annotations) {
    if (a.kind === "sketch") {
      if (a.points.length < 2) continue;
      ctx.beginPath();
      polyPath(ctx, a.points, toPx, false);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.lineWidth = 3.4 * s;
      ctx.strokeStyle = "rgba(0,0,0,0.5)";
      ctx.stroke();
      ctx.lineWidth = 2 * s;
      ctx.strokeStyle = `rgb(${a.color[0]},${a.color[1]},${a.color[2]})`;
      ctx.stroke();
    } else {
      const [x, y] = toPx(a.x, a.y);
      ctx.beginPath();
      ctx.arc(x, y, 4 * s, 0, Math.PI * 2);
      ctx.fillStyle = "#facc15";
      ctx.fill();
      ctx.lineWidth = 1.2 * s;
      ctx.strokeStyle = "#111";
      ctx.stroke();
      if (style.labels) drawTag(ctx, a.text, x + 7 * s, y - 8 * s, s, "#fef9c3", "#111");
    }
  }
}

import type { Pt, Shape } from "./api";
import type { Doc } from "./state";

/** Index of the lowest and highest finite value. */
export function extremes(data: Float32Array): { min: number; max: number } | null {
  let min = -1;
  let max = -1;
  for (let i = 0; i < data.length; i++) {
    const v = data[i]!;
    if (Number.isNaN(v)) continue;
    if (min < 0 || v < data[min]!) min = i;
    if (max < 0 || v > data[max]!) max = i;
  }
  return min < 0 ? null : { min, max };
}

/** Grey scale for high-sensitivity (difference) images: mid-grey is no change. */
export function renderDifference(diff: Float32Array, w: number, h: number, span: number): ImageData {
  const img = new ImageData(w, h);
  for (let i = 0; i < w * h; i++) {
    const v = diff[i]!;
    const g = Number.isFinite(v) ? Math.round(Math.min(255, Math.max(0, 128 + (v / span) * 127))) : 0;
    img.data[i * 4] = g;
    img.data[i * 4 + 1] = g;
    img.data[i * 4 + 2] = g;
    img.data[i * 4 + 3] = 255;
  }
  return img;
}

export function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

export function pointInPolygon(pts: Pt[], x: number, y: number): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i]!;
    const [xj, yj] = pts[j]!;
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

export type Drag =
  | { kind: "pan"; start: Pt; panStart: Pt }
  | { kind: "create"; shape: Shape; origin: Pt }
  | { kind: "move"; index: number; start: Pt; original: Shape }
  | { kind: "handle"; index: number; handle: number; original: Shape }
  | { kind: "sketch"; points: Pt[] }
  | { kind: "crop"; origin: Pt; end: Pt };

export interface Placement {
  src: [number, number, number, number];
  dst: [number, number, number, number];
}

export function isTyping(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
}

export function moved(s: Shape, dx: number, dy: number, w: number, h: number): Shape {
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  switch (s.kind) {
    case "spot":
    case "wand":
      return { ...s, x: clamp(s.x + dx, 0, w - 0.001), y: clamp(s.y + dy, 0, h - 0.001) };
    case "box": {
      const x0 = Math.min(s.x, s.x + s.w);
      const y0 = Math.min(s.y, s.y + s.h);
      const ddx = clamp(dx, -x0, w - (x0 + Math.abs(s.w)));
      const ddy = clamp(dy, -y0, h - (y0 + Math.abs(s.h)));
      return { ...s, x: s.x + ddx, y: s.y + ddy };
    }
    case "ellipse":
      return { ...s, cx: clamp(s.cx + dx, 0, w), cy: clamp(s.cy + dy, 0, h) };
    case "line": {
      const ddx = clamp(dx, -Math.min(s.x1, s.x2), w - Math.max(s.x1, s.x2));
      const ddy = clamp(dy, -Math.min(s.y1, s.y2), h - Math.max(s.y1, s.y2));
      return { ...s, x1: s.x1 + ddx, y1: s.y1 + ddy, x2: s.x2 + ddx, y2: s.y2 + ddy };
    }
    case "polygon":
    case "polyline": {
      const xs = s.points.map((p) => p[0]);
      const ys = s.points.map((p) => p[1]);
      const ddx = clamp(dx, -Math.min(...xs), w - Math.max(...xs));
      const ddy = clamp(dy, -Math.min(...ys), h - Math.max(...ys));
      return { ...s, points: s.points.map((p) => [p[0] + ddx, p[1] + ddy] as Pt) };
    }
  }
}

/** Moves handle `i` (see `handles`) to `p`. */
export function resized(s: Shape, i: number, [px, py]: Pt): Shape {
  switch (s.kind) {
    case "spot":
    case "wand":
      return s;
    case "box": {
      const left = i === 0 || i === 2;
      const top = i === 0 || i === 1;
      const x0 = left ? px : s.x;
      const x1 = left ? s.x + s.w : px;
      const y0 = top ? py : s.y;
      const y1 = top ? s.y + s.h : py;
      return { ...s, x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
    }
    case "ellipse": {
      // The opposite corner of the bounding box stays put.
      const ox = i === 0 || i === 2 ? s.cx + s.rx : s.cx - s.rx;
      const oy = i === 0 || i === 1 ? s.cy + s.ry : s.cy - s.ry;
      return { ...s, cx: (ox + px) / 2, cy: (oy + py) / 2, rx: Math.abs(px - ox) / 2, ry: Math.abs(py - oy) / 2 };
    }
    case "line":
      return i === 0 ? { ...s, x1: px, y1: py } : { ...s, x2: px, y2: py };
    case "polygon":
    case "polyline":
      return { ...s, points: s.points.map((q, k) => (k === i ? ([px, py] as Pt) : q)) };
  }
}

export function normalized(s: Shape): Shape {
  if (s.kind === "box") {
    return { kind: "box", x: Math.min(s.x, s.x + s.w), y: Math.min(s.y, s.y + s.h), w: Math.abs(s.w), h: Math.abs(s.h) };
  }
  if (s.kind === "ellipse") return { ...s, rx: Math.abs(s.rx), ry: Math.abs(s.ry) };
  return s;
}

export function tooSmall(s: Shape): boolean {
  switch (s.kind) {
    case "box":
      return Math.abs(s.w) < 1 || Math.abs(s.h) < 1;
    case "ellipse":
      return s.rx < 0.75 || s.ry < 0.75;
    case "line":
      return Math.hypot(s.x2 - s.x1, s.y2 - s.y1) < 1;
    default:
      return false;
  }
}

export type BaseKind = "thermal" | "photo" | "composite";

/** Thermal pixels → base pixels (the geometry of `Viewer.placement`). */
export function computePlacement(doc: Pick<Doc, "info" | "photo" | "presentation">, base: BaseKind, compositePlacement: Placement | null): Placement {
  const { width: w, height: h } = doc.info;
  const identity: Placement = { src: [0, 0, w, h], dst: [0, 0, w, h] };
  switch (base) {
    case "thermal":
      return identity;
    case "composite":
      return compositePlacement ?? identity;
    case "photo": {
      // The PiP geometry maps the camera's picture-in-picture crop; the same linear mapping
      // extends to the whole thermal frame (render.rs `placement`).
      const pip = doc.info.pip;
      let dst: [number, number, number, number] = [0, 0, doc.photo!.width, doc.photo!.height];
      if (pip && pip.source.w > 0 && pip.source.h > 0) {
        const sx = pip.destination.w / pip.source.w;
        const sy = pip.destination.h / pip.source.h;
        dst = [pip.destination.x - pip.source.x * sx, pip.destination.y - pip.source.y * sy, w * sx, h * sy];
      }
      // Manual alignment (render.rs `placement_aligned`).
      const [dx, dy, s] = doc.presentation.align ?? [0, 0, 1];
      const k = s > 0.05 ? s : 1;
      return { src: [0, 0, w, h], dst: [dst[0] + dx - (dst[2] * (k - 1)) / 2, dst[1] + dy - (dst[3] * (k - 1)) / 2, dst[2] * k, dst[3] * k] };
    }
  }
}

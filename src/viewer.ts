// The image canvas: draws the thermal image, photo or blend, measurement overlays, and handles
// the measurement tools, zoom and pan.
import type { Measurement, Palette, Range, Shape } from "./api";
import type { Doc, Tool, View } from "./state";

type Pt = [number, number];

/** Maps temperatures to RGBA with the same rule as the Rust renderer (palette.rs `index`). */
export function renderThermal(data: Float32Array, w: number, h: number, range: Range, palette: Palette): ImageData {
  const img = new ImageData(w, h);
  const px = img.data;
  const [lo, hi] = range;
  const span = hi - lo;
  for (let i = 0; i < w * h; i++) {
    const v = data[i]!;
    let c: [number, number, number];
    if (Number.isNaN(v)) {
      c = [128, 128, 128];
    } else {
      const idx = span <= 0 ? (v < lo ? 0 : 255) : Math.min(255, Math.max(0, Math.floor(((v - lo) / span) * 256)));
      c = palette.colors[idx]!;
    }
    px[i * 4] = c[0];
    px[i * 4 + 1] = c[1];
    px[i * 4 + 2] = c[2];
    px[i * 4 + 3] = 255;
  }
  return img;
}

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

export interface OverlayStyle {
  scale: number; // device pixels per CSS pixel (or export scale)
  selected: number | null;
  labels: boolean;
}

const ACCENT = "#38bdf8";

/** Draws measurement shapes. `toPx` maps thermal coordinates to canvas pixels. */
export function drawMeasurements(
  ctx: CanvasRenderingContext2D,
  measurements: Measurement[],
  toPx: (x: number, y: number) => Pt,
  style: OverlayStyle,
): void {
  const s = style.scale;
  measurements.forEach((m, i) => {
    const selected = style.selected === i;
    for (const pass of [0, 1]) {
      ctx.beginPath();
      ctx.lineWidth = (pass === 0 ? 3.2 : 1.4) * s;
      ctx.strokeStyle = pass === 0 ? "rgba(0,0,0,0.55)" : selected ? ACCENT : "#ffffff";
      traceShape(ctx, m.shape, toPx, s);
      ctx.stroke();
    }
    if (selected) {
      for (const [hx, hy] of handles(m.shape, toPx)) {
        ctx.fillStyle = "#0b1220";
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1.5 * s;
        ctx.beginPath();
        ctx.rect(hx - 4 * s, hy - 4 * s, 8 * s, 8 * s);
        ctx.fill();
        ctx.stroke();
      }
    }
    if (style.labels) {
      const [lx, ly] = labelAnchor(m.shape, toPx, s);
      drawTag(ctx, m.label, lx, ly, s, selected ? ACCENT : "rgba(10,14,22,0.72)", selected ? "#06121f" : "#fff");
    }
  });
}

function traceShape(ctx: CanvasRenderingContext2D, shape: Shape, toPx: (x: number, y: number) => Pt, s: number) {
  switch (shape.kind) {
    case "spot": {
      const [x, y] = toPx(shape.x, shape.y);
      ctx.moveTo(x + 5 * s, y);
      ctx.arc(x, y, 5 * s, 0, Math.PI * 2);
      ctx.moveTo(x - 11 * s, y);
      ctx.lineTo(x - 3 * s, y);
      ctx.moveTo(x + 3 * s, y);
      ctx.lineTo(x + 11 * s, y);
      ctx.moveTo(x, y - 11 * s);
      ctx.lineTo(x, y - 3 * s);
      ctx.moveTo(x, y + 3 * s);
      ctx.lineTo(x, y + 11 * s);
      break;
    }
    case "box": {
      const [x0, y0] = toPx(shape.x, shape.y);
      const [x1, y1] = toPx(shape.x + shape.w, shape.y + shape.h);
      ctx.rect(Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
      break;
    }
    case "ellipse": {
      const [cx, cy] = toPx(shape.cx, shape.cy);
      const [ex, ey] = toPx(shape.cx + Math.abs(shape.rx), shape.cy + Math.abs(shape.ry));
      ctx.ellipse(cx, cy, Math.max(0.5, Math.abs(ex - cx)), Math.max(0.5, Math.abs(ey - cy)), 0, 0, Math.PI * 2);
      break;
    }
    case "line": {
      const [x0, y0] = toPx(shape.x1, shape.y1);
      const [x1, y1] = toPx(shape.x2, shape.y2);
      ctx.moveTo(x0, y0);
      ctx.lineTo(x1, y1);
      break;
    }
  }
}

function labelAnchor(shape: Shape, toPx: (x: number, y: number) => Pt, s: number): Pt {
  switch (shape.kind) {
    case "spot": {
      const [x, y] = toPx(shape.x, shape.y);
      return [x + 8 * s, y + 6 * s];
    }
    case "box": {
      const [x, y] = toPx(Math.min(shape.x, shape.x + shape.w), Math.min(shape.y, shape.y + shape.h));
      return [x + 3 * s, y + 3 * s];
    }
    case "ellipse": {
      const [x, y] = toPx(shape.cx - Math.abs(shape.rx), shape.cy - Math.abs(shape.ry));
      return [x, y - 18 * s];
    }
    case "line": {
      const [x, y] = toPx(shape.x1, shape.y1);
      return [x + 4 * s, y + 4 * s];
    }
  }
}

function drawTag(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, s: number, bg: string, fg: string) {
  ctx.font = `600 ${11 * s}px system-ui, -apple-system, "Segoe UI", sans-serif`;
  const w = ctx.measureText(text).width + 8 * s;
  ctx.fillStyle = bg;
  ctx.beginPath();
  ctx.roundRect(x, y, w, 16 * s, 3 * s);
  ctx.fill();
  ctx.fillStyle = fg;
  ctx.textBaseline = "middle";
  ctx.fillText(text, x + 4 * s, y + 8.5 * s);
}

function handles(shape: Shape, toPx: (x: number, y: number) => Pt): Pt[] {
  switch (shape.kind) {
    case "spot":
      return [];
    case "box":
      return [
        toPx(shape.x, shape.y),
        toPx(shape.x + shape.w, shape.y),
        toPx(shape.x, shape.y + shape.h),
        toPx(shape.x + shape.w, shape.y + shape.h),
      ];
    case "ellipse":
      return [
        toPx(shape.cx - shape.rx, shape.cy - shape.ry),
        toPx(shape.cx + shape.rx, shape.cy - shape.ry),
        toPx(shape.cx - shape.rx, shape.cy + shape.ry),
        toPx(shape.cx + shape.rx, shape.cy + shape.ry),
      ];
    case "line":
      return [toPx(shape.x1, shape.y1), toPx(shape.x2, shape.y2)];
  }
}

function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

type Drag =
  | { kind: "pan"; start: Pt; panStart: Pt }
  | { kind: "create"; shape: Shape; origin: Pt }
  | { kind: "move"; index: number; start: Pt; original: Shape }
  | { kind: "handle"; index: number; handle: number; original: Shape };

export interface ViewerEvents {
  /** Cursor position in thermal pixels, or null when outside the image. */
  hover(pos: Pt | null): void;
  /** Measurements were added, moved or resized. `final` is false while dragging. */
  measurementsChanged(final: boolean): void;
  select(index: number | null): void;
  zoomChanged(): void;
  /** Label for a new measurement of this kind (Sp1, Bx2, ...). */
  newLabel(kind: Shape["kind"]): string;
}

export class Viewer {
  readonly canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private thermal = document.createElement("canvas");
  private renderKey = "";
  private doc: Doc | null = null;
  private dataExtremes: { min: number; max: number } | null = null;
  private zoom = 1; // relative to fit
  private pan: Pt = [0, 0];
  private drag: Drag | null = null;
  private spaceDown = false;

  view: View = "thermal";
  blend = 0.65;
  smooth = true;
  showExtremes = true;
  tool: Tool = "select";
  selected: number | null = null;
  palette: Palette | null = null;
  range: Range = [0, 1];

  constructor(
    host: HTMLElement,
    private events: ViewerEvents,
  ) {
    this.canvas = document.createElement("canvas");
    this.canvas.className = "viewer-canvas";
    host.append(this.canvas);
    this.ctx = this.canvas.getContext("2d")!;
    new ResizeObserver(() => this.resize()).observe(host);
    this.canvas.addEventListener("pointerdown", (e) => this.onDown(e));
    this.canvas.addEventListener("pointermove", (e) => this.onMove(e));
    this.canvas.addEventListener("pointerup", (e) => this.onUp(e));
    this.canvas.addEventListener("pointercancel", (e) => this.onUp(e));
    this.canvas.addEventListener("pointerleave", () => {
      if (!this.drag) this.events.hover(null);
    });
    this.canvas.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    this.canvas.addEventListener("dblclick", (e) => {
      if (this.tool === "select" && this.hitTest(this.eventPoint(e)) === null) this.fit();
    });
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !isTyping(e)) this.spaceDown = true;
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "Space") this.spaceDown = false;
    });
  }

  setDoc(doc: Doc | null, resetView: boolean): void {
    const changed = doc !== this.doc;
    this.doc = doc;
    if (changed || resetView) {
      this.zoom = 1;
      this.pan = [0, 0];
    }
    this.dataExtremes = doc ? extremes(doc.kelvin) : null;
    this.renderKey = "";
    this.draw();
  }

  /** Call after the doc's temperatures changed (recompute). */
  dataChanged(): void {
    if (this.doc) this.dataExtremes = extremes(this.doc.kelvin);
    this.renderKey = "";
    this.draw();
  }

  /** Screen (CSS) pixels per image pixel, as a percentage. */
  zoomPercent(): number {
    return this.doc ? Math.round((this.scale() / (window.devicePixelRatio || 1)) * 100) : 100;
  }

  fit(): void {
    this.zoom = 1;
    this.pan = [0, 0];
    this.draw();
    this.events.zoomChanged();
  }

  zoomBy(factor: number, at?: Pt): void {
    const doc = this.doc;
    if (!doc) return;
    const p = at ?? [this.canvas.width / 2, this.canvas.height / 2];
    const before = this.toBase(p);
    this.zoom = Math.max(0.25, Math.min(64, this.zoom * factor));
    const after = this.fromBase(before);
    this.pan = [this.pan[0] + p[0] - after[0], this.pan[1] + p[1] - after[1]];
    this.draw();
    this.events.zoomChanged();
  }

  // ------------------------------------------------------------- geometry

  private resize(): void {
    const r = this.canvas.parentElement!.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.round(r.width * dpr));
    this.canvas.height = Math.max(1, Math.round(r.height * dpr));
    this.canvas.style.width = `${r.width}px`;
    this.canvas.style.height = `${r.height}px`;
    this.draw();
  }

  private usesPhoto(doc: Doc): boolean {
    return this.view !== "thermal" && doc.photo !== null;
  }

  /** Size of the drawing space: thermal pixels, or photo pixels in photo/blend view. */
  private baseSize(doc: Doc): Pt {
    return this.usesPhoto(doc) ? [doc.photo!.width, doc.photo!.height] : [doc.info.width, doc.info.height];
  }

  /** Where the thermal image sits in photo pixels (PiP geometry, else stretched over the photo). */
  private thermalPlacement(doc: Doc): { src: [number, number, number, number]; dst: [number, number, number, number] } {
    const { width: w, height: h } = doc.info;
    const pip = doc.info.pip;
    if (pip) {
      const s = pip.source;
      const d = pip.destination;
      return { src: [s.x, s.y, s.w, s.h], dst: [d.x, d.y, d.w, d.h] };
    }
    const [pw, ph] = [doc.photo!.width, doc.photo!.height];
    return { src: [0, 0, w, h], dst: [0, 0, pw, ph] };
  }

  private thermalToBase(doc: Doc, x: number, y: number): Pt {
    if (!this.usesPhoto(doc)) return [x, y];
    const { src, dst } = this.thermalPlacement(doc);
    return [dst[0] + ((x - src[0]) * dst[2]) / src[2], dst[1] + ((y - src[1]) * dst[3]) / src[3]];
  }

  private baseToThermal(doc: Doc, x: number, y: number): Pt {
    if (!this.usesPhoto(doc)) return [x, y];
    const { src, dst } = this.thermalPlacement(doc);
    return [src[0] + ((x - dst[0]) * src[2]) / dst[2], src[1] + ((y - dst[1]) * src[3]) / dst[3]];
  }

  /** Device pixels per base pixel. */
  private scale(): number {
    const doc = this.doc;
    if (!doc) return 1;
    const [bw, bh] = this.baseSize(doc);
    const pad = 24 * (window.devicePixelRatio || 1);
    const fit = Math.min((this.canvas.width - pad) / bw, (this.canvas.height - pad) / bh);
    return Math.max(0.01, fit * this.zoom);
  }

  private origin(): Pt {
    const doc = this.doc!;
    const [bw, bh] = this.baseSize(doc);
    const s = this.scale();
    return [(this.canvas.width - bw * s) / 2 + this.pan[0], (this.canvas.height - bh * s) / 2 + this.pan[1]];
  }

  private fromBase([x, y]: Pt): Pt {
    const [ox, oy] = this.origin();
    const s = this.scale();
    return [ox + x * s, oy + y * s];
  }

  private toBase([x, y]: Pt): Pt {
    const [ox, oy] = this.origin();
    const s = this.scale();
    return [(x - ox) / s, (y - oy) / s];
  }

  private toScreen = (x: number, y: number): Pt => this.fromBase(this.thermalToBase(this.doc!, x, y));

  private toThermal(p: Pt): Pt {
    const [bx, by] = this.toBase(p);
    return this.baseToThermal(this.doc!, bx, by);
  }

  private eventPoint(e: MouseEvent): Pt {
    const r = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    return [(e.clientX - r.left) * dpr, (e.clientY - r.top) * dpr];
  }

  // ------------------------------------------------------------- drawing

  draw(): void {
    const ctx = this.ctx;
    const { width: cw, height: ch } = this.canvas;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, cw, ch);
    const doc = this.doc;
    if (!doc || !this.palette) return;

    this.refreshThermal(doc);
    const s = this.scale();
    const [ox, oy] = this.origin();
    ctx.imageSmoothingEnabled = this.smooth;
    ctx.imageSmoothingQuality = "high";

    if (this.usesPhoto(doc)) {
      const photo = doc.photo!;
      ctx.drawImage(photo, ox, oy, photo.width * s, photo.height * s);
      if (this.view === "blend") {
        const { src, dst } = this.thermalPlacement(doc);
        ctx.globalAlpha = this.blend;
        ctx.drawImage(this.thermal, src[0], src[1], src[2], src[3], ox + dst[0] * s, oy + dst[1] * s, dst[2] * s, dst[3] * s);
        ctx.globalAlpha = 1;
      }
    } else {
      ctx.drawImage(this.thermal, ox, oy, doc.info.width * s, doc.info.height * s);
    }

    if (this.view === "photo" && doc.photo) return;

    const dpr = window.devicePixelRatio || 1;
    if (this.showExtremes && this.dataExtremes) {
      const w = doc.info.width;
      const mark = (i: number, color: string, up: boolean) => {
        const [x, y] = this.toScreen((i % w) + 0.5, Math.floor(i / w) + 0.5);
        const r = 7 * dpr;
        ctx.beginPath();
        ctx.moveTo(x, y + (up ? -r : r));
        ctx.lineTo(x - r * 0.9, y + (up ? r * 0.6 : -r * 0.6));
        ctx.lineTo(x + r * 0.9, y + (up ? r * 0.6 : -r * 0.6));
        ctx.closePath();
        ctx.fillStyle = color;
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 1.6 * dpr;
        ctx.fill();
        ctx.stroke();
      };
      mark(this.dataExtremes.max, "#ef4444", true);
      mark(this.dataExtremes.min, "#3b82f6", false);
    }

    const measurements = [...doc.measurements];
    if (this.drag?.kind === "create") measurements.push({ label: "", shape: this.drag.shape });
    drawMeasurements(ctx, measurements, this.toScreen, { scale: dpr, selected: this.selected, labels: true });
  }

  private refreshThermal(doc: Doc): void {
    const key = `${doc.info.id}|${this.palette!.id}|${this.range[0]}|${this.range[1]}`;
    if (key === this.renderKey) return;
    this.renderKey = key;
    const { width: w, height: h } = doc.info;
    this.thermal.width = w;
    this.thermal.height = h;
    this.thermal.getContext("2d")!.putImageData(renderThermal(doc.kelvin, w, h, this.range, this.palette!), 0, 0);
  }

  // ------------------------------------------------------------- interaction

  private hitTest(p: Pt): number | null {
    const doc = this.doc;
    if (!doc || (this.view === "photo" && doc.photo)) return null;
    const tol = 7 * (window.devicePixelRatio || 1);
    for (let i = doc.measurements.length - 1; i >= 0; i--) {
      const sh = doc.measurements[i]!.shape;
      switch (sh.kind) {
        case "spot": {
          const c = this.toScreen(sh.x, sh.y);
          if (Math.hypot(p[0] - c[0], p[1] - c[1]) <= tol * 1.6) return i;
          break;
        }
        case "box": {
          const a = this.toScreen(sh.x, sh.y);
          const b = this.toScreen(sh.x + sh.w, sh.y + sh.h);
          const [x0, x1] = [Math.min(a[0], b[0]) - tol, Math.max(a[0], b[0]) + tol];
          const [y0, y1] = [Math.min(a[1], b[1]) - tol, Math.max(a[1], b[1]) + tol];
          if (p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1) return i;
          break;
        }
        case "ellipse": {
          const c = this.toScreen(sh.cx, sh.cy);
          const e = this.toScreen(sh.cx + Math.abs(sh.rx), sh.cy + Math.abs(sh.ry));
          const [rx, ry] = [Math.abs(e[0] - c[0]) + tol, Math.abs(e[1] - c[1]) + tol];
          if (((p[0] - c[0]) / rx) ** 2 + ((p[1] - c[1]) / ry) ** 2 <= 1) return i;
          break;
        }
        case "line":
          if (distToSegment(p, this.toScreen(sh.x1, sh.y1), this.toScreen(sh.x2, sh.y2)) <= tol) return i;
          break;
      }
    }
    return null;
  }

  private handleAt(p: Pt): number | null {
    const doc = this.doc;
    if (!doc || this.selected === null) return null;
    const m = doc.measurements[this.selected];
    if (!m) return null;
    const tol = 8 * (window.devicePixelRatio || 1);
    const hs = handles(m.shape, this.toScreen);
    const i = hs.findIndex(([x, y]) => Math.abs(p[0] - x) <= tol && Math.abs(p[1] - y) <= tol);
    return i >= 0 ? i : null;
  }

  private clampToImage([x, y]: Pt): Pt {
    const doc = this.doc!;
    return [Math.min(doc.info.width, Math.max(0, x)), Math.min(doc.info.height, Math.max(0, y))];
  }

  private onDown(e: PointerEvent): void {
    const doc = this.doc;
    if (!doc) return;
    const p = this.eventPoint(e);
    try {
      this.canvas.setPointerCapture(e.pointerId);
    } catch {
      // Capture is a nicety (drags continue outside the canvas); never block the gesture on it.
    }
    const photoOnly = this.view === "photo" && doc.photo !== null;

    if (e.button === 1 || e.button === 2 || this.spaceDown || photoOnly) {
      this.drag = { kind: "pan", start: p, panStart: [...this.pan] };
      this.canvas.classList.add("panning");
      return;
    }
    if (e.button !== 0) return;
    const t = this.clampToImage(this.toThermal(p));

    if (this.tool === "select") {
      const handle = this.handleAt(p);
      if (handle !== null && this.selected !== null) {
        const original = { ...doc.measurements[this.selected]!.shape };
        this.drag = { kind: "handle", index: this.selected, handle, original };
        return;
      }
      const hit = this.hitTest(p);
      this.selected = hit;
      this.events.select(hit);
      if (hit !== null) {
        this.drag = { kind: "move", index: hit, start: this.toThermal(p), original: { ...doc.measurements[hit]!.shape } };
      } else {
        this.drag = { kind: "pan", start: p, panStart: [...this.pan] };
        this.canvas.classList.add("panning");
      }
      this.draw();
      return;
    }

    const raw = this.toThermal(p);
    if (raw[0] < 0 || raw[1] < 0 || raw[0] > doc.info.width || raw[1] > doc.info.height) return;
    let shape: Shape;
    switch (this.tool) {
      case "spot":
        shape = { kind: "spot", x: t[0], y: t[1] };
        break;
      case "box":
        shape = { kind: "box", x: t[0], y: t[1], w: 0, h: 0 };
        break;
      case "ellipse":
        shape = { kind: "ellipse", cx: t[0], cy: t[1], rx: 0, ry: 0 };
        break;
      case "line":
        shape = { kind: "line", x1: t[0], y1: t[1], x2: t[0], y2: t[1] };
        break;
    }
    if (shape.kind === "spot") {
      this.addMeasurement(shape);
      this.drag = { kind: "move", index: doc.measurements.length - 1, start: t, original: { ...shape } };
      return;
    }
    this.drag = { kind: "create", shape, origin: t };
    this.draw();
  }

  private onMove(e: PointerEvent): void {
    const doc = this.doc;
    if (!doc) return;
    const p = this.eventPoint(e);
    const t = this.toThermal(p);
    const inside = t[0] >= 0 && t[1] >= 0 && t[0] < doc.info.width && t[1] < doc.info.height;
    this.events.hover(inside && !(this.view === "photo" && doc.photo) ? t : null);
    this.updateCursor(p);

    const drag = this.drag;
    if (!drag) return;
    const c = this.clampToImage(t);
    switch (drag.kind) {
      case "pan":
        this.pan = [drag.panStart[0] + p[0] - drag.start[0], drag.panStart[1] + p[1] - drag.start[1]];
        this.draw();
        return;
      case "create": {
        const [x0, y0] = drag.origin;
        const sh = drag.shape;
        if (sh.kind === "box") Object.assign(sh, { x: x0, y: y0, w: c[0] - x0, h: c[1] - y0 });
        if (sh.kind === "ellipse") {
          // Drag from one corner of the bounding box to the other.
          Object.assign(sh, { cx: (x0 + c[0]) / 2, cy: (y0 + c[1]) / 2, rx: Math.abs(c[0] - x0) / 2, ry: Math.abs(c[1] - y0) / 2 });
        }
        if (sh.kind === "line") Object.assign(sh, { x2: c[0], y2: c[1] });
        this.draw();
        return;
      }
      case "move": {
        const m = doc.measurements[drag.index];
        if (!m) return;
        m.shape = moved(drag.original, t[0] - drag.start[0], t[1] - drag.start[1], doc.info.width, doc.info.height);
        this.draw();
        this.events.measurementsChanged(false);
        return;
      }
      case "handle": {
        const m = doc.measurements[drag.index];
        if (!m) return;
        m.shape = resized(drag.original, drag.handle, c);
        this.draw();
        this.events.measurementsChanged(false);
        return;
      }
    }
  }

  private onUp(e: PointerEvent): void {
    const drag = this.drag;
    this.drag = null;
    this.canvas.classList.remove("panning");
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    if (!drag || !this.doc) return;
    if (drag.kind === "create") {
      if (!tooSmall(drag.shape)) this.addMeasurement(normalized(drag.shape));
      this.draw();
    } else if (drag.kind === "move" || drag.kind === "handle") {
      const m = this.doc.measurements[drag.index];
      if (m) m.shape = normalized(m.shape);
      this.events.measurementsChanged(true);
      this.draw();
    }
  }

  private addMeasurement(shape: Shape): void {
    const doc = this.doc!;
    doc.measurements.push({ label: this.events.newLabel(shape.kind), shape });
    this.selected = doc.measurements.length - 1;
    this.events.select(this.selected);
    this.events.measurementsChanged(true);
    this.draw();
  }

  private onWheel(e: WheelEvent): void {
    if (!this.doc) return;
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0015));
    this.zoomBy(factor, this.eventPoint(e));
  }

  private updateCursor(p: Pt): void {
    if (this.drag) return;
    let cursor = "crosshair";
    if (this.spaceDown || (this.view === "photo" && this.doc?.photo)) cursor = "grab";
    else if (this.tool === "select") {
      cursor = this.handleAt(p) !== null ? "nwse-resize" : this.hitTest(p) !== null ? "move" : "default";
    }
    this.canvas.style.cursor = cursor;
  }
}

function isTyping(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
}

function moved(s: Shape, dx: number, dy: number, w: number, h: number): Shape {
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  switch (s.kind) {
    case "spot":
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
  }
}

/** Moves handle `i` (see `handles`) to `p`. */
function resized(s: Shape, i: number, [px, py]: Pt): Shape {
  switch (s.kind) {
    case "spot":
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
  }
}

function normalized(s: Shape): Shape {
  if (s.kind === "box") {
    return { kind: "box", x: Math.min(s.x, s.x + s.w), y: Math.min(s.y, s.y + s.h), w: Math.abs(s.w), h: Math.abs(s.h) };
  }
  if (s.kind === "ellipse") return { ...s, rx: Math.abs(s.rx), ry: Math.abs(s.ry) };
  return s;
}

function tooSmall(s: Shape): boolean {
  switch (s.kind) {
    case "box":
      return Math.abs(s.w) < 1 || Math.abs(s.h) < 1;
    case "ellipse":
      return s.rx < 0.75 || s.ry < 0.75;
    case "line":
      return Math.hypot(s.x2 - s.x1, s.y2 - s.y1) < 1;
    case "spot":
      return false;
  }
}

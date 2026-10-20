// The image canvas: draws the thermal image (rendered by Rust), the photo, blends and composites,
// overlays, and handles the tools, zoom, pan, rotation and crop.
import type { Composite, Pt, Shape, ShapeKind } from "./api";
import { ACCENT, drawAnnotations, drawFindings, drawMeasurements, handles, type OverlayStyle } from "./overlays";
import type { Doc, Tool, View } from "./state";

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

function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

function pointInPolygon(pts: Pt[], x: number, y: number): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i]!;
    const [xj, yj] = pts[j]!;
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

type Drag =
  | { kind: "pan"; start: Pt; panStart: Pt }
  | { kind: "create"; shape: Shape; origin: Pt }
  | { kind: "move"; index: number; start: Pt; original: Shape }
  | { kind: "handle"; index: number; handle: number; original: Shape }
  | { kind: "sketch"; points: Pt[] }
  | { kind: "crop"; origin: Pt; end: Pt };

export interface ViewerEvents {
  /** Cursor position in thermal pixels, or null when outside the image. */
  hover(pos: Pt | null): void;
  /** Measurements were added, moved or resized. `final` is false while dragging. */
  measurementsChanged(final: boolean): void;
  select(index: number | null): void;
  zoomChanged(): void;
  /** Label for a new measurement of this kind (Sp1, Bx2, ...). */
  newLabel(kind: ShapeKind): string;
  annotationsChanged(): void;
  cropChanged(crop: [number, number, number, number] | null): void;
  /** Asks for a note's text at a screen position (CSS pixels); resolves to null if cancelled. */
  requestText(at: Pt): Promise<string | null>;
  wandTolerance(): number;
  sketchColor(): [number, number, number];
  /** The thermal image as colours, and the photo composite for MSX and fusion views. */
  renderThermal(doc: Doc): Promise<ImageData>;
  renderComposite(doc: Doc): Promise<Composite>;
}

interface Placement {
  src: [number, number, number, number];
  dst: [number, number, number, number];
}

export class Viewer {
  readonly canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private thermal = document.createElement("canvas");
  /** Image id the thermal canvas currently holds, so another image's picture never shows. */
  private thermalFor = -1;
  private composite = document.createElement("canvas");
  private compositePlacement: Placement | null = null;
  private thermalKey = "";
  private thermalPending = "";
  private compositeKey = "";
  private compositePending = "";
  private doc: Doc | null = null;
  private dataVersion = 0;
  private dataExtremes: { min: number; max: number } | null = null;
  private zoom = 1; // relative to fit
  private pan: Pt = [0, 0];
  private drag: Drag | null = null;
  private drawing: { kind: "polygon" | "polyline"; points: Pt[]; cursor: Pt | null } | null = null;
  private spaceDown = false;

  view: View = "thermal";
  blend = 0.65;
  smooth = true;
  showExtremes = true;
  tool: Tool = "select";
  selected: number | null = null;
  /** Key of everything that changes the thermal colours (palette, range, distribution, isotherms). */
  lookKey = "";
  /** Key of the composite settings (MSX strength, fusion interval). */
  fusionKey = "";
  /** Alarm state per measurement, for highlighting. */
  alarms: boolean[] = [];

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
    this.canvas.addEventListener("pointerup", (e) => void this.onUp(e));
    this.canvas.addEventListener("pointercancel", (e) => void this.onUp(e));
    this.canvas.addEventListener("pointerleave", () => {
      if (!this.drag) this.events.hover(null);
    });
    this.canvas.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    this.canvas.addEventListener("dblclick", (e) => {
      if (this.drawing) {
        this.finishDrawing();
        return;
      }
      if (this.tool === "select" && this.hitTest(this.eventPoint(e)) === null) this.fit();
    });
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !isTyping(e)) this.spaceDown = true;
      if (this.drawing && !isTyping(e)) {
        if (e.key === "Enter") {
          e.preventDefault();
          this.finishDrawing();
        } else if (e.key === "Escape") {
          this.drawing = null;
          this.draw();
        } else if (e.key === "Backspace") {
          e.preventDefault();
          this.drawing.points.pop();
          if (this.drawing.points.length === 0) this.drawing = null;
          this.draw();
        }
      }
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "Space") this.spaceDown = false;
    });
  }

  /** Whether a polygon or polyline is being drawn (keys go to it). */
  isDrawing(): boolean {
    return this.drawing !== null;
  }

  setDoc(doc: Doc | null, resetView: boolean): void {
    const changed = doc !== this.doc;
    this.doc = doc;
    this.drawing = null;
    if (changed || resetView) {
      this.zoom = 1;
      this.pan = [0, 0];
    }
    if (changed) {
      this.thermalKey = "";
      this.compositeKey = "";
    }
    this.dataExtremes = doc ? extremes(doc.kelvin) : null;
    this.draw();
  }

  /** Call after the doc's temperatures changed (recompute, new video frame). */
  dataChanged(): void {
    this.dataVersion++;
    if (this.doc) this.dataExtremes = extremes(this.doc.kelvin);
    this.draw();
  }

  /** Screen (CSS) pixels per image pixel, as a percentage. */
  zoomPercent(): number {
    const doc = this.doc;
    if (!doc) return 100;
    // Thermal pixels in thermal-based views, photo pixels when showing the photo.
    const s = this.base(doc) === "photo" ? this.scale() : this.thermalScale(doc);
    return Math.round((s / (window.devicePixelRatio || 1)) * 100);
  }

  fit(): void {
    this.zoom = 1;
    this.pan = [0, 0];
    this.draw();
    this.events.zoomChanged();
  }

  zoomBy(factor: number, at?: Pt): void {
    if (!this.doc) return;
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

  private hsmActive(doc: Doc): boolean {
    return !!doc.video?.hsm && !!doc.video.hsmData;
  }

  /** Which picture is the drawing base. */
  private base(doc: Doc): "thermal" | "photo" | "composite" {
    if (this.hsmActive(doc) || this.view === "thermal" || !doc.photo) return "thermal";
    if (this.view === "msx" || this.view === "fusion") return "composite";
    return "photo";
  }

  private photoOnly(doc: Doc): boolean {
    return this.view === "photo" && doc.photo !== null && !this.hsmActive(doc);
  }

  /** Thermal pixels → base pixels. */
  private placement(doc: Doc): Placement {
    const { width: w, height: h } = doc.info;
    const identity: Placement = { src: [0, 0, w, h], dst: [0, 0, w, h] };
    switch (this.base(doc)) {
      case "thermal":
        return identity;
      case "composite":
        return this.compositePlacement ?? identity;
      case "photo": {
        // The PiP geometry maps the camera's picture-in-picture crop; the same linear mapping
        // extends to the whole thermal frame (render.rs `placement`).
        const pip = doc.info.pip;
        if (pip && pip.source.w > 0 && pip.source.h > 0) {
          const sx = pip.destination.w / pip.source.w;
          const sy = pip.destination.h / pip.source.h;
          return { src: [0, 0, w, h], dst: [pip.destination.x - pip.source.x * sx, pip.destination.y - pip.source.y * sy, w * sx, h * sy] };
        }
        return { src: [0, 0, w, h], dst: [0, 0, doc.photo!.width, doc.photo!.height] };
      }
    }
  }

  /** The base region shown: the crop in thermal views, else the whole base picture. */
  private region(doc: Doc): [number, number, number, number] {
    const b = this.base(doc);
    if (b === "thermal") {
      const c = doc.presentation.crop;
      return c ?? [0, 0, doc.info.width, doc.info.height];
    }
    if (b === "composite" && this.compositePlacement) return [0, 0, this.composite.width, this.composite.height];
    return [0, 0, doc.photo!.width, doc.photo!.height];
  }

  private angle(): number {
    return ((this.doc?.presentation.rotation ?? 0) * Math.PI) / 180;
  }

  /** Device pixels per base pixel. */
  private scale(): number {
    const doc = this.doc;
    if (!doc) return 1;
    const [, , bw, bh] = this.region(doc);
    const a = this.angle();
    const rw = Math.abs(bw * Math.cos(a)) + Math.abs(bh * Math.sin(a));
    const rh = Math.abs(bw * Math.sin(a)) + Math.abs(bh * Math.cos(a));
    const pad = 24 * (window.devicePixelRatio || 1);
    const fit = Math.min((this.canvas.width - pad) / rw, (this.canvas.height - pad) / rh);
    return Math.max(0.01, fit * this.zoom);
  }

  private centre(): Pt {
    const [x, y, w, h] = this.region(this.doc!);
    return [x + w / 2, y + h / 2];
  }

  private fromBase([x, y]: Pt): Pt {
    const s = this.scale();
    const a = this.angle();
    const [cx, cy] = this.centre();
    const dx = (x - cx) * s;
    const dy = (y - cy) * s;
    return [this.canvas.width / 2 + this.pan[0] + dx * Math.cos(a) - dy * Math.sin(a), this.canvas.height / 2 + this.pan[1] + dx * Math.sin(a) + dy * Math.cos(a)];
  }

  private toBase([px, py]: Pt): Pt {
    const s = this.scale();
    const a = this.angle();
    const [cx, cy] = this.centre();
    const dx = px - this.canvas.width / 2 - this.pan[0];
    const dy = py - this.canvas.height / 2 - this.pan[1];
    return [cx + (dx * Math.cos(a) + dy * Math.sin(a)) / s, cy + (-dx * Math.sin(a) + dy * Math.cos(a)) / s];
  }

  private thermalToBase(doc: Doc, x: number, y: number): Pt {
    const { src, dst } = this.placement(doc);
    return [dst[0] + ((x - src[0]) * dst[2]) / src[2], dst[1] + ((y - src[1]) * dst[3]) / src[3]];
  }

  private baseToThermal(doc: Doc, x: number, y: number): Pt {
    const { src, dst } = this.placement(doc);
    return [src[0] + ((x - dst[0]) * src[2]) / dst[2], src[1] + ((y - dst[1]) * src[3]) / dst[3]];
  }

  private toScreen = (x: number, y: number): Pt => this.fromBase(this.thermalToBase(this.doc!, x, y));

  private toThermal(p: Pt): Pt {
    const [bx, by] = this.toBase(p);
    return this.baseToThermal(this.doc!, bx, by);
  }

  /** Device pixels per thermal pixel. */
  private thermalScale(doc: Doc): number {
    const { src, dst } = this.placement(doc);
    return this.scale() * (dst[2] / src[2]);
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
    if (!doc) return;

    const base = this.base(doc);
    if (base === "composite") this.ensureComposite(doc);
    if (base !== "photo" || this.view === "blend") this.ensureThermal(doc);
    if (base === "composite" && !this.compositePlacement) return;

    // Base → screen as a canvas transform.
    const s = this.scale();
    const a = this.angle();
    const [cx, cy] = this.centre();
    const [ox, oy] = [cw / 2 + this.pan[0], ch / 2 + this.pan[1]];
    const ta = s * Math.cos(a);
    const tb = s * Math.sin(a);
    ctx.setTransform(ta, tb, -tb, ta, ox - (ta * cx - tb * cy), oy - (tb * cx + ta * cy));
    ctx.imageSmoothingEnabled = this.smooth;
    ctx.imageSmoothingQuality = "high";

    ctx.save();
    const [rx, ry, rw, rh] = this.region(doc);
    ctx.beginPath();
    ctx.rect(rx, ry, rw, rh);
    ctx.clip();
    if (base === "thermal") {
      if (this.thermalFor === doc.info.id) ctx.drawImage(this.thermal, 0, 0, doc.info.width, doc.info.height);
    } else if (base === "composite") {
      ctx.drawImage(this.composite, 0, 0);
    } else {
      const photo = doc.photo!;
      ctx.drawImage(photo, 0, 0);
      if (this.view === "blend") {
        const { src, dst } = this.placement(doc);
        if (this.thermalFor === doc.info.id) {
          ctx.globalAlpha = this.blend;
          ctx.drawImage(this.thermal, src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3]);
          ctx.globalAlpha = 1;
        }
      }
    }
    ctx.restore();
    ctx.setTransform(1, 0, 0, 1, 0, 0);

    if (this.photoOnly(doc)) return;

    const dpr = window.devicePixelRatio || 1;
    const style: OverlayStyle = { scale: dpr, selected: this.selected, labels: true, rotation: a, pixel: this.thermalScale(doc) };
    // Overlays outside a crop are hidden with it.
    ctx.save();
    if (doc.presentation.crop && base === "thermal") {
      ctx.beginPath();
      [[rx, ry], [rx + rw, ry], [rx + rw, ry + rh], [rx, ry + rh]].forEach((p, i) => {
        const [x, y] = this.fromBase([p[0]!, p[1]!]);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.closePath();
      ctx.clip();
    }
    if (this.showExtremes && this.dataExtremes && !this.hsmActive(doc)) {
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

    drawFindings(ctx, doc.findings, this.toScreen, style);
    const measurements = [...doc.measurements];
    if (this.drag?.kind === "create") measurements.push({ label: "", shape: this.drag.shape });
    drawMeasurements(ctx, measurements, this.toScreen, style, doc.outlines, this.alarms);
    const notes = [...doc.annotations];
    if (this.drag?.kind === "sketch") notes.push({ kind: "sketch", points: this.drag.points, color: this.events.sketchColor() });
    drawAnnotations(ctx, notes, this.toScreen, style);
    ctx.restore();

    if (this.drawing) {
      const pts = [...this.drawing.points, ...(this.drawing.cursor ? [this.drawing.cursor] : [])];
      ctx.beginPath();
      pts.forEach((p, i) => {
        const [x, y] = this.toScreen(p[0], p[1]);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      if (this.drawing.kind === "polygon" && pts.length > 2) ctx.closePath();
      ctx.setLineDash([5 * dpr, 4 * dpr]);
      ctx.lineWidth = 1.6 * dpr;
      ctx.strokeStyle = ACCENT;
      ctx.stroke();
      ctx.setLineDash([]);
      for (const p of this.drawing.points) {
        const [x, y] = this.toScreen(p[0], p[1]);
        ctx.fillStyle = ACCENT;
        ctx.fillRect(x - 3 * dpr, y - 3 * dpr, 6 * dpr, 6 * dpr);
      }
    }
    if (this.drag?.kind === "crop") {
      const [x0, y0] = this.drag.origin;
      const [x1, y1] = this.drag.end;
      ctx.beginPath();
      [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].forEach((p, i) => {
        const [x, y] = this.toScreen(p[0]!, p[1]!);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.closePath();
      ctx.setLineDash([6 * dpr, 4 * dpr]);
      ctx.lineWidth = 1.5 * dpr;
      ctx.strokeStyle = "#fde047";
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }

  private ensureThermal(doc: Doc): void {
    const hsm = this.hsmActive(doc);
    const key = `${doc.info.id}|${this.dataVersion}|${hsm ? `hsm${doc.video!.hsmSpan}` : this.lookKey}`;
    if (key === this.thermalKey || key === this.thermalPending) return;
    if (hsm) {
      const img = renderDifference(doc.video!.hsmData!, doc.info.width, doc.info.height, doc.video!.hsmSpan);
      this.putThermal(img, doc.info.id);
      this.thermalKey = key;
      return;
    }
    this.thermalPending = key;
    void this.events
      .renderThermal(doc)
      .then((img) => {
        if (this.thermalPending !== key) return;
        this.thermalPending = "";
        this.putThermal(img, doc.info.id);
        this.thermalKey = key;
        this.draw();
      })
      .catch(() => {
        if (this.thermalPending === key) this.thermalPending = "";
      });
  }

  private putThermal(img: ImageData, id: number): void {
    this.thermalFor = id;
    this.thermal.width = img.width;
    this.thermal.height = img.height;
    this.thermal.getContext("2d")!.putImageData(img, 0, 0);
  }

  private ensureComposite(doc: Doc): void {
    const key = `${doc.info.id}|${this.dataVersion}|${this.lookKey}|${this.view}|${this.fusionKey}`;
    if (key === this.compositeKey || key === this.compositePending) return;
    this.compositePending = key;
    void this.events
      .renderComposite(doc)
      .then((c) => {
        if (this.compositePending !== key) return;
        this.compositePending = "";
        this.composite.width = c.image.width;
        this.composite.height = c.image.height;
        this.composite.getContext("2d")!.putImageData(c.image, 0, 0);
        const first = !this.compositePlacement;
        this.compositePlacement = { src: c.source, dst: c.destination };
        this.compositeKey = key;
        if (first) this.zoom = 1;
        this.draw();
      })
      .catch(() => {
        if (this.compositePending === key) this.compositePending = "";
      });
  }

  /** Forget the composite (its size may change with the view). */
  resetComposite(): void {
    this.compositePlacement = null;
    this.compositeKey = "";
  }

  // ------------------------------------------------------------- interaction

  private hitTest(p: Pt): number | null {
    const doc = this.doc;
    if (!doc || this.photoOnly(doc)) return null;
    const [tx, ty] = this.toThermal(p);
    const tol = (7 * (window.devicePixelRatio || 1)) / this.thermalScale(doc);
    for (let i = doc.measurements.length - 1; i >= 0; i--) {
      const sh = doc.measurements[i]!.shape;
      switch (sh.kind) {
        case "spot":
        case "wand":
          if (Math.hypot(tx - sh.x, ty - sh.y) <= tol * 1.6) return i;
          break;
        case "box": {
          const [x0, x1] = [Math.min(sh.x, sh.x + sh.w) - tol, Math.max(sh.x, sh.x + sh.w) + tol];
          const [y0, y1] = [Math.min(sh.y, sh.y + sh.h) - tol, Math.max(sh.y, sh.y + sh.h) + tol];
          if (tx >= x0 && tx <= x1 && ty >= y0 && ty <= y1) return i;
          break;
        }
        case "ellipse": {
          const [rx, ry] = [Math.abs(sh.rx) + tol, Math.abs(sh.ry) + tol];
          if (((tx - sh.cx) / rx) ** 2 + ((ty - sh.cy) / ry) ** 2 <= 1) return i;
          break;
        }
        case "line":
          if (distToSegment([tx, ty], [sh.x1, sh.y1], [sh.x2, sh.y2]) <= tol) return i;
          break;
        case "polygon":
        case "polyline": {
          const pts = sh.points;
          const segs = sh.kind === "polygon" ? pts.map((q, k) => [q, pts[(k + 1) % pts.length]!] as const) : pts.slice(1).map((q, k) => [pts[k]!, q] as const);
          if (segs.some(([a, b]) => distToSegment([tx, ty], a, b) <= tol)) return i;
          if (sh.kind === "polygon" && pts.length > 2 && pointInPolygon(pts, tx, ty)) return i;
          break;
        }
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
    const i = handles(m.shape)
      .map((q) => this.toScreen(q[0], q[1]))
      .findIndex(([x, y]) => Math.abs(p[0] - x) <= tol && Math.abs(p[1] - y) <= tol);
    return i >= 0 ? i : null;
  }

  private clampToImage([x, y]: Pt): Pt {
    const doc = this.doc!;
    return [Math.min(doc.info.width, Math.max(0, x)), Math.min(doc.info.height, Math.max(0, y))];
  }

  private inside(t: Pt): boolean {
    const doc = this.doc!;
    return t[0] >= 0 && t[1] >= 0 && t[0] <= doc.info.width && t[1] <= doc.info.height;
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

    if (e.button === 1 || e.button === 2 || this.spaceDown || this.photoOnly(doc)) {
      this.drag = { kind: "pan", start: p, panStart: [...this.pan] };
      this.canvas.classList.add("panning");
      return;
    }
    if (e.button !== 0) return;
    const raw = this.toThermal(p);
    const t = this.clampToImage(raw);

    if (this.tool === "select") {
      const handle = this.handleAt(p);
      if (handle !== null && this.selected !== null) {
        const original = structuredClone(doc.measurements[this.selected]!.shape);
        this.drag = { kind: "handle", index: this.selected, handle, original };
        return;
      }
      const hit = this.hitTest(p);
      this.selected = hit;
      this.events.select(hit);
      if (hit !== null) {
        this.drag = { kind: "move", index: hit, start: raw, original: structuredClone(doc.measurements[hit]!.shape) };
      } else {
        this.drag = { kind: "pan", start: p, panStart: [...this.pan] };
        this.canvas.classList.add("panning");
      }
      this.draw();
      return;
    }

    if (!this.inside(raw)) return;
    switch (this.tool) {
      case "spot": {
        const shape: Shape = { kind: "spot", x: t[0], y: t[1] };
        this.addMeasurement(shape);
        this.drag = { kind: "move", index: doc.measurements.length - 1, start: t, original: { ...shape } };
        return;
      }
      case "wand":
        this.addMeasurement({ kind: "wand", x: t[0], y: t[1], tolerance: this.events.wandTolerance() });
        return;
      case "box":
        this.drag = { kind: "create", shape: { kind: "box", x: t[0], y: t[1], w: 0, h: 0 }, origin: t };
        break;
      case "ellipse":
        this.drag = { kind: "create", shape: { kind: "ellipse", cx: t[0], cy: t[1], rx: 0, ry: 0 }, origin: t };
        break;
      case "line":
        this.drag = { kind: "create", shape: { kind: "line", x1: t[0], y1: t[1], x2: t[0], y2: t[1] }, origin: t };
        break;
      case "polygon":
      case "polyline": {
        if (this.drawing && this.drawing.kind !== this.tool) this.drawing = null;
        if (!this.drawing) this.drawing = { kind: this.tool, points: [], cursor: null };
        const pts = this.drawing.points;
        // Clicking the first point closes a polygon.
        if (this.tool === "polygon" && pts.length >= 3) {
          const [fx, fy] = this.toScreen(pts[0]![0], pts[0]![1]);
          if (Math.hypot(p[0] - fx, p[1] - fy) <= 9 * (window.devicePixelRatio || 1)) {
            this.finishDrawing();
            return;
          }
        }
        const last = pts[pts.length - 1];
        if (!last || Math.hypot(last[0] - t[0], last[1] - t[1]) > 0.25) pts.push(t);
        this.draw();
        return;
      }
      case "text": {
        const r = this.canvas.getBoundingClientRect();
        void this.events.requestText([e.clientX - r.left, e.clientY - r.top]).then((text) => {
          if (text && this.doc === doc) {
            doc.annotations.push({ kind: "text", x: t[0], y: t[1], text });
            this.events.annotationsChanged();
            this.draw();
          }
        });
        return;
      }
      case "sketch":
        this.drag = { kind: "sketch", points: [t] };
        return;
      case "crop":
        this.drag = { kind: "crop", origin: t, end: t };
        return;
    }
    this.draw();
  }

  private finishDrawing(): void {
    const d = this.drawing;
    this.drawing = null;
    if (!d) return;
    const need = d.kind === "polygon" ? 3 : 2;
    if (d.points.length >= need) {
      this.addMeasurement({ kind: d.kind, points: d.points.map((q) => [q[0], q[1]] as Pt) });
    }
    this.draw();
  }

  private onMove(e: PointerEvent): void {
    const doc = this.doc;
    if (!doc) return;
    const p = this.eventPoint(e);
    const t = this.toThermal(p);
    const inside = t[0] >= 0 && t[1] >= 0 && t[0] < doc.info.width && t[1] < doc.info.height;
    this.events.hover(inside && !this.photoOnly(doc) ? t : null);
    this.updateCursor(p);

    if (this.drawing) {
      this.drawing.cursor = this.clampToImage(t);
      this.draw();
    }
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
      case "sketch": {
        const last = drag.points[drag.points.length - 1]!;
        if (Math.hypot(last[0] - c[0], last[1] - c[1]) * this.thermalScale(doc) >= 2) drag.points.push(c);
        this.draw();
        return;
      }
      case "crop":
        drag.end = c;
        this.draw();
        return;
    }
  }

  private async onUp(e: PointerEvent): Promise<void> {
    const drag = this.drag;
    this.drag = null;
    this.canvas.classList.remove("panning");
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    const doc = this.doc;
    if (!drag || !doc) return;
    switch (drag.kind) {
      case "create":
        if (!tooSmall(drag.shape)) this.addMeasurement(normalized(drag.shape));
        this.draw();
        break;
      case "move":
      case "handle": {
        const m = doc.measurements[drag.index];
        if (m) m.shape = normalized(m.shape);
        this.events.measurementsChanged(true);
        this.draw();
        break;
      }
      case "sketch":
        if (drag.points.length >= 2) {
          doc.annotations.push({ kind: "sketch", points: drag.points, color: this.events.sketchColor() });
          this.events.annotationsChanged();
        }
        this.draw();
        break;
      case "crop": {
        const [x0, y0] = drag.origin;
        const [x1, y1] = drag.end;
        const w = Math.abs(x1 - x0);
        const h = Math.abs(y1 - y0);
        if (w >= 4 && h >= 4) {
          this.events.cropChanged([Math.round(Math.min(x0, x1)), Math.round(Math.min(y0, y1)), Math.round(w), Math.round(h)]);
          this.zoom = 1;
          this.pan = [0, 0];
        }
        this.draw();
        break;
      }
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
    if (this.spaceDown || (this.doc && this.photoOnly(this.doc))) cursor = "grab";
    else if (this.tool === "select") {
      cursor = this.handleAt(p) !== null ? "move" : this.hitTest(p) !== null ? "move" : "default";
    } else if (this.tool === "text") cursor = "text";
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
function resized(s: Shape, i: number, [px, py]: Pt): Shape {
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
    default:
      return false;
  }
}

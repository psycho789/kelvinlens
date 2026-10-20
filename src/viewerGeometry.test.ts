import { describe, expect, it } from "vitest";
import type { Shape } from "./api";
import type { Doc } from "./state";
import { computePlacement, moved, normalized, resized, tooSmall } from "./viewerGeometry";

const pip = { source: { x: 10, y: 20, w: 100, h: 50 }, destination: { x: 30, y: 40, w: 200, h: 100 } };
const doc = (w: number, h: number, opts: { pip?: typeof pip; align?: [number, number, number]; photo?: [number, number] } = {}) =>
  ({
    info: { width: w, height: h, pip: opts.pip },
    photo: opts.photo ? { width: opts.photo[0], height: opts.photo[1] } : null,
    presentation: { align: opts.align },
  }) as unknown as Doc;

describe("computePlacement", () => {
  it("thermal base is the identity", () => {
    expect(computePlacement(doc(160, 120), "thermal", null)).toEqual({ src: [0, 0, 160, 120], dst: [0, 0, 160, 120] });
  });
  it("composite without a placement is the identity", () => {
    expect(computePlacement(doc(160, 120), "composite", null)).toEqual({ src: [0, 0, 160, 120], dst: [0, 0, 160, 120] });
  });
  it("composite uses the given placement", () => {
    const p = { src: [0, 0, 1, 1], dst: [2, 3, 4, 5] } as const;
    expect(computePlacement(doc(160, 120), "composite", { src: [...p.src], dst: [...p.dst] })).toEqual({ src: [0, 0, 1, 1], dst: [2, 3, 4, 5] });
  });
  it("photo extends the PiP mapping to the whole frame", () => {
    expect(computePlacement(doc(160, 120, { pip, photo: [640, 480] }), "photo", null)).toEqual({ src: [0, 0, 160, 120], dst: [10, 0, 320, 240] });
  });
  it("photo applies the manual alignment around the centre", () => {
    expect(computePlacement(doc(160, 120, { pip, photo: [640, 480], align: [5, -3, 1.5] }), "photo", null)).toEqual({ src: [0, 0, 160, 120], dst: [-65, -63, 480, 360] });
  });
  it("an alignment scale under 0.05 is ignored", () => {
    expect(computePlacement(doc(160, 120, { pip, photo: [640, 480], align: [0, 0, 0.01] }), "photo", null).dst).toEqual([10, 0, 320, 240]);
  });
  it("matches the Rust placement tests", () => {
    const p = { source: { x: 160, y: 120, w: 320, h: 240 }, destination: { x: 1000, y: 800, w: 400, h: 300 } };
    expect(computePlacement(doc(640, 480, { pip: p, photo: [2592, 1944] }), "photo", null).dst).toEqual([800, 650, 800, 600]);
    const d = computePlacement(doc(10, 10, { photo: [100, 100], align: [5, -3, 1.1] }), "photo", null).dst;
    expect(d[0]).toBeCloseTo(0, 4);
    expect(d[1]).toBeCloseTo(-8, 4);
    expect(d[2]).toBeCloseTo(110, 4);
    expect(d[3]).toBeCloseTo(110, 4);
  });
});

describe("shape geometry", () => {
  it("moved clamps a spot to the image", () => {
    const s: Shape = { kind: "spot", x: 5, y: 5, label: "" } as unknown as Shape;
    expect(moved(s, -10, 3, 20, 20)).toMatchObject({ x: 0, y: 8 });
  });
  it("moved keeps a box inside the image", () => {
    const s: Shape = { kind: "box", x: 2, y: 2, w: 4, h: 4 };
    expect(moved(s, 100, 0, 10, 10)).toMatchObject({ x: 6, y: 2 });
  });
  it("resized moves a box corner and keeps the opposite one", () => {
    const s: Shape = { kind: "box", x: 0, y: 0, w: 10, h: 10 };
    expect(resized(s, 0, [2, 3])).toMatchObject({ x: 2, y: 3, w: 8, h: 7 });
  });
  it("resized leaves a spot alone", () => {
    const s = { kind: "spot", x: 1, y: 1 } as unknown as Shape;
    expect(resized(s, 0, [5, 5])).toBe(s);
  });
  it("normalized makes box sizes positive", () => {
    expect(normalized({ kind: "box", x: 10, y: 10, w: -4, h: -2 })).toEqual({ kind: "box", x: 6, y: 8, w: 4, h: 2 });
  });
  it("normalized leaves a spot alone", () => {
    const s = { kind: "spot", x: 1, y: 1 } as unknown as Shape;
    expect(normalized(s)).toBe(s);
  });
  it("tooSmall flags degenerate boxes but not spots", () => {
    expect(tooSmall({ kind: "box", x: 0, y: 0, w: 0.5, h: 10 })).toBe(true);
    expect(tooSmall({ kind: "box", x: 0, y: 0, w: 5, h: 5 })).toBe(false);
    expect(tooSmall({ kind: "spot", x: 0, y: 0 } as unknown as Shape)).toBe(false);
  });
});

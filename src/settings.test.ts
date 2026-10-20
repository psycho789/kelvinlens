import { afterEach, describe, expect, it, vi } from "vitest";
import { parseLastReport, parseSettings } from "./settings";
import { loadSettings } from "./state";

afterEach(() => vi.unstubAllGlobals());

describe("parseSettings", () => {
  it("returns {} for non-objects", () => {
    for (const v of [null, 5, "x", [], undefined]) expect(parseSettings(v)).toEqual({});
  });
  it("keeps valid keys and drops unknown and wrong-typed ones", () => {
    const out = parseSettings({ unit: "F", smooth: "yes", msxStrength: 0.5, bogus: 1, wandTolerance: NaN, paper: "a4" });
    expect(out).toEqual({ unit: "F", msxStrength: 0.5, paper: "a4" });
  });
  it("rejects an unknown unit", () => {
    expect(parseSettings({ unit: "R" })).toEqual({});
  });
  it("filters malformed custom palettes", () => {
    const ok = { id: "custom-1", name: "A", colors: [[0, 0, 0]] };
    expect(parseSettings({ customPalettes: [ok, { id: 3 }, null] }).customPalettes).toEqual([ok]);
  });
  it("validates nested inspect values", () => {
    expect(parseSettings({ inspect: { minDelta: "3", minPixels: 4 } }).inspect).toEqual({ minPixels: 4 });
  });
});

describe("parseLastReport", () => {
  const fallback = { title: "T", site: "", fields: {}, conditions: { insideK: null, outsideK: null, humidity: null, ambientK: null, basis: "similar" as const }, includePhotos: true };
  it("falls back for non-objects and fixes bad fields", () => {
    expect(parseLastReport(null, fallback)).toEqual(fallback);
    const out = parseLastReport({ title: 5, site: "S", fields: { a: "1", b: 2 }, conditions: { humidity: 40, basis: "x" } }, fallback);
    expect(out).toMatchObject({ title: "T", site: "S", fields: { a: "1" }, conditions: { humidity: 40, basis: "similar" } });
  });
});

describe("loadSettings", () => {
  it("survives corrupt JSON", () => {
    vi.stubGlobal("localStorage", { getItem: () => "{not json" });
    expect(loadSettings()).toEqual({});
  });
  it("applies the validator to stored data", () => {
    vi.stubGlobal("localStorage", { getItem: () => '{"unit":"K","junk":true}' });
    expect(loadSettings()).toEqual({ unit: "K" });
  });
});

import { describe, expect, it } from "vitest";
import { hexRgb, rgbHex, stopsToColors } from "./color";

describe("color", () => {
  it("round-trips hex", () => {
    expect(rgbHex([255, 0, 16])).toBe("#ff0010");
    expect(hexRgb("#ff0010")).toEqual([255, 0, 16]);
  });
  it("stopsToColors interpolates 256 entries", () => {
    const c = stopsToColors([[0, [0, 0, 0]], [1, [255, 255, 255]]]);
    expect(c).toHaveLength(256);
    expect(c[0]).toEqual([0, 0, 0]);
    expect(c[255]).toEqual([255, 255, 255]);
    expect(c[128]).toEqual([128, 128, 128]);
  });
  it("stopsToColors handles 0 and 1 stops", () => {
    expect(stopsToColors([])[7]).toEqual([128, 128, 128]);
    expect(stopsToColors([[0.5, [1, 2, 3]]])[200]).toEqual([1, 2, 3]);
  });
  it("stopsToColors sorts unordered stops", () => {
    const c = stopsToColors([[1, [255, 255, 255]], [0, [0, 0, 0]]]);
    expect(c[0]).toEqual([0, 0, 0]);
  });
});

import { describe, expect, it } from "vitest";
import { deltaFromKelvin, fromKelvin, symbol, temp, toKelvin } from "./units";

describe("units", () => {
  it("converts kelvin to C, F, K", () => {
    expect(fromKelvin(273.15, "C")).toBeCloseTo(0, 10);
    expect(fromKelvin(273.15, "F")).toBeCloseTo(32, 10);
    expect(fromKelvin(300, "K")).toBe(300);
  });
  it("round-trips through toKelvin", () => {
    for (const u of ["C", "F", "K"] as const) expect(toKelvin(fromKelvin(310, u), u)).toBeCloseTo(310, 9);
  });
  it("scales deltas only for F", () => {
    expect(deltaFromKelvin(10, "F")).toBeCloseTo(18, 10);
    expect(deltaFromKelvin(10, "C")).toBe(10);
  });
  it("formats", () => {
    expect(temp(293.15, "C")).toBe("20.0 °C");
    expect(temp(null, "C")).toBe("–");
    expect(symbol("K")).toBe("K");
  });
});

import { describe, expect, it } from "vitest";
import { parseSettingsFile, type SettingsFile } from "./settingsFile";

describe("parseSettingsFile", () => {
  it("keeps well-typed fields", () => {
    const out = parseSettingsFile(JSON.stringify({ kelvinlensSettings: 1, palette: "iron", range: [280, 320], distribution: "temperatureLinear" }));
    expect(out).toEqual({ kelvinlensSettings: 1, palette: "iron", range: [280, 320], distribution: "temperatureLinear" });
  });
  it("rejects other versions", () => {
    expect(() => parseSettingsFile(JSON.stringify({ kelvinlensSettings: 2 }))).toThrow("Not a Kelvinlens settings file");
  });
  it("drops wrong-typed fields", () => {
    expect(parseSettingsFile(JSON.stringify({ kelvinlensSettings: 1, palette: 5, range: "x" }))).toEqual({ kelvinlensSettings: 1 });
  });
  it("throws on invalid JSON", () => {
    expect(() => parseSettingsFile("{nope")).toThrow();
  });
  it("round-trips an exported file", () => {
    const full = {
      kelvinlensSettings: 1,
      params: { emissivity: 0.9 },
      palette: "rainbow",
      range: null,
      distribution: "histogram",
      isotherms: [{ id: 1 }],
      measurements: [{ kind: "spot" }],
      formulas: [],
      presentation: { crop: null },
    } as unknown as SettingsFile;
    expect(parseSettingsFile(JSON.stringify(full))).toEqual(full);
  });
});

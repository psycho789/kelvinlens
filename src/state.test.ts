import { describe, expect, it } from "vitest";
import { finiteRange, isArea, isPath, nextLabel, type Doc } from "./state";

const doc = (labels: string[]) => ({ measurements: labels.map((label) => ({ label })) }) as unknown as Doc;

describe("state helpers", () => {
  it("nextLabel picks the first free number per kind", () => {
    expect(nextLabel(doc([]), "spot")).toBe("Sp1");
    expect(nextLabel(doc(["Sp1", "Sp2", "Bx1"]), "spot")).toBe("Sp3");
    expect(nextLabel(doc(["Sp2"]), "spot")).toBe("Sp1");
    expect(nextLabel(doc(["Bx1"]), "box")).toBe("Bx2");
  });
  it("finiteRange skips NaN and gives null when an infinity is present", () => {
    expect(finiteRange(new Float32Array([3, NaN, 1, 2]))).toEqual([1, 3]);
    expect(finiteRange(new Float32Array([3, 1, Infinity]))).toBeNull();
    expect(finiteRange(new Float32Array([NaN]))).toBeNull();
  });
  it("shape classes", () => {
    expect(isArea("box")).toBe(true);
    expect(isArea("spot")).toBe(false);
    expect(isPath("polyline")).toBe(true);
  });
});

import { expect, it } from "vitest";
import { nudgeDelta, viewerLabel } from "./a11y";

it("nudgeDelta maps arrows and Shift", () => {
  expect(nudgeDelta("ArrowLeft", false)).toEqual([-1, 0]);
  expect(nudgeDelta("ArrowDown", true)).toEqual([0, 10]);
  expect(nudgeDelta("a", false)).toBeNull();
});
it("viewerLabel names the file, size and range", () => {
  expect(viewerLabel(null, "C")).toBe("No image open");
  const doc = { info: { name: "IR_1.jpg", width: 160, height: 120 }, range: [293.15, 303.15], dataRange: null } as never;
  expect(viewerLabel(doc, "C")).toContain("IR_1.jpg, 160 by 120 pixels, 20.0");
});

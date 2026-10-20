// Geometry helpers. Thermal-on-photo placement tests live in viewerGeometry.test.ts.
import { expect, it } from "vitest";
import { handles, labelPoint } from "./overlays";

it("labelPoint of a box with negative size is its top-left corner", () => {
  expect(labelPoint({ kind: "box", x: 10, y: 20, w: -4, h: -6 })).toEqual([6, 14]);
});
it("handles: spot has none, box has four corners", () => {
  expect(handles({ kind: "spot", x: 1, y: 2 })).toEqual([]);
  expect(handles({ kind: "box", x: 0, y: 0, w: 2, h: 3 })).toEqual([[0, 0], [2, 0], [0, 3], [2, 3]]);
});
it("labelPoint of an empty polygon falls back to the origin", () => {
  expect(labelPoint({ kind: "polygon", points: [] })).toEqual([0, 0]);
});

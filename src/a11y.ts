import type { Unit } from "./api";
import { type Doc, displayRange } from "./state";
import { temp } from "./units";

/** Accessible name of the image canvas. The Measurements section of the side panel is the text equivalent. */
export function viewerLabel(doc: Pick<Doc, "info" | "range" | "dataRange"> | null, unit: Unit): string {
  if (!doc) return "No image open";
  const [lo, hi] = displayRange(doc as Doc);
  return `Thermal image ${doc.info.name}, ${doc.info.width} by ${doc.info.height} pixels, ${temp(lo, unit)} to ${temp(hi, unit)}. Measurements are listed in the side panel.`;
}

/** Arrow key to a pixel step, 10 with Shift; null for other keys. */
export function nudgeDelta(key: string, shift: boolean): [number, number] | null {
  const s = shift ? 10 : 1;
  switch (key) {
    case "ArrowLeft":
      return [-s, 0];
    case "ArrowRight":
      return [s, 0];
    case "ArrowUp":
      return [0, -s];
    case "ArrowDown":
      return [0, s];
    default:
      return null;
  }
}

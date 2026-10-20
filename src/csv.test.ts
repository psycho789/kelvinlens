import { expect, it } from "vitest";
import { csvCell } from "./csv";

it("csvCell quotes only when needed", () => {
  expect(csvCell("plain")).toBe("plain");
  expect(csvCell("a,b")).toBe('"a,b"');
  expect(csvCell('say "hi"')).toBe('"say ""hi"""');
  expect(csvCell("two\nlines")).toBe('"two\nlines"');
});

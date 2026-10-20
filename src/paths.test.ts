import { describe, expect, it } from "vitest";
import { basename, join, siblingPath, stem } from "./paths";

describe("paths", () => {
  it("stem drops only the last extension", () => {
    expect(stem("IR_0001.jpg")).toBe("IR_0001");
    expect(stem("a.b.c.csq")).toBe("a.b.c");
    expect(stem("noext")).toBe("noext");
  });
  it("basename handles both separators", () => {
    expect(basename("/a/b/c.jpg")).toBe("c.jpg");
    expect(basename("C:\\a\\b\\c.jpg")).toBe("c.jpg");
  });
  it("join keeps the folder's separator and avoids doubles", () => {
    expect(join("/a/b", "x")).toBe("/a/b/x");
    expect(join("/a/b/", "x")).toBe("/a/b/x");
    expect(join("C:\\a", "x")).toBe("C:\\a\\x");
  });
  it("siblingPath replaces the file name", () => {
    expect(siblingPath("/a/b/c.jpg", "d.csv")).toBe("/a/b/d.csv");
    expect(siblingPath("C:\\a\\c.jpg", "d.csv")).toBe("C:\\a\\d.csv");
    expect(siblingPath("c.jpg", "d.csv")).toBe("d.csv");
  });
});

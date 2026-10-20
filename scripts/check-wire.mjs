import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { errorText } from "../src/errors.ts";
import { parseFuseHeader, FUSE_HEADER_BYTES } from "../src/fuseWire.ts";
const root = (p) => fileURLToPath(new URL(`../${p}`, import.meta.url));
const hex = readFileSync(root("crates/kelvinlens-core/tests/fixtures/fuse-wire.hex"), "utf8").replace(/\s+/g, "");
const bytes = Uint8Array.from(hex.match(/../g).map((b) => parseInt(b, 16)));
assert.equal(bytes.length, FUSE_HEADER_BYTES + 8);
assert.deepEqual(parseFuseHeader(bytes.buffer), { width: 2, height: 1, source: [0, 0, 4, 2], destination: [1, 2, 8, 4] });
assert.deepEqual([...bytes.slice(FUSE_HEADER_BYTES)], [10, 20, 30, 255, 40, 50, 60, 255]);
assert.equal(errorText({ kind: "io", message: "File error: gone" }), "File error: gone");
assert.equal(errorText({ kind: "internal", message: "lock" }), "Internal error: lock");
assert.equal(errorText("invalid args `id`"), "invalid args `id`"); // Tauri's own failures stay strings
assert.equal(errorText(new Error("boom")), "boom");
assert.equal(errorText({ code: 1 }), '{"code":1}');
console.log("wire layout ok");

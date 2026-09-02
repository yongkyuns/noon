import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const noonSource = await readFile(new URL("./python/noon.py", import.meta.url), "utf8");
const allMatch = noonSource.match(/__all__\s*=\s*\[([\s\S]*?)\]\s*$/);
assert.ok(allMatch, "noon.py must keep an explicit public star-export contract");

const exported = new Set(
  [...allMatch[1].matchAll(/["']([A-Z][A-Z0-9_]*)["']/g)].map((match) => match[1]),
);

for (const family of ["BLUE", "TEAL", "GREEN", "YELLOW", "RED", "PURPLE", "GRAY", "GREY"]) {
  for (const shade of ["A", "B", "C", "D", "E"]) {
    assert.ok(
      exported.has(`${family}_${shade}`),
      `from noon import * must export ${family}_${shade}`,
    );
  }
}

console.log("✓ Noon star imports expose complete Manim palette families");

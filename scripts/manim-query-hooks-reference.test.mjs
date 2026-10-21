import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

const fixture = JSON.parse(await readFile(new URL("../fixtures/manim-v0.21/query-hooks.json", import.meta.url), "utf8"));

test("query-hook observations retain the exact pinned generator and public probe", async () => {
  assert.equal(fixture.manimVersion, "0.21.0");
  assert.equal(fixture.cases.length, 280);
  for (const [path, hash] of [["./manim-query-hooks-reference.py", fixture.generatorSha256],
    ["../web/python/examples/manim_parity_query_hooks.py", fixture.probeSha256]]) {
    assert.equal(createHash("sha256").update(await readFile(new URL(path, import.meta.url))).digest("hex"), hash);
  }
  for (const name of ["__len__", "__iter__", "__getitem__", "split", "has_points", "has_no_points"]) {
    assert.match(fixture.methodSourceSha256[name], /^[0-9a-f]{64}$/);
  }
});

test("inherited queries keep split and point-predicate callbacks independent", () => {
  const unique = new Set();
  for (const row of fixture.cases) {
    assert.equal(unique.has(JSON.stringify(row.spec)), false);
    unique.add(JSON.stringify(row.spec));
    if (row.spec.hook.startsWith("split_") && row.spec.operation !== "split") {
      assert.deepEqual(row.events, [], JSON.stringify(row.spec));
    }
    if (row.spec.hook.startsWith("has_points_") && row.spec.operation === "split") {
      assert.deepEqual(row.events, [], JSON.stringify(row.spec));
    }
  }
});

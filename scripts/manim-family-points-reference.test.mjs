import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

const fixture = JSON.parse(readFileSync(new URL("../fixtures/manim-v0.21/family-points.json", import.meta.url), "utf8"));

test("family point golden records the exact pinned reference generator", () => {
  const generator = readFileSync(new URL("./manim-family-points-reference.py", import.meta.url));
  assert.equal(fixture.generatorSha256, createHash("sha256").update(generator).digest("hex"));
  assert.equal(fixture.schemaVersion, 1);
  assert.equal(fixture.manimVersion, "0.21.0");
  assert.deepEqual(Object.keys(fixture.methodSourceSha256).sort(),
    ["__getattr__", "__getitem__", "__iter__", "__len__", "add", "remove", "split"].sort());
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[0-9a-f]{64}$/);
});

test("family point golden retains every unique scalar mutation control", () => {
  const expected = new Set();
  for (const kind of ["Group", "VGroup"])
    for (const ownPoints of [0, 1, 4, 5])
      for (const action of ["none", "set_points", "clear_points", "remove_first", "clear_children", "append_extra"])
        for (const index of [0, 1, 2, -1, -3, 3, 10])
          expected.add(JSON.stringify({ class: kind, ownPoints, action, index }));
  assert.equal(fixture.cases.length, expected.size);
  const actual = new Set(fixture.cases.map(row => JSON.stringify(row.spec)));
  assert.equal(actual.size, fixture.cases.length);
  assert.deepEqual(actual, expected);
});

test("controls distinguish old normalization from the current own-point branch", () => {
  const find = spec => fixture.cases.find(row => JSON.stringify(row.spec) === JSON.stringify(spec));
  const cleared = find({ class: "Group", ownPoints: 4, action: "clear_points", index: -1 });
  assert.equal(cleared.before.length, 3);
  assert.equal(cleared.after.length, 2);
  assert.deepEqual(cleared.result, { exception: "IndexError", message: "list index out of range" });
  const created = find({ class: "VGroup", ownPoints: 0, action: "set_points", index: 0 });
  assert.equal(created.result, "self");
  const outside = find({ class: "VGroup", ownPoints: 0, action: "set_points", index: 10 });
  assert.deepEqual(outside.result, { exception: "IndexError", message: "range object index out of range" });
  assert.equal(outside.after.points, 4);
});

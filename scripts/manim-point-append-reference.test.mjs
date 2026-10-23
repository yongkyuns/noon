import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

const fixture = JSON.parse(readFileSync(new URL("../fixtures/manim-v0.21/point-append.json", import.meta.url)));
test("point append controls pin the unchanged CE observations and shared observer", () => {
  assert.equal(fixture.manimVersion, "0.21.0");
  for (const [field, file] of [["generatorSha256", "./manim-point-append-reference.py"], ["observerSha256", "./manim_point_append_observation.py"]]) {
    assert.equal(fixture[field], createHash("sha256").update(readFileSync(new URL(file, import.meta.url))).digest("hex"));
  }
  assert.deepEqual(Object.keys(fixture.methodSourceSha256), ["append_points", "add_cubic_bezier_curves", "add_subpath"]);
  for (const hash of Object.values(fixture.methodSourceSha256)) assert.match(hash, /^[a-f0-9]{64}$/);
  assert.equal(fixture.cases.length, 84);
  assert.deepEqual(fixture.familyCases, fixture.cases);
  assert.equal(fixture.hookCases.length, 28);
  assert.ok(fixture.cases.every(row => row.inputUnchanged));
  assert.ok(fixture.cases.every(row => row.pointsXYZ.every(point => point.length === 3)));
  for (const row of fixture.cases) {
    assert.deepEqual(row.pointsXYZ.map(([x, y]) => [x, y]), row.pointsXY);
  }
  assert.equal(fixture.cases.filter(row => row.pointsXYZ.some(point => point[2] !== 0)).length, 4);
  assert.deepEqual([...new Set(fixture.cases.map(row => row.prefixRows))], [0, 4]);
});

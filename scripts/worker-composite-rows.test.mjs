import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const workerSource = readFileSync(new URL("../web/python-worker.source.js", import.meta.url), "utf8");
class WasmAuthoringMobjectHandle {
  constructor(memberCount) { this.memberCount = memberCount; }
}
class WasmAuthoringFamilyHandle {
  constructor(memberCount) { this.memberCount = memberCount; }
}
class WasmCompositeRows {
  rows = [];
  beginRow() { this.rows.push([]); }
  appendEntry(entry) {
    assert.ok(entry instanceof WasmAuthoringMobjectHandle, "expected instance of WasmAuthoringMobjectHandle");
    this.rows.at(-1).push({ kind: "object", entry });
  }
  appendFamilyEntry(entry) {
    assert.ok(entry instanceof WasmAuthoringFamilyHandle, "expected instance of WasmAuthoringFamilyHandle");
    this.rows.at(-1).push({ kind: "family", entry });
  }
}
const start = workerSource.indexOf("  const isFamilyHandle = (handle) =>");
const end = workerSource.indexOf("\n  self.noonTableOptions =", start);
assert.ok(start >= 0 && end > start, "exercise the production worker table-entry boundary");
const host = {};
const compositeRows = vm.runInNewContext(workerSource.slice(start, end) + "\ncompositeRows;", {
  WasmCompositeRows, WasmAuthoringFamilyHandle, self: host,
});

test("Python and JavaScript share the same typed handle-kind probe", () => {
  assert.equal(host.noonIsAuthoringFamilyHandle(new WasmAuthoringMobjectHandle(2)), false);
  assert.equal(host.noonIsAuthoringFamilyHandle(new WasmAuthoringFamilyHandle(0)), true);
});

test("ordinary objects remain object entries after gaining member queries", () => {
  for (const count of [0, 2]) {
    const object = new WasmAuthoringMobjectHandle(count);
    const result = compositeRows([[object]]);
    assert.deepEqual(result.rows, [[{ kind: "object", entry: object }]]);
  }
});
test("point-free and populated families retain their typed entry boundary", () => {
  for (const count of [0, 2]) {
    const family = new WasmAuthoringFamilyHandle(count);
    const result = compositeRows([[family]]);
    assert.deepEqual(result.rows, [[{ kind: "family", entry: family }]]);
  }
});
test("mixed row order and empty rows do not alter handle type", () => {
  const object = new WasmAuthoringMobjectHandle(1);
  const family = new WasmAuthoringFamilyHandle(0);
  const result = compositeRows([[object, family], [], [family, object]]);
  assert.deepEqual(result.rows, [
    [{ kind: "object", entry: object }, { kind: "family", entry: family }], [],
    [{ kind: "family", entry: family }, { kind: "object", entry: object }],
  ]);
});

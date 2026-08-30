import assert from "node:assert/strict";
import test from "node:test";

import {
  referenceInventoryFingerprint,
  validateReferenceLedger,
} from "./manim-reference-ledger.mjs";

const upstream = {
  repository: "ManimCommunity/manim",
  version: "v0.21.0",
  revision: "861cd4849b17db1db3515b531ffe80b297848f93",
};

function example(sourcePath, directiveLine, name, hashByte) {
  return {
    source_path: sourcePath,
    directive_line: directiveLine,
    name,
    source_sha256: hashByte.repeat(64),
  };
}

function fixture() {
  const examples = [
    example("docs/source/a.rst", 10, "First", "a"),
    example("manim/mobject/b.py", 20, null, "b"),
  ];
  return {
    inventory: { schema_version: 1, upstream, examples },
    ledger: {
      schema_version: 1,
      upstream,
      example_count: examples.length,
      provenance_sha256: referenceInventoryFingerprint(examples),
    },
  };
}

test("accepts an exact pinned reference inventory lock", () => {
  const { inventory, ledger } = fixture();
  assert.deepEqual(validateReferenceLedger(inventory, ledger), {
    schema_version: 1,
    upstream,
    tracked_examples: 2,
    provenance_sha256: ledger.provenance_sha256,
  });
});

test("rejects added or removed pinned examples", () => {
  const { inventory, ledger } = fixture();
  inventory.examples.pop();
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /reference example count drift: expected 2, extracted 1/u,
  );
});

test("rejects moved, renamed, or source-changed reference examples", () => {
  for (const mutation of [
    (exampleValue) => ({ ...exampleValue, source_path: "docs/source/moved.rst" }),
    (exampleValue) => ({ ...exampleValue, name: "Renamed" }),
    (exampleValue) => ({ ...exampleValue, source_sha256: "c".repeat(64) }),
  ]) {
    const { inventory, ledger } = fixture();
    inventory.examples[0] = mutation(inventory.examples[0]);
    assert.throws(
      () => validateReferenceLedger(inventory, ledger),
      /reference inventory provenance drift/u,
    );
  }
});

test("fingerprint ignores directive options and extracted source payload", () => {
  const { inventory } = fixture();
  const baseline = referenceInventoryFingerprint(inventory.examples);
  inventory.examples[0] = {
    ...inventory.examples[0],
    options: { save_last_frame: true },
    source: "class First(Scene): pass",
  };
  assert.equal(referenceInventoryFingerprint(inventory.examples), baseline);
});

test("rejects upstream revision drift", () => {
  const { inventory, ledger } = fixture();
  ledger.upstream = { ...upstream, revision: "deadbeef" };
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /ledger upstream provenance does not match extracted inventory/u,
  );
});

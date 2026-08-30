import assert from "node:assert/strict";
import test from "node:test";

import { validateReferenceLedger } from "./manim-reference-ledger.mjs";

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
  const first = example("docs/source/a.rst", 10, "First", "a");
  const second = example("manim/mobject/b.py", 20, null, "b");
  return {
    inventory: { schema_version: 1, upstream, examples: [first, second] },
    ledger: { schema_version: 1, upstream, entries: [first, second] },
  };
}

test("accepts an exact pinned reference ledger", () => {
  const { inventory, ledger } = fixture();
  assert.deepEqual(validateReferenceLedger(inventory, ledger), {
    schema_version: 1,
    upstream,
    tracked_examples: 2,
  });
});

test("rejects silently missing pinned examples", () => {
  const { inventory, ledger } = fixture();
  ledger.entries.pop();
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /ledger is missing 1 pinned reference example/u,
  );
});

test("rejects stale source provenance at the same directive location", () => {
  const { inventory, ledger } = fixture();
  ledger.entries[0] = { ...ledger.entries[0], source_sha256: "c".repeat(64) };
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /ledger provenance drift at docs\/source\/a\.rst:10/u,
  );
});

test("rejects extra and duplicate ledger locations", () => {
  const { inventory, ledger } = fixture();
  ledger.entries.push(example("docs/source/extra.rst", 30, "Extra", "d"));
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /ledger contains reference absent from pinned inventory/u,
  );

  const duplicate = fixture();
  duplicate.ledger.entries.push({ ...duplicate.ledger.entries[0] });
  assert.throws(
    () => validateReferenceLedger(duplicate.inventory, duplicate.ledger),
    /duplicate ledger reference location/u,
  );
});

test("rejects upstream revision drift", () => {
  const { inventory, ledger } = fixture();
  ledger.upstream = { ...upstream, revision: "deadbeef" };
  assert.throws(
    () => validateReferenceLedger(inventory, ledger),
    /ledger upstream provenance does not match extracted inventory/u,
  );
});

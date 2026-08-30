import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizedSourceHash,
  reconcileReferenceCoverage,
} from "./manim-reference-coverage.mjs";

const SOURCE_A = "from manim import *\n\nclass DotExample(Scene):\n    pass\n";
const SOURCE_B = "from manim import *\n\nclass OtherExample(Scene):\n    pass\n";

function inventory(examples) {
  return {
    schema_version: 1,
    upstream: { repository: "ManimCommunity/manim", version: "v0.21.0", revision: "abc" },
    examples,
  };
}

function example(name, source, line = 10) {
  return {
    source_path: `docs/source/reference/${name}.py`,
    directive_line: line,
    name,
    source_sha256: normalizedSourceHash(source),
  };
}

function manifest(entries) {
  return { entries };
}

function exactEntry(overrides = {}) {
  return {
    id: "dot-example",
    title: "DotExample",
    status: "ready",
    upstream: "reference/manim.mobject.geometry.arc.Dot.html",
    upstream_source: "fixtures/dot.py",
    reuse: "source-equivalent-manim-v0.21",
    ...overrides,
  };
}

function sourceReader(sources) {
  return async (file) => {
    const key = file.replaceAll("\\", "/").split("/").at(-1);
    if (!(key in sources)) {
      throw new Error(`missing fixture ${key}`);
    }
    return sources[key];
  };
}

test("normalizes line endings and trailing whitespace before provenance hashing", () => {
  assert.equal(normalizedSourceHash("a\r\nb\r\n"), normalizedSourceHash("a\nb"));
});

test("reconciles exact-source reference entries by immutable source provenance", async () => {
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", SOURCE_A), example("OtherExample", SOURCE_B, 20)]),
    manifest([
      exactEntry(),
      { id: "quickstart", status: "ready", upstream: "tutorials/quickstart.html" },
    ]),
    { schema_version: 1, minimum_reconciled_examples: 1 },
    { readSource: sourceReader({ "dot.py": SOURCE_A }), repositoryRoot: "/repo" },
  );

  assert.equal(report.inventory_examples, 2);
  assert.equal(report.manifest_reference_entries, 1);
  assert.equal(report.reconciled_examples, 1);
  assert.equal(report.unclassified_inventory_examples, 1);
  assert.equal(report.reconciled[0].inventory.name, "DotExample");
});

test("uses the manifest title to disambiguate identical directive source", async () => {
  const duplicate = example("OtherName", SOURCE_A, 30);
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", SOURCE_A), duplicate]),
    manifest([exactEntry()]),
    { schema_version: 1, minimum_reconciled_examples: 1 },
    { readSource: sourceReader({ "dot.py": SOURCE_A }), repositoryRoot: "/repo" },
  );

  assert.equal(report.reconciled[0].inventory.name, "DotExample");
});

test("fails when an exact-source fixture no longer identifies one pinned directive", async () => {
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([example("OtherExample", SOURCE_B)]),
      manifest([exactEntry()]),
      { schema_version: 1, minimum_reconciled_examples: 0 },
      { readSource: sourceReader({ "dot.py": SOURCE_A }), repositoryRoot: "/repo" },
    ),
    /matched no extracted directive/u,
  );
});

test("ratchets the number of reconciled reference examples", async () => {
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([example("DotExample", SOURCE_A)]),
      manifest([exactEntry()]),
      { schema_version: 1, minimum_reconciled_examples: 2 },
      { readSource: sourceReader({ "dot.py": SOURCE_A }), repositoryRoot: "/repo" },
    ),
    /reference coverage regression/u,
  );
});

test("keeps blocked/deferred reference classifications visible without inventing source provenance", async () => {
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", SOURCE_A)]),
    manifest([
      {
        id: "blocked-reference",
        title: "BlockedReference",
        status: "blocked",
        upstream: "reference/manim.example.BlockedReference.html",
        dependency: "#123",
      },
    ]),
    { schema_version: 1, minimum_reconciled_examples: 0 },
    { readSource: sourceReader({}), repositoryRoot: "/repo" },
  );

  assert.equal(report.reconciled_examples, 0);
  assert.deepEqual(report.classified_without_exact_source, [
    {
      id: "blocked-reference",
      title: "BlockedReference",
      status: "blocked",
      upstream: "reference/manim.example.BlockedReference.html",
    },
  ]);
});

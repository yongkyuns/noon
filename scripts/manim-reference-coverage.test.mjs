import assert from "node:assert/strict";
import test from "node:test";

import {
  directiveSourceFromFixture,
  normalizedSourceHash,
  reconcileReferenceCoverage,
} from "./manim-reference-coverage.mjs";

const DIRECTIVE_A = "class DotExample(Scene):\n    pass";
const DIRECTIVE_B = "class OtherExample(Scene):\n    pass";
const FIXTURE_A = `from manim import *\n\n${DIRECTIVE_A}\n`;
const FIXTURE_B = `from manim import *\n\n${DIRECTIVE_B}\n`;

function inventory(examples) {
  return {
    schema_version: 1,
    upstream: { repository: "ManimCommunity/manim", version: "v0.21.0", revision: "abc" },
    examples,
  };
}

function example(name, source, line = 10) {
  return {
    source_path: `manim/example/${name}.py`,
    directive_line: line,
    name,
    source_sha256: normalizedSourceHash(source),
  };
}

function manifest(entries) {
  return { entries };
}

function coverageLock(minimumReconciled = 0, minimumClassified = minimumReconciled) {
  return {
    schema_version: 2,
    minimum_reconciled_examples: minimumReconciled,
    minimum_classified_examples: minimumClassified,
  };
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

function provenanceFor(value) {
  return {
    source_path: value.source_path,
    directive_line: value.directive_line,
    name: value.name,
    source_sha256: value.source_sha256,
  };
}

function classifiedEntry(value, overrides = {}) {
  return {
    id: "blocked-reference",
    title: value.name,
    status: "blocked",
    upstream: `reference/manim.example.${value.name}.html`,
    reference_provenance: provenanceFor(value),
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

test("removes only the runnable Manim import prelude before directive matching", () => {
  assert.equal(directiveSourceFromFixture(FIXTURE_A), DIRECTIVE_A);
  assert.equal(directiveSourceFromFixture(DIRECTIVE_A), DIRECTIVE_A);
  assert.equal(
    directiveSourceFromFixture(`import math\n\n${DIRECTIVE_A}\n`),
    `import math\n\n${DIRECTIVE_A}`,
  );
});

test("reconciles exact-source reference entries by immutable source provenance", async () => {
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", DIRECTIVE_A), example("OtherExample", DIRECTIVE_B, 20)]),
    manifest([
      exactEntry(),
      { id: "quickstart", status: "ready", upstream: "tutorials/quickstart.html" },
    ]),
    coverageLock(1, 1),
    { readSource: sourceReader({ "dot.py": FIXTURE_A }), repositoryRoot: "/repo" },
  );

  assert.equal(report.inventory_examples, 2);
  assert.equal(report.manifest_reference_entries, 1);
  assert.equal(report.reconciled_examples, 1);
  assert.equal(report.provenance_classified_examples, 0);
  assert.equal(report.classified_examples, 1);
  assert.equal(report.unclassified_inventory_examples, 1);
  assert.equal(report.reconciled[0].inventory.name, "DotExample");
});

test("uses the manifest title to disambiguate identical directive source", async () => {
  const duplicate = example("OtherName", DIRECTIVE_A, 30);
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", DIRECTIVE_A), duplicate]),
    manifest([exactEntry()]),
    coverageLock(1, 1),
    { readSource: sourceReader({ "dot.py": FIXTURE_A }), repositoryRoot: "/repo" },
  );

  assert.equal(report.reconciled[0].inventory.name, "DotExample");
});

test("fails when an exact-source fixture no longer identifies one pinned directive", async () => {
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([example("OtherExample", DIRECTIVE_B)]),
      manifest([exactEntry()]),
      coverageLock(),
      { readSource: sourceReader({ "dot.py": FIXTURE_A }), repositoryRoot: "/repo" },
    ),
    /matched no extracted directive/u,
  );
});

test("ratchets the number of reconciled reference examples", async () => {
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([example("DotExample", DIRECTIVE_A)]),
      manifest([exactEntry()]),
      coverageLock(2, 2),
      { readSource: sourceReader({ "dot.py": FIXTURE_A }), repositoryRoot: "/repo" },
    ),
    /reference coverage regression/u,
  );
});

test("keeps unprovenanced blocked/deferred reference entries visible but unclassified", async () => {
  const report = await reconcileReferenceCoverage(
    inventory([example("DotExample", DIRECTIVE_A)]),
    manifest([
      {
        id: "blocked-reference",
        title: "BlockedReference",
        status: "blocked",
        upstream: "reference/manim.example.BlockedReference.html",
        dependency: "#123",
      },
    ]),
    coverageLock(),
    { readSource: sourceReader({}), repositoryRoot: "/repo" },
  );

  assert.equal(report.reconciled_examples, 0);
  assert.equal(report.provenance_classified_examples, 0);
  assert.equal(report.classified_examples, 0);
  assert.deepEqual(report.classified_without_exact_source, [
    {
      id: "blocked-reference",
      title: "BlockedReference",
      status: "blocked",
      upstream: "reference/manim.example.BlockedReference.html",
    },
  ]);
});

test("classifies blocked/deferred reference entries by immutable inventory provenance", async () => {
  const blocked = example("DashedLineExample", DIRECTIVE_A);
  const deferred = example("TangentLineExample", DIRECTIVE_B, 20);
  const report = await reconcileReferenceCoverage(
    inventory([blocked, deferred]),
    manifest([
      classifiedEntry(blocked),
      classifiedEntry(deferred, {
        id: "deferred-reference",
        status: "deferred",
      }),
    ]),
    coverageLock(0, 2),
    { readSource: sourceReader({}), repositoryRoot: "/repo" },
  );

  assert.equal(report.reconciled_examples, 0);
  assert.equal(report.provenance_classified_examples, 2);
  assert.equal(report.classified_examples, 2);
  assert.equal(report.unclassified_inventory_examples, 0);
  assert.deepEqual(
    report.provenance_classified.map((entry) => entry.inventory.name),
    ["DashedLineExample", "TangentLineExample"],
  );
});

test("rejects provenance-only ready entries so supported examples keep exact-source proof", async () => {
  const pinned = example("DotExample", DIRECTIVE_A);
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([pinned]),
      manifest([classifiedEntry(pinned, { status: "ready" })]),
      coverageLock(),
      { readSource: sourceReader({}), repositoryRoot: "/repo" },
    ),
    /must be blocked or deferred/u,
  );
});

test("rejects stale provenance-only classifications", async () => {
  const pinned = example("DashedLineExample", DIRECTIVE_A);
  const stale = provenanceFor(pinned);
  stale.directive_line += 1;
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([pinned]),
      manifest([classifiedEntry(pinned, { reference_provenance: stale })]),
      coverageLock(),
      { readSource: sourceReader({}), repositoryRoot: "/repo" },
    ),
    /does not match one pinned directive/u,
  );
});

test("rejects duplicate claims across exact-source and provenance-only entries", async () => {
  const pinned = example("DotExample", DIRECTIVE_A);
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([pinned]),
      manifest([exactEntry(), classifiedEntry(pinned)]),
      coverageLock(1, 1),
      { readSource: sourceReader({ "dot.py": FIXTURE_A }), repositoryRoot: "/repo" },
    ),
    /multiple manifest entries map/u,
  );
});

test("ratchets total classified inventory independently from exact-source reconciliation", async () => {
  const pinned = example("DashedLineExample", DIRECTIVE_A);
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([pinned]),
      manifest([classifiedEntry(pinned)]),
      coverageLock(0, 2),
      { readSource: sourceReader({}), repositoryRoot: "/repo" },
    ),
    /reference classification regression/u,
  );
});

test("requires the classification ratchet to cover the exact-source ratchet", async () => {
  await assert.rejects(
    reconcileReferenceCoverage(
      inventory([]),
      manifest([]),
      coverageLock(2, 1),
      { readSource: sourceReader({}), repositoryRoot: "/repo" },
    ),
    /minimum_classified_examples must be at least minimum_reconciled_examples/u,
  );
});

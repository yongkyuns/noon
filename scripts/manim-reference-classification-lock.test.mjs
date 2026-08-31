import assert from "node:assert/strict";
import test from "node:test";

import { validateReferenceClassificationLock } from "./manim-reference-classification-lock.mjs";

const HASH = "a".repeat(64);

function lock(ids) {
  return {
    schema_version: 2,
    minimum_reconciled_examples: 0,
    minimum_classified_examples: ids.length,
    required_classified_entry_ids: ids,
  };
}

function exactEntry(id = "exact") {
  return {
    id,
    status: "ready",
    upstream: `reference/${id}.html`,
    upstream_source: `fixtures/${id}.py`,
    reuse: "source-equivalent-manim-v0.21",
  };
}

function provenanceEntry(id = "blocked") {
  return {
    id,
    status: "blocked",
    upstream: `reference/${id}.html`,
    reference_provenance: {
      source_path: "manim/example.py",
      directive_line: 10,
      name: "Example",
      source_sha256: HASH,
    },
  };
}

test("preserves exact-source and provenance-classified reference identities", () => {
  const report = validateReferenceClassificationLock(
    { entries: [exactEntry(), provenanceEntry()] },
    lock(["exact", "blocked"]),
  );
  assert.equal(report.required_classified_entries, 2);
});

test("fails if one locked classification disappears even when total coverage can be replaced", () => {
  assert.throws(
    () =>
      validateReferenceClassificationLock(
        { entries: [exactEntry("replacement"), provenanceEntry()] },
        lock(["exact", "blocked"]),
      ),
    /required classified reference entry exact is missing/u,
  );
});

test("does not let an unprovenanced blocked stub satisfy a locked classification", () => {
  assert.throws(
    () =>
      validateReferenceClassificationLock(
        {
          entries: [
            {
              id: "blocked",
              status: "blocked",
              upstream: "reference/blocked.html",
              dependency: "#123",
            },
          ],
        },
        lock(["blocked"]),
      ),
    /no longer carries pinned classification evidence/u,
  );
});

test("requires every count-ratcheted classification to have a locked identity", () => {
  assert.throws(
    () =>
      validateReferenceClassificationLock(
        { entries: [exactEntry()] },
        {
          ...lock(["exact"]),
          minimum_classified_examples: 2,
        },
      ),
    /must name exactly 2 required classified entry ids/u,
  );
});

test("rejects duplicate locked ids", () => {
  assert.throws(
    () =>
      validateReferenceClassificationLock(
        { entries: [exactEntry()] },
        {
          ...lock(["exact"]),
          minimum_classified_examples: 2,
          required_classified_entry_ids: ["exact", "exact"],
        },
      ),
    /duplicate required classified entry id exact/u,
  );
});

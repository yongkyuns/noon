import assert from "node:assert/strict";
import test from "node:test";
import { selectShowcaseEntries } from "./showcase-cases.mjs";

const entries = ["first", "second", "third"].map(id => ({ id: `showcase-${id}` }));

test("omitted selection retains the complete showcase inventory", () => {
  assert.deepEqual(selectShowcaseEntries(entries), entries);
});

test("explicit selection retains manifest order and exact entry objects", () => {
  assert.deepEqual(selectShowcaseEntries(entries, " showcase-third, showcase-first "),
    [entries[0], entries[2]]);
});

test("unknown, duplicate and empty selections cannot silently change scope", () => {
  for (const selection of ["", " ", ",", "showcase-first,", ",showcase-first",
    "showcase-first,,showcase-second", "showcase-first, showcase-first", "showcase-unknown"]) {
    assert.throws(() => selectShowcaseEntries(entries, selection), /empty selection|duplicate IDs|unknown showcase/);
  }
  assert.throws(() => selectShowcaseEntries(entries, null), /comma-separated list/);
});

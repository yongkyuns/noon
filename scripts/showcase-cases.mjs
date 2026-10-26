import assert from "node:assert/strict";

// Selection changes qualification scope, never the gallery manifest or its order.
export function selectShowcaseEntries(entries, selection) {
  assert.ok(Array.isArray(entries) && entries.length > 0, "showcase inventory is empty");
  const known = new Set(entries.map(entry => entry.id));
  assert.equal(known.size, entries.length, "showcase inventory contains duplicate IDs");
  if (selection === undefined) return [...entries];
  assert.equal(typeof selection, "string", "NOON_SHOWCASE_CASES must be a comma-separated list");
  const requested = selection.split(",").map(id => id.trim());
  assert.ok(requested.every(Boolean), "NOON_SHOWCASE_CASES contains an empty selection");
  const selected = new Set(requested);
  assert.equal(selected.size, requested.length, "NOON_SHOWCASE_CASES contains duplicate IDs");
  for (const id of selected) assert.ok(known.has(id), `unknown showcase case: ${id}`);
  return entries.filter(entry => selected.has(entry.id));
}

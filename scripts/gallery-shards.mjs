import assert from "node:assert/strict";

// One inventory is shared by the browser harness and the independent shard
// coverage tests; adding an example cannot silently bypass an existing browser.
export const galleryManifestPaths = Object.freeze([
  "python/examples/manim_tutorial_manifest.json",
  "python/examples/manim_compatibility_manifest.json",
  "python/examples/manim_stress_manifest.json",
  "python/examples/noon_showcase_manifest.json",
]);

export const curatedLessons = Object.freeze([
  "showcase-spatial-scene",
  "showcase-three-d-axes",
  "showcase-linear-algebra",
  "showcase-camera-follows-path",
  "showcase-text-math",
  "showcase-latex-create",
]);

export const affected = Object.freeze([
  "compatible-timed-composition",
  "parity-moving-dots",
  "parity-rotation-updater",
  "compatible-indicate-square",
  "showcase-always-redraw",
  ...curatedLessons,
]);

export function galleryEntriesFromManifests(manifests) {
  assert.equal(manifests.length, galleryManifestPaths.length, "gallery manifest inventory changed");
  const entries = manifests.slice(0, 3).flatMap(manifest => {
    assert.ok(Array.isArray(manifest.entries), "missing gallery manifest entries");
    return manifest.entries.filter(entry => entry.status === "ready");
  });
  const showcase = manifests[3];
  assert.ok(Array.isArray(showcase.entries), "missing showcase manifest entries");
  entries.push(...showcase.entries.filter(entry =>
    entry.performance || entry.id === "showcase-always-redraw" || curatedLessons.includes(entry.id)
  ).map(entry => ({ ...entry, expected_duration: entry.duration })));
  const ids = entries.map(entry => entry.id);
  assert.ok(ids.length > 0, "gallery inventory is empty");
  assert.ok(ids.every(id => typeof id === "string" && id.length > 0), "invalid gallery ID");
  assert.equal(new Set(ids).size, ids.length, "duplicate gallery IDs");
  for (const id of affected) assert.ok(ids.includes(id), id + " is no longer selectable");
  return entries;
}

// Preserve the original case ordering: first the regular inventory, then
// portable no-JSPI controls on Chromium and WebKit only.
export function galleryCaseQueue(entries, browserName) {
  assert.ok(["chromium", "firefox", "webkit"].includes(browserName), "unknown gallery browser");
  const queue = entries.map(entry => ({ entry, noJspi: false }));
  if (browserName !== "firefox") {
    for (const entry of entries) {
      if (affected.includes(entry.id)) queue.push({ entry, noJspi: true });
    }
  }
  return queue;
}

function caseCost(entry) {
  const rawDuration = Number(entry.expected_duration ?? entry.duration ?? 2);
  const duration = Number.isFinite(rawDuration) && rawDuration > 0 ? Math.min(rawDuration, 30) : 2;
  // Fixed deterministic weights; never use runner-measured timing to choose
  // the inventory. Give duplicate portable cases and stress scenes extra weight.
  return 1 + duration * 0.2 +
    (affected.includes(entry.id) ? 2 : 0) +
    (entry.performance || entry.id.includes("stress") ? 3 : 0);
}

// Stable longest-estimated-case-first greedy packing. Membership is identical
// across OSes, while the result preserves manifest order within each shard.
export function shardGalleryEntries(entries, shardIndex, shardCount) {
  assert.ok(Number.isSafeInteger(shardCount) && shardCount > 0 && shardCount <= 32,
    "gallery shard count must be an integer in 1..32");
  assert.ok(Number.isSafeInteger(shardIndex) && shardIndex >= 0 && shardIndex < shardCount,
    "gallery shard index is outside 0..count-1");
  const buckets = Array.from({ length: shardCount }, () => ({ total: 0, ids: new Set() }));
  const sorted = entries.map(entry => ({ entry, cost: caseCost(entry) }))
    .sort((a, b) => b.cost - a.cost || (a.entry.id < b.entry.id ? -1 : a.entry.id > b.entry.id ? 1 : 0));
  for (const { entry, cost } of sorted) {
    let lowest = 0;
    for (let index = 1; index < buckets.length; index++) {
      if (buckets[index].total < buckets[lowest].total) lowest = index;
    }
    buckets[lowest].total += cost;
    buckets[lowest].ids.add(entry.id);
  }
  return entries.filter(entry => buckets[shardIndex].ids.has(entry.id));
}

export function galleryShardFromEnvironment(env) {
  const rawIndex = env.NOON_GALLERY_SHARD_INDEX;
  const rawCount = env.NOON_GALLERY_SHARD_COUNT;
  if (rawIndex === undefined && rawCount === undefined) return null;
  assert.ok(typeof rawIndex === "string" && /^\d+$/.test(rawIndex),
    "NOON_GALLERY_SHARD_INDEX must be a nonnegative integer");
  assert.ok(typeof rawCount === "string" && /^\d+$/.test(rawCount),
    "NOON_GALLERY_SHARD_COUNT must be a positive integer");
  const index = Number(rawIndex);
  const count = Number(rawCount);
  shardGalleryEntries([], index, count);
  return { index, count };
}

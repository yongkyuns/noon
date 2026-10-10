import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

import {
  affected,
  galleryCaseQueue,
  galleryEntriesFromManifests,
  galleryManifestPaths,
  galleryShardFromEnvironment,
  shardGalleryEntries,
} from "./gallery-shards.mjs";

const manifests = await Promise.all(galleryManifestPaths.map(async path =>
  JSON.parse(await readFile(new URL("../web/" + path, import.meta.url), "utf8"))
));
const entries = galleryEntriesFromManifests(manifests);
const shardCount = 3;
// Freeze the portable cases independently of the implementation. Otherwise a
// future edit could delete a no-JSPI control from both runtime and assertions.
const requiredPortableControls = [
  "compatible-timed-composition",
  "parity-moving-dots",
  "parity-rotation-updater",
  "compatible-indicate-square",
  "showcase-always-redraw",
  "showcase-spatial-scene",
  "showcase-three-d-axes",
  "showcase-linear-algebra",
  "showcase-camera-follows-path",
  "showcase-text-math",
  "showcase-latex-create",
];
const caseKey = ({ entry, noJspi }) => entry.id + ":" + (noJspi ? "no-jspi" : "normal");

test("every current gallery entry and no-JSPI control is covered exactly once per browser", () => {
  assert.deepEqual([...affected], requiredPortableControls, "portable coverage policy was changed");
  for (const browser of ["chromium", "firefox", "webkit"]) {
    const expected = galleryCaseQueue(entries, browser).map(caseKey).sort();
    const shards = Array.from({ length: shardCount },
      (_, i) => shardGalleryEntries(entries, i, shardCount));
    assert.ok(shards.every(shard => shard.length > 0), browser + ": empty shard");
    const actual = shards.flatMap(shard => galleryCaseQueue(shard, browser)).map(caseKey).sort();
    assert.deepEqual(actual, expected, browser + ": missing or duplicated gallery cases");
    for (const id of requiredPortableControls) {
      assert.ok(actual.includes(id + ":normal"), browser + ": missing affected case " + id);
      assert.equal(actual.includes(id + ":no-jspi"), browser !== "firefox",
        browser + ": portable control mismatch for " + id);
    }
  }
});

test("shard assignment is stable regardless of manifest order", () => {
  for (let index = 0; index < shardCount; index++) {
    const actual = shardGalleryEntries(entries, index, shardCount).map(x => x.id).sort();
    const reversed = shardGalleryEntries([...entries].reverse(), index, shardCount).map(x => x.id).sort();
    assert.deepEqual(actual, reversed);
  }
});

test("one shard preserves the original order, and explicit selections partition exactly", () => {
  assert.deepEqual(shardGalleryEntries(entries, 0, 1), entries);
  const selected = entries.filter(entry => affected.includes(entry.id));
  const shards = Array.from({ length: shardCount },
    (_, i) => shardGalleryEntries(selected, i, shardCount));
  assert.deepEqual(shards.flatMap(x => x.map(entry => entry.id)).sort(),
    selected.map(entry => entry.id).sort());
});

test("incorrect matrix configuration fails closed", () => {
  assert.equal(galleryShardFromEnvironment({}), null);
  assert.deepEqual(galleryShardFromEnvironment({
    NOON_GALLERY_SHARD_INDEX: "2", NOON_GALLERY_SHARD_COUNT: "3",
  }), { index: 2, count: 3 });
  for (const [index, count] of [
    [undefined, "3"], ["0", undefined], ["-1", "3"], ["3", "3"],
    ["1.5", "3"], ["bad", "3"], ["0", "0"], ["0", "33"],
    ["9007199254740993", "3"],
  ]) {
    assert.throws(() => galleryShardFromEnvironment({
      NOON_GALLERY_SHARD_INDEX: index,
      NOON_GALLERY_SHARD_COUNT: count,
    }));
  }
});

test("empty and duplicate manifest inventories cannot silently pass admission", () => {
  const empty = structuredClone(manifests);
  for (const manifest of empty) manifest.entries = [];
  assert.throws(() => galleryEntriesFromManifests(empty), /empty/);
  const duplicate = structuredClone(manifests);
  duplicate[0].entries.push(structuredClone(duplicate[1].entries[0]));
  assert.throws(() => galleryEntriesFromManifests(duplicate), /duplicate/);
});

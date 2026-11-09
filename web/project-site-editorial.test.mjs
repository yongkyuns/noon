import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = relative => readFile(new URL(relative, import.meta.url), "utf8");

// The maintainer identified this scene as an imitation of another creator's
// content, made only to demonstrate engine capability. Gallery membership does
// not approve it for the project website, README, or their preview media.
// Keep the demonstration itself unchanged; this is an editorial boundary.
test("creator-derived demonstration is excluded from project presentation", async () => {
  const excluded = /showcase[-_]triangle[-_]light|light\s+in\s+a\s+triangle/i;
  for (const relative of ["../README.md", "./site/index.html", "./site/examples.json"]) {
    assert.doesNotMatch(await read(relative), excluded, relative);
  }
});

test("website videos and README GIFs use only the selected project examples", async () => {
  const selected = JSON.parse(await read("./site/examples.json")).map(entry => entry.id).sort();
  const html = await read("./site/index.html");
  const readme = await read("../README.md");
  const videos = [...html.matchAll(/src="\.\/site\/media\/(showcase-[a-z0-9-]+)\.mp4"/g)]
    .map(match => match[1]).sort();
  const gifs = [...readme.matchAll(/\/site\/media\/(showcase-[a-z0-9-]+)\.gif/g)]
    .map(match => match[1]).sort();
  assert.deepEqual(videos, selected);
  assert.deepEqual(gifs, selected);
  for (const id of selected) {
    assert.ok(html.includes(`example=${id}`), `${id}: editable website demo`);
    assert.ok(readme.includes(`example=${id}`), `${id}: editable README demo`);
  }
});

test("wave-interference lesson is deliberately selected for project examples", async () => {
  const selected = JSON.parse(await read("./site/examples.json"));
  const wave = selected.find(entry => entry.id === "showcase-wave-interference");
  assert.deepEqual(wave, { id: "showcase-wave-interference", start: 1, end: 9 });
  const manifest = JSON.parse(await read("./python/examples/noon_showcase_manifest.json"));
  const lesson = manifest.entries.find(entry => entry.id === wave.id);
  assert.equal(lesson.title, "Interference is addition");
  assert.equal(lesson.path, "python/examples/showcase_wave_interference.py");
  assert.ok(wave.end <= lesson.duration);
  for (const relative of ["../README.md", "./site/index.html"]) {
    const content = await read(relative);
    assert.ok(content.includes("Interference is addition"), relative);
    assert.ok(content.includes("example=showcase-wave-interference"), relative);
  }
});

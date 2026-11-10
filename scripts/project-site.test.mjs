import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { playgroundRedirect } from "../web/site/site.js";
import { buildProjectSite } from "./build-project-site.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = name => readFile(path.join(root, name), "utf8");

test("legacy query URLs preserve every parameter and fragment under a project prefix", () => {
  for (const base of ["https://example.test/", "https://example.test/noon/", "https://example.test/noon/index.html"]) {
    const url = new URL(`${base}?example=showcase-first-scene&catalog=showcase&backend=webgl&x=1&x=2#editor`);
    const redirected = new URL(playgroundRedirect(url));
    assert.equal(redirected.origin, url.origin);
    assert.equal(redirected.pathname, new URL("./playground.html", base).pathname);
    assert.equal(redirected.search, url.search);
    assert.equal(redirected.hash, url.hash);
  }
});

test("homepage and its section anchors do not redirect", () => {
  assert.equal(playgroundRedirect(new URL("https://example.test/noon/")), null);
  assert.equal(playgroundRedirect(new URL("https://example.test/noon/#start")), null);
});

test("every selected clip and editable link resolves to a current curated source", async () => {
  const selected = JSON.parse(await read("web/site/examples.json"));
  const gallery = JSON.parse(await read("web/python/examples/noon_showcase_manifest.json"));
  const html = await read("web/site/index.html");
  const readme = await read("README.md");
  assert.equal(new Set(selected.map(entry => entry.id)).size, selected.length);
  for (const { id, start, end } of selected) {
    const entry = gallery.entries.find(item => item.id === id);
    assert.ok(entry, id);
    assert.ok(Number.isFinite(start) && Number.isFinite(end) && start >= 0 && end > start && end <= entry.duration);
    await readFile(path.join(root, "web", entry.path));
    await readFile(path.join(root, "web", entry.thumbnail));
    assert.ok(html.includes(`${id}.mp4`) && html.includes(`example=${id}`), id);
    assert.ok(readme.includes(`${id}.gif`) && readme.includes(`example=${id}`), id);
  }
  for (const match of html.matchAll(/example=(showcase-[a-z0-9-]+)/g)) {
    assert.ok(gallery.entries.some(entry => entry.id === match[1]), match[1]);
  }
});

test("homepage is usable without JS, never autoplays, and loads no third-party presentation assets", async () => {
  const html = await read("web/site/index.html");
  assert.match(html, /class="skip" href="#main"/);
  assert.equal([...html.matchAll(/<h1\b/g)].length, 1);
  for (const [video] of html.matchAll(/<video\b[^>]*>/g)) {
    assert.match(video, /\bcontrols\b/);
    assert.match(video, /preload="none"/);
    assert.match(video, /aria-label="[^"]+"/);
    assert.doesNotMatch(video, /\bautoplay\b/);
  }
  assert.doesNotMatch(html, /(?:src|poster)="https?:/);
  for (const [, id] of html.matchAll(/href="#([^"]+)"/g)) assert.ok(html.includes(`id="${id}"`), id);
  assert.match(await read("web/site/site.css"), /prefers-reduced-motion/);
});

test("staging preserves the tested playground, runtime paths, build identity and input files", async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), "noon-site-test-"));
  try {
    await mkdir(path.join(temp, "web/site/media"), { recursive: true });
    const fixtures = {
      "index.html": "original playground", "main.js": "same module", "build-info.json": '{"commit":"test"}',
      "site/index.html": "new homepage", "site/examples.json": '[{"id":"showcase-test"}]',
      "site/media/showcase-test.gif": "GIF89a", "site/media/showcase-test.mp4": "0000ftyp",
      "site/media/provenance.json": "{}",
    };
    for (const [name, contents] of Object.entries(fixtures)) await writeFile(path.join(temp, "web", name), contents);
    const staged = await buildProjectSite(temp);
    assert.equal(await readFile(path.join(staged, "index.html"), "utf8"), "new homepage");
    assert.equal(await readFile(path.join(staged, "playground.html"), "utf8"), "original playground");
    for (const name of ["main.js", "build-info.json"]) assert.equal(await readFile(path.join(staged, name), "utf8"), fixtures[name]);
    for (const [name, contents] of Object.entries(fixtures)) assert.equal(await readFile(path.join(temp, "web", name), "utf8"), contents);
    await rm(path.join(temp, "web/site/media/showcase-test.gif"));
    await assert.rejects(buildProjectSite(temp), /ENOENT/);
    assert.equal(await readFile(path.join(staged, "index.html"), "utf8"), "new homepage", "failed admission must not delete a prior staged bundle");
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test("documentation is generated from canonical sources with an explicit preview allowlist", async () => {
  const navigation = JSON.parse(await read("scripts/site/navigation.json"));
  const slugs = navigation.map(page => page.slug);
  assert.equal(new Set(slugs).size, slugs.length);
  for (const required of ["index", "getting-started", "python", "rust", "guide", "export", "troubleshooting", "examples", "api", "architecture"]) assert.ok(slugs.includes(required), required);
  assert.equal(navigation.find(page => page.slug === "architecture").source, "docs/architecture.md");
  assert.equal(navigation.find(page => page.slug === "reference").source, "REFERENCE.md");
  assert.match(await read("web/site/index.html"), /data-theme-choice/);
  assert.match(await read("web/site/index.html"), /theme-init\.js/);
  assert.match(await read("web/site/theme.js"), /noon-site-theme/);
  assert.match(await read("web/site/theme.js"), /removeItem/);
  const examples = await read("docs/site/examples.md");
  const selected = JSON.parse(await read("web/site/examples.json"));
  for (const { id } of selected) assert.ok(examples.includes("noon:demo " + id));
  assert.doesNotMatch(examples, /showcase-triangle-light|<iframe/);
  const builder = await read("scripts/build-project-site.mjs");
  assert.match(builder, /build-project-docs\.sh/);
  const docsBuilder = await read("scripts/site/build_docs.py");
  assert.match(docsBuilder, /check_links/);
  assert.match(docsBuilder, /build_embed/);
});

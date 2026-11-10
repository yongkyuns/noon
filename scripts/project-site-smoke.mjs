import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { qualifyDocumentation } from "./project-site-docs-smoke.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "browser-smoke-artifacts/project-site");
await mkdir(output, { recursive: true });
const server = await serveRepository(root, 0);
let browser;
try {
  browser = await chromium.launch({ headless: true });
  // The shared server serves files, not directory indexes. This still exercises
  // the same relative paths as GitHub's /noon/ project prefix.
  const base = `${server.baseUrl}/_site/index.html`;
  for (const width of [1440, 768, 390, 320]) {
    const page = await browser.newPage({ viewport: { width, height: 1000 } });
    const errors = [], requests = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("request", request => requests.push(request.url()));
    await page.goto(base);
    await page.locator("video").first().waitFor();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${width}: horizontal overflow`);
    assert.equal(await page.locator("h1").textContent(), "Animated visuals,written in code.");
    assert.equal(await page.locator("video").evaluateAll(videos => videos.every(video => video.paused && !video.autoplay)), true);
    assert.ok(!requests.some(url => /\.wasm(?:\?|$)|pyodide|python-worker/.test(url)), "homepage must not load the engine");
    for (const name of ["Native Python", "Native Rust"]) {
      await page.locator("summary").filter({ hasText: name }).click();
    }
    assert.equal(await page.locator("details[open]").count(), 2);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${width}: expanded code overflows`);
    await page.screenshot({ path: path.join(output, `homepage-${width}.png`), fullPage: true });
    assert.deepEqual(errors, []);
    await page.close();
  }
  const page = await browser.newPage();
  await page.goto(base);
  // Every static asset must resolve under the project prefix, including the
  // generated media. Anchor destinations are checked in the structural suite.
  for (const asset of await page.locator("[src], [poster], link[rel=stylesheet]").evaluateAll(elements =>
    elements.flatMap(element => [element.getAttribute("src"), element.getAttribute("poster"), element.getAttribute("href")]).filter(Boolean))) {
    if (!asset.startsWith("./")) continue;
    assert.equal((await page.request.get(new URL(asset, base).href)).ok(), true, asset);
  }
  const selections = JSON.parse(await readFile(path.join(root, "web/site/examples.json"), "utf8"));
  for (const { id } of selections) {
    const decoded = await page.evaluate(async id => {
      const image = new Image(); image.src = `./site/media/${id}.gif`; await image.decode();
      return { width: image.naturalWidth, height: image.naturalHeight };
    }, id);
    assert.deepEqual(decoded, { width: 480, height: 270 }, id);
  }
  const video = page.locator("video").first();
  await video.evaluate(async element => { element.muted = true; await element.play(); });
  await page.waitForFunction(() => document.querySelector("video").currentTime > 0);
  await video.evaluate(element => element.pause());
  // Stub only the target document to inspect the navigation without starting
  // another full playground runtime. Capture qualification tests real scenes.
  await page.route("**/playground.html?**", route => route.fulfill({ contentType: "text/html", body: "<title>Playground destination</title>" }));
  await page.goto(`${base}?catalog=showcase&example=showcase-first-scene&backend=webgl#editor`);
  await page.waitForURL("**/playground.html?catalog=showcase&example=showcase-first-scene&backend=webgl#editor");
  await page.close();
  const noJs = await browser.newContext({ javaScriptEnabled: false });
  const plain = await noJs.newPage();
  await plain.goto(base);
  assert.equal(await plain.locator("h1").isVisible(), true);
  assert.ok(await plain.locator('a[href*="playground.html"]').count() > 0);
  await noJs.close();
  await qualifyDocumentation(browser, base, output);
  console.log("Project site: responsive layout, no-JS content, media, and legacy routing passed");
} finally {
  await browser?.close();
  await server.close();
}

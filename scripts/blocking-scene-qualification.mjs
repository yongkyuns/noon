import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

// Qualify a separately packaged opt-in JSPI module. The ordinary web/pkg stays
// usable in browsers without JSPI and is never replaced by this fixture.
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageDirectory = path.resolve(process.env.NOON_BLOCKING_SCENE_PACKAGE ??
  path.join(repoRoot, "browser-smoke-artifacts/blocking-scene/pkg"));
const defaultPackageDirectory = path.resolve(process.env.NOON_BLOCKING_SCENE_DEFAULT_PACKAGE ??
  path.join(repoRoot, "web/pkg"));
const artifacts = path.resolve(process.env.NOON_BLOCKING_SCENE_ARTIFACTS ?? path.dirname(packageDirectory));
const backend = process.env.NOON_BROWSER_SMOKE_BACKEND ?? "webgpu";
assert.ok(["webgpu", "webgl"].includes(backend));
await mkdir(artifacts, { recursive: true });
const source = await readFile(path.join(repoRoot, "web/blocking-scene-probe.js"), "utf8");
const hashes = {};
const defaultHashes = {};
for (const file of ["noon_web.js", "noon_web_bg.wasm"]) {
  hashes[file] = createHash("sha256").update(await readFile(path.join(packageDirectory, file))).digest("hex");
  defaultHashes[file] = createHash("sha256").update(await readFile(path.join(defaultPackageDirectory, file))).digest("hex");
}
assert.doesNotMatch(await readFile(path.join(defaultPackageDirectory, "noon_web.js"), "utf8"), /WebAssembly\.(promising|Suspending)/);
const server = await serveRepository(path.dirname(packageDirectory), 0);
const defaultServer = await serveRepository(path.dirname(defaultPackageDirectory), 0);
const errors = [];
const report = { backend, browser: null, packageDirectory, hashes, defaultPackageDirectory, defaultHashes, defaultModule: null, result: null, errors };
let browser;
try {
  browser = await chromium.launch({
    ...(process.env.NOON_CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.NOON_CHROMIUM_EXECUTABLE }
      : { channel: "chromium" }),
    headless: true,
    args: browserArgs(backend),
  });
  report.browser = browser.version();
  const defaultPage = await browser.newPage();
  defaultPage.on("pageerror", error => errors.push("default pageerror: " + error));
  defaultPage.on("console", message => { if (message.type() === "error") errors.push("default console: " + message.text()); });
  await defaultPage.route("**/blocking-scene-default.html", route => route.fulfill({
    contentType: "text/html", body: '<!doctype html><html><head><link rel="icon" href="data:,"></head><body></body></html>',
  }));
  await defaultPage.goto(`${defaultServer.baseUrl}/blocking-scene-default.html`, { waitUntil: "load" });
  report.defaultModule = await defaultPage.evaluate(async directory => {
    Object.defineProperty(WebAssembly, "promising", { value: undefined, configurable: true });
    Object.defineProperty(WebAssembly, "Suspending", { value: undefined, configurable: true });
    const module = await import(`./${directory}/noon_web.js`);
    await module.default();
    return {
      initialized: true,
      promising: typeof WebAssembly.promising,
      suspending: typeof WebAssembly.Suspending,
      blockingRunner: typeof module.BlockingSceneRunner,
    };
  }, path.basename(defaultPackageDirectory));
  assert.deepEqual(report.defaultModule, {
    initialized: true, promising: "undefined", suspending: "undefined", blockingRunner: "undefined",
  });
  await defaultPage.close();
  const page = await browser.newPage();
  page.on("pageerror", error => errors.push("pageerror: " + error));
  page.on("console", message => { if (message.type() === "error") errors.push("console: " + message.text()); });
  await page.route("**/blocking-scene-qualification.html*", route => route.fulfill({
    contentType: "text/html",
    body: '<!doctype html><html><head><link rel="icon" href="data:,"></head><body><script type="module" src="./blocking-scene-probe.js"></script></body></html>',
  }));
  await page.route("**/blocking-scene-probe.js", route => route.fulfill({
    contentType: "text/javascript", body: source,
  }));
  const query = new URLSearchParams({ backend, package: path.basename(packageDirectory) });
  await page.goto(`${server.baseUrl}/blocking-scene-qualification.html?${query}`, { waitUntil: "load" });
  await page.waitForFunction(() => globalThis.noonBlockingSceneProbe?.ready === true, null, { timeout: 90_000 });
  report.result = await page.evaluate(() => globalThis.noonBlockingSceneProbe);
  assert.equal(report.result.error, null);
  assert.equal(report.result.cases.length, 5);
  assert.deepEqual(errors, []);
  // A package built elsewhere must remain the same bytes throughout qualification.
  for (const file of Object.keys(hashes)) {
    assert.equal(createHash("sha256").update(await readFile(path.join(packageDirectory, file))).digest("hex"), hashes[file]);
    assert.equal(createHash("sha256").update(await readFile(path.join(defaultPackageDirectory, file))).digest("hex"), defaultHashes[file]);
  }
  console.log(JSON.stringify({
    backend, browser: report.browser, hashes,
    defaultModule: report.defaultModule,
    cases: report.result.cases.map(({ name, time, frames }) => ({ name, time, frames })),
    reentrantSamples: report.result.reentrantSamples,
    errors,
    report: path.join(artifacts, `blocking-scene-${backend}-report.json`),
  }, null, 2));
} catch (error) {
  report.failure = String(error.stack ?? error);
  throw error;
} finally {
  await writeFile(path.join(artifacts, `blocking-scene-${backend}-report.json`), JSON.stringify(report, null, 2) + "\n");
  await browser?.close();
  await server.close();
  await defaultServer.close();
}

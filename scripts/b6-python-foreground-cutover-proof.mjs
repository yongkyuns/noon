import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { playgroundLaunchOptions } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = 4211;
const base = `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(root, "browser-smoke-artifacts/b6-python-foreground");
const source = await readFile(path.join(root, "web/python/examples/foreground_membership.py"), "utf8");
await mkdir(artifacts, { recursive: true });

let server;
let browser;
let context;
let runtimeCache;
const result = { errors: [] };

try {
  server = spawn(
    "python3",
    ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root],
    { stdio: "ignore" },
  );
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    ready = await fetch(base).then(response => response.ok).catch(() => false);
    if (ready) break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.ok(ready, "foreground smoke HTTP server did not start");

  const workerResponse = await fetch(new URL("python-worker.js", base), {
    signal: AbortSignal.timeout(20000),
  });
  assert.ok(workerResponse.ok, "Python worker source is unavailable");
  runtimeCache = createPyodideResourceCache(await workerResponse.text());

  browser = await playwright.chromium.launch(playgroundLaunchOptions("chromium"));
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    deviceScaleFactor: 1,
  });
  await runtimeCache.install(context);
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on("pageerror", error => result.errors.push(String(error)));
  page.on("console", message => {
    if (message.type() === "error") result.errors.push(message.text());
  });

  await page.goto(`${base}?example=compatible-indicate-square`, {
    waitUntil: "domcontentloaded",
    timeout: 30000,
  });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, {
    timeout: 45000,
  });
  await page.waitForFunction(
    () =>
      !window.__noonExampleGallery.runInFlight &&
      document.querySelector("#patch-status")?.dataset.state === "applied",
    null,
    { timeout: 90000 },
  );

  await page.evaluate(pythonSource => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) {
      throw new Error("Python scene editor is unavailable");
    }
    editor.value = pythonSource;
    window.__foregroundProofDone = false;
    window.__foregroundProofError = null;
    Promise.resolve(window.__noonExampleGallery.run())
      .catch(error => {
        window.__foregroundProofError = String(error);
      })
      .finally(() => {
        window.__foregroundProofDone = true;
      });
  }, source);

  await page.waitForFunction(() => window.__foregroundProofDone === true, null, {
    timeout: 90000,
  });
  const state = await page.evaluate(() => ({
    runError: window.__foregroundProofError,
    inFlight: window.__noonExampleGallery.runInFlight,
    patchState: document.querySelector("#patch-status")?.dataset.state,
    patchText: document.querySelector("#patch-status")?.value,
  }));
  result.state = state;
  assert.equal(state.runError, null, state.runError ?? undefined);
  assert.equal(state.inFlight, false, "foreground Python run did not settle");
  assert.equal(state.patchState, "applied", state.patchText);

  const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  result.metrics = metrics;
  assert.ok(Number(metrics?.metrics?.presentedFrames) > 0, "foreground scene rendered no frames");
  assert.deepEqual(result.errors, []);
  result.outcome = "pass";
} catch (error) {
  result.outcome = "fail";
  result.failure = String(error);
  throw error;
} finally {
  if (context) {
    const pages = context.pages();
    if (pages.length > 0) {
      await pages[0]
        .screenshot({ path: path.join(artifacts, "foreground.png"), timeout: 5000 })
        .catch(() => {});
    }
  }
  result.runtimeResources = runtimeCache?.stats();
  await writeFile(
    path.join(artifacts, "foreground.json"),
    JSON.stringify(result, (_, value) => typeof value === "bigint" ? value.toString() : value, 2),
  );
  await context?.close();
  await browser?.close();
  server?.kill("SIGTERM");
}

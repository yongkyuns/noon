import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.resolve(root, process.env.NOON_WAIT_ARTIFACTS ?? "browser-smoke-artifacts/playground-wait-clock");
const backend = process.env.NOON_WAIT_BACKEND ?? "webgl";
assert.ok(["webgl", "webgpu"].includes(backend));
const source = `from noon import *
class WaitClock(Scene):
    def construct(self):
        dot = Circle(radius=0.4)
        self.add(dot)
        self.wait(3)
        self.play(dot.animate.shift(RIGHT), run_time=0.4)
        self.wait(3)
        self.play(dot.animate.shift(LEFT), run_time=0.4)
`;
const report = { backend, errors: [], samples: {} };
await mkdir(out, { recursive: true });
const server = await serveRepository(root, Number(process.env.NOON_WAIT_PORT ?? 4197));
let browser;
function checkClock(samples, label, tolerance) {
  assert.ok(samples.length >= 8, `${label}: missing clock samples`);
  const start = samples[0];
  for (const sample of samples) {
    assert.ok(Number.isFinite(sample.time), `${label}: invalid elapsed time`);
    assert.ok(Math.abs((sample.time - start.time) - (sample.wall - start.wall)) < tolerance,
      `${label}: elapsed time does not track wall time: ${JSON.stringify(sample)}`);
  }
  assert.ok(samples.at(-1).time - start.time > 0.8, `${label}: wait clock did not advance`);
}
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  await createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8")).install(context);
  const page = await context.newPage();
  page.setDefaultTimeout(60_000);
  page.on("pageerror", error => report.errors.push(error.stack ?? String(error)));
  page.on("console", message => { if (message.type() === "error") report.errors.push(message.text()); });
  // The public editor lifecycle must submit the source; there is no parallel test
  // animation driver, fake UI clock or page-side interpolation in this regression.
  await page.goto(`${server.baseUrl}/web/index.html`);
  await page.waitForFunction(() => document.querySelector("#patch-status")?.dataset.state === "applied" &&
    window.__noonExampleGallery?.runInFlight === false);
  await page.evaluate(source => {
    const editor = document.querySelector("#python-scene-source");
    editor.value = source;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }, source);
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    const time = Number(controls?.dataset.elapsedSeconds);
    return controls?.dataset.controllable === "false" && time > 0.1 && time < 1.5;
  });
  async function sample(label) {
    const samples = await page.evaluate(async () => {
      const samples = [];
      const start = performance.now();
      while (performance.now() - start < 1_100) {
        const controls = document.querySelector(".playback-controls");
        samples.push({ wall: (performance.now() - start) / 1_000,
          time: Number(controls?.dataset.elapsedSeconds), controllable: controls?.dataset.controllable });
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      return samples;
    });
    report.samples[label] = samples;
    checkClock(samples, label, 0.28); // Includes the existing 100 ms UI polling cadence.
    return samples;
  }
  const live = await sample("initial-wait");
  assert.ok(live.every(s => s.controllable === "false" && s.time < 3), "must sample inside the first source-owned wait");
  await page.screenshot({ path: path.join(out, "initial-wait.png") });
  assert.equal(await page.locator("#status").getAttribute("data-renderer-backend"),
    backend === "webgl" ? "WebGL2" : "WebGPU");
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    const time = Number(controls?.dataset.elapsedSeconds);
    return controls?.dataset.controllable === "false" && time > 3.55 && time < 4.9;
  });
  const later = await sample("wait-after-animation");
  assert.ok(later.every(s => s.controllable === "false" && s.time > 3.4 && s.time < 6.4),
    "must also tick during the wait following an active animation");
  await page.waitForFunction(() => document.querySelector("#patch-status")?.dataset.state === "applied" &&
    window.__noonExampleGallery?.runInFlight === false);
  const duration = await page.locator(".playback-scrubber").getAttribute("max");
  assert.ok(Math.abs(Number(duration) - 6.8) < 1e-9, "both waits must contribute to replay duration");
  await page.locator(".playback-toggle").click(); // Pause the completed replay.
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "false");
  await page.locator(".playback-scrubber").evaluate(range => {
    range.value = "0"; range.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.waitForFunction(() => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) < 0.05 &&
    document.querySelector(".playback-controls")?.dataset.busy === "false");
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) > 0.1);
  const replay = await sample("replay-wait");
  assert.ok(replay.every(s => s.controllable === "true" && s.time < 3), "must sample inside the completed replay wait");
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "false");
  const held = await page.evaluate(async () => {
    const get = () => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds);
    const before = get(); await new Promise(resolve => setTimeout(resolve, 350)); return { before, after: get() };
  });
  report.paused = held;
  assert.ok(held.before >= replay.at(-1).time, "pause must not rewind to the last rendered frame");
  assert.equal(held.before, held.after, "paused time must be frozen");
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(held => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) > held + 0.2, held.after);
  report.resumed = Number(await page.locator(".playback-controls").getAttribute("data-elapsed-seconds"));
  assert.ok(report.resumed > held.after, "resume must continue from the paused wait position");
  assert.deepEqual(report.errors, []);
  console.log(`wait clock tracks wall time in source and replay, preserves pause/resume (${backend})`);
} catch (error) {
  report.failure = error.stack ?? String(error);
  throw error;
} finally {
  await writeFile(path.join(out, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server.close();
}

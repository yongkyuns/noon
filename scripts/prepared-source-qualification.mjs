// Actual autonomous source tasks interleave with the existing canvas/input driver.
// All source control flow, scene state and history delivery remain inside Rust.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_SOURCE_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-canvas/source");
await mkdir(output, { recursive: true });
const report = { status: "running", cases: [] };
let server, browser;
function equalPixels(a, b) { return a.width === b.width && a.height === b.height && a.data.equals(b.data); }
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    for (const scenario of ["complete", "failure", "cancel"]) {
      const result = { backend, scenario, status: "running", checks: [] }; report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
      page.setDefaultTimeout(60_000);
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      const state = () => page.evaluate(() => ({ playback: JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()), calls: autoCanvas.source.calls(), dropped: autoCanvas.source.producerDropped(), pump: autoCanvas.pump?.stats(), errors: autoCanvas.errors }));
      const capture = async name => {
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${backend}-${scenario}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async scenario => {
          globalThis.Worker = class { constructor() { throw new Error("source preparation introduced worker transport"); } };
          const { default: init, CooperativeCanvasFixture } = await import("./pkg/noon_web.js"); await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const { createCooperativePreparationDriver } = await import("./cooperative-preparation-driver.js");
          const source = new CooperativeCanvasFixture(128, scenario === "failure" ? 5 : 0xffffffff);
          const canvas = document.querySelector("#scene");
          const renderer = await source.createCanvas(canvas.transferControlToOffscreen());
          renderer.setPreparedPlaying(false);
          const errors = [];
          const direct = createDirectExecutionWakeDriver(renderer);
          const detach = attachNativeInputs(renderer, canvas, { inspectionZoom: true, onInput: () => direct.wake(), onError: error => errors.push(String(error)) });
          // Registering a logical view can require a fresh receipt.
          direct.wake();
          globalThis.autoCanvas = { source, renderer, errors, direct, detach, pump: null,
            start() { this.pump = createCooperativePreparationDriver(() => source.prepareNext(renderer), { onProgress: () => direct.wake(), onError: error => errors.push(String(error)) }); } };
        }, scenario);
        assert.equal(await page.evaluate(() => autoCanvas.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        const baseline = await capture("baseline");
        assert.equal((await state()).calls, 1);
        await page.evaluate(() => autoCanvas.start());
        if (scenario === "complete") {
          await page.waitForFunction(() => autoCanvas.source.calls() >= 3 && autoCanvas.pump.stats().active);
          const rect = await page.locator("#scene").boundingBox();
          await page.mouse.move(rect.x + 320, rect.y + 180); await page.mouse.down();
          const pressed = await state();
          assert.ok(pressed.pump.active && pressed.calls < 130);
          result.callsAtPress = pressed.calls;
          const receipt = await page.evaluate(() => autoCanvas.renderer.debugPointerPresentationJson());
          await page.waitForFunction(calls => autoCanvas.source.calls() > calls + 2, pressed.calls);
          assert.equal(await page.evaluate(() => autoCanvas.renderer.debugPointerPresentationJson()), receipt);
          await page.mouse.up();
          await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions);
          await page.waitForTimeout(100);
          assert.equal((await state()).playback.time, 0.5);
          assert.ok(!equalPixels(baseline, await capture("highlight")));
          result.checks.push("automatic-typed-preparation", "press-yields-to-source-tasks", "receipt-preserved-during-delivery", "paused-visible-highlight");
          await page.waitForFunction(() => !autoCanvas.pump.stats().active && !JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions && autoCanvas.direct.stats().idle);
          const end = await state();
          assert.equal(end.calls, 130); assert.equal(end.playback.end, 33); assert.equal(end.dropped, true);
          assert.equal(end.playback.time, 0.5); assert.equal(end.pump.failed, false); assert.deepEqual(end.errors, []);
          assert.ok(equalPixels(baseline, await capture("restored")));
          const frames = await page.evaluate(() => autoCanvas.direct.stats().presentedFrames);
          await page.waitForTimeout(120);
          assert.equal(await page.evaluate(() => autoCanvas.direct.stats().presentedFrames), frames);
          await page.evaluate(() => { autoCanvas.renderer.seekDirect(33); autoCanvas.direct.wake(); });
          assert.equal((await state()).playback.time, 33);
          await page.evaluate(() => { autoCanvas.renderer.seekDirect(0.5); autoCanvas.direct.wake(); });
          assert.ok(equalPixels(baseline, await capture("rewind")));
          result.checks.push("source-completed-once", "producer-released", "terminal-after-adoption", "exact-restoration-and-rewind", "settled-without-polling");
        } else {
          if (scenario === "failure") {
            await page.waitForFunction(() => autoCanvas.pump.stats().failed);
            assert.equal((await state()).errors.length, 1); assert.equal((await state()).calls, 6);
          } else {
            await page.waitForFunction(() => autoCanvas.source.calls() >= 3 && autoCanvas.pump.stats().active);
            await page.evaluate(() => { autoCanvas.pump.stop(); autoCanvas.source.cancel(); });
          }
          const stopped = await state();
          assert.equal(stopped.dropped, true); assert.equal(stopped.playback.end, null); assert.equal(stopped.playback.time, 0.5);
          await page.waitForTimeout(120); assert.equal((await state()).calls, stopped.calls);
          const rect = await page.locator("#scene").boundingBox(); await page.mouse.click(rect.x + 320, rect.y + 180);
          await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions);
          await page.waitForFunction(() => !JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions && autoCanvas.direct.stats().idle);
          assert.ok(equalPixels(baseline, await capture("still-interactive")));
          result.checks.push("no-source-retry", "no-false-completion", "producer-released", "retained-history-still-interactive");
        }
        assert.deepEqual(errors, []);
        result.status = "passed";
        console.log(`[PASS] cooperative ${backend}/${scenario}: ${result.checks.join(", ")}`);
      } catch (error) { result.status = "failed"; result.error = String(error); result.pageErrors = errors; throw error; }
      finally { await page.close(); }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error); throw error; }
finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  if (browser) await browser.close(); if (server) await server.close();
}

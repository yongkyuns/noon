// Actual direct-WASM canvas ownership, typed incremental delivery and DOM input.
// Screenshots are observations only; no scene/runtime data is round-tripped.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_CANVAS_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-canvas");
await mkdir(output, { recursive: true });
const report = { status: "running", cases: [] };
let server, browser;
function changedPixels(a, b) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (!a.data.subarray(i, i + 4).equals(b.data.subarray(i, i + 4))) changed++;
  }
  return changed;
}
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    const result = { backend, status: "running", checks: [] }; report.cases.push(result);
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
    page.setDefaultTimeout(60_000);
    const errors = []; page.on("pageerror", error => errors.push(String(error)));
    const status = () => page.evaluate(() => JSON.parse(prefixCanvas.renderer.debugPreparedPlaybackJson()));
    const receipt = () => page.evaluate(() => JSON.parse(prefixCanvas.renderer.debugPointerPresentationJson()));
    const capture = async label => {
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const bytes = await page.locator("#scene").screenshot();
      await writeFile(path.join(output, `${backend}-${label}.png`), bytes);
      return PNG.sync.read(bytes);
    };
    const click = async (x = 320, y = 180) => {
      const rect = await page.locator("#scene").boundingBox();
      await page.mouse.click(rect.x + x, rect.y + y);
    };
    const step = time => page.evaluate(time => {
      prefixCanvas.renderer.advanceDirectRealtime(time); prefixCanvas.renderer.render();
    }, time);
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      const initial = await page.evaluate(async () => {
        globalThis.Worker = class { constructor() { throw new Error("prepared direct canvas attempted worker transport"); } };
        const { default: init, PreparedCanvasFixture } = await import("./pkg/noon_web.js");
        await init();
        const { attachNativeInputs } = await import("./native-inputs.js");
        const canvas = document.querySelector("#scene");
        const producer = new PreparedCanvasFixture();
        const renderer = await producer.createCanvas(canvas.transferControlToOffscreen());
        renderer.setPreparedPlaying(false);
        const errors = [], trace = [];
        globalThis.prefixCanvas = { canvas, producer, renderer, errors, trace, driver: null };
        const original = renderer.nativePointerInput.bind(renderer);
        renderer.nativePointerInput = (...args) => {
          const outcome = original(...args); trace.push({ kind: args[0], admitted: outcome !== undefined }); return outcome;
        };
        prefixCanvas.detach = attachNativeInputs(renderer, canvas, {
          inspectionZoom: true,
          onInput: () => prefixCanvas.driver?.wake(), onError: error => errors.push(String(error)),
        });
        return { receipt: JSON.parse(renderer.debugPointerPresentationJson()), state: JSON.parse(renderer.debugPreparedPlaybackJson()) };
      });
      assert.equal(initial.receipt.presented, null);
      assert.equal(initial.state.time, 0.5); assert.equal(initial.state.frontier, 1); assert.equal(initial.state.end, null);
      await click(); // No presented frame can authorize a click.
      assert.equal((await status()).interactions, false);
      assert.equal(await page.evaluate(() => prefixCanvas.trace.at(-1).admitted), false);
      await page.evaluate(() => prefixCanvas.renderer.render());
      assert.equal(await page.evaluate(() => prefixCanvas.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
      assert.equal((await receipt()).presented, (await receipt()).current);
      const baseline = await capture("baseline");
      const blue = Array.from({ length: baseline.data.length / 4 }, (_, i) => i * 4)
        .filter(i => baseline.data[i + 2] > 200 && baseline.data[i] < 40).length;
      assert.ok(blue > 500, "a real nonempty blue circle must be presented");
      result.checks.push("real-prepared-canvas", "unpresented-click-rejected", "exact-presented-receipt");

      await click(); await step(1000); await step(1250);
      const midpoint = await capture("paused-highlight");
      result.midpointChangedPixels = changedPixels(baseline, midpoint);
      assert.ok(result.midpointChangedPixels > 500);
      assert.equal((await status()).time, 0.5);
      await step(1500);
      assert.equal(changedPixels(baseline, await capture("restored")), 0);
      result.checks.push("paused-click-highlight", "exact-restoration");

      // Future source mutation/delivery happens between genuine DOM press/release.
      const rect = await page.locator("#scene").boundingBox();
      await page.mouse.move(rect.x + 320, rect.y + 180); await page.mouse.down();
      const beforeAppend = await receipt();
      await page.evaluate(() => prefixCanvas.producer.publishNext(prefixCanvas.renderer, false));
      assert.equal((await status()).frontier, 2);
      assert.deepEqual(await receipt(), beforeAppend);
      await page.mouse.up();
      assert.equal((await status()).interactions, true);
      await step(2000); await step(2250);
      assert.ok(changedPixels(baseline, await capture("past-click-after-future")) > 500);
      await step(2500);
      assert.equal(changedPixels(baseline, await capture("past-restored")), 0);
      result.checks.push("press-survives-future-delivery", "historical-binding-not-producer-state");

      await page.evaluate(() => { prefixCanvas.renderer.seekDirect(1.5); prefixCanvas.renderer.render(); });
      const later = await capture("later-source-revision");
      assert.ok(changedPixels(baseline, later) > 500);
      const beforeRejected = { status: await status(), receipt: await receipt() };
      const rejected = await page.evaluate(() => {
        try { prefixCanvas.renderer.seekDirect(2); return false; } catch { return true; }
      });
      assert.ok(rejected, "unsealed frontier is not a seekable frame");
      assert.deepEqual({ status: await status(), receipt: await receipt() }, beforeRejected);
      await page.evaluate(() => { prefixCanvas.renderer.seekDirect(0.5); prefixCanvas.renderer.render(); prefixCanvas.renderer.setPreparedPlaying(true); prefixCanvas.renderer.directWakeDirectiveJson(3000); });
      await step(3500); // Authored revision at 1.0.
      assert.equal((await status()).time, 1);
      await step(4500); // The exact open frontier cannot be evaluated.
      const buffered = await status();
      assert.equal(buffered.buffering, true); assert.equal(buffered.time, 1);
      await step(50000);
      assert.deepEqual(await status(), buffered);
      assert.equal(await page.evaluate(() => JSON.parse(prefixCanvas.renderer.directWakeDirectiveJson(50000)).cadence), "idle");
      result.checks.push("invalid-seek-atomic", "open-frontier-buffers-without-spin");

      const bufferedReceipt = await receipt();
      await page.evaluate(() => prefixCanvas.producer.publishNext(prefixCanvas.renderer, true));
      assert.equal(await page.evaluate(() => prefixCanvas.producer.producerAlive()), false);
      assert.deepEqual(await receipt(), bufferedReceipt);
      assert.equal((await status()).time, 1); assert.equal((await status()).end, 3);
      await page.evaluate(() => prefixCanvas.renderer.directWakeDirectiveJson(50000));
      await step(50000); assert.equal((await status()).time, 1, "buffered wall time must not become catch-up");
      await step(51000); assert.equal((await status()).time, 2);
      await step(52000); assert.equal((await status()).time, 3);
      assert.equal(await page.evaluate(() => JSON.parse(prefixCanvas.renderer.directWakeDirectiveJson(52000)).cadence), "idle");
      const endpoint = await capture("terminal");
      assert.ok(changedPixels(later, endpoint) > 500);
      await page.evaluate(() => { prefixCanvas.renderer.seekDirect(0.5); prefixCanvas.renderer.render(); prefixCanvas.renderer.setPreparedPlaying(false); });
      assert.equal(changedPixels(baseline, await capture("rewind-after-producer-drop")), 0);
      result.checks.push("typed-terminal-delivery", "resume-no-catch-up", "exact-endpoint", "rewind-after-producer-drop");

      // Ordinary driver/DOM wiring, not a fixture-owned interpolation loop.
      await page.evaluate(async () => {
        const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
        prefixCanvas.driver = createDirectExecutionWakeDriver(prefixCanvas.renderer);
      });
      await click();
      await page.waitForFunction(() => prefixCanvas.driver.stats().idle && !JSON.parse(prefixCanvas.renderer.debugPreparedPlaybackJson()).interactions);
      const stats = await page.evaluate(() => prefixCanvas.driver.stats());
      assert.ok(stats.presentedFrames >= 2, "the existing driver must render the effect autonomously");
      assert.equal(changedPixels(baseline, await capture("autonomous-restored")), 0);
      await page.waitForTimeout(120);
      assert.equal((await page.evaluate(() => prefixCanvas.driver.stats())).presentedFrames, stats.presentedFrames);
      await page.evaluate(() => prefixCanvas.driver.stop());
      result.checks.push("existing-autonomous-driver", "settled-idle-no-frame-polling");

      await page.evaluate(() => prefixCanvas.renderer.setPointerFillSelection(4));
      await click(); await step(60000); await step(60500);
      assert.ok(changedPixels(baseline, await capture("selection-overlay")) > 500);
      await click(20, 320); await page.evaluate(() => prefixCanvas.renderer.render());
      assert.equal(changedPixels(baseline, await capture("selection-cleared")), 0);
      result.checks.push("shared-selection-overlay-and-clear");

      // Viewer-only zoom uses the same presented-frame receipt and leaves time fixed.
      await page.mouse.move(rect.x + 320, rect.y + 180);
      await page.mouse.wheel(0, -120);
      await page.waitForFunction(() => JSON.parse(prefixCanvas.renderer.debugPointerPresentationJson()).camera[2] !== 8);
      const zoom = await receipt();
      assert.notEqual(zoom.camera[2], 8);
      assert.equal((await status()).time, 0.5);
      await page.evaluate(() => prefixCanvas.renderer.render());
      assert.equal((await receipt()).presented, (await receipt()).current);
      result.checks.push("inspection-zoom-with-paused-history");
      assert.deepEqual(errors, []);
      assert.deepEqual(await page.evaluate(() => prefixCanvas.errors), []);
      result.status = "passed";
      console.log(`[PASS] prepared ${backend}: ${result.checks.join(", ")}`);
    } catch (error) {
      result.status = "failed"; result.error = String(error); result.pageErrors = errors;
      throw error;
    } finally {
      await page.close(); await browser.close(); browser = null;
    }
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error); throw error;
} finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  if (browser) await browser.close();
  if (server) await server.close();
}

// Real DOM wheel delivery, shared execution, and actual WebGPU/WebGL pixels.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { VIEW, SHAPES, assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_INSPECTION_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/direct-inspection");
await mkdir(output, { recursive: true });
const report = { status: "running", cases: [] };
let server, browser;
function blue(image) {
  let n = 0, x = 0, y = 0;
  for (let row = 0; row < image.height; ++row) for (let col = 0; col < image.width; ++col) {
    const i = 4 * (row * image.width + col), [r, g, b] = image.data.subarray(i, i + 3);
    if (b > 100 && b > r * 1.8 && g > r * 1.7) { n++; x += col + 0.5; y += row + 0.5; }
  }
  assert.ok(n > 100, "blue fixture must occupy visible pixels");
  return { n, x: x / n, y: y / n };
}
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, channel: "chromium", args: browserArgs(backend) });
    for (const mode of ["session", "finished-program", "indicate"]) {
      const entry = { backend, mode, status: "running", checks: [] }; report.cases.push(entry);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
      page.setDefaultTimeout(60_000);
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        const before = await page.evaluate(async mode => {
          globalThis.Worker = class { constructor() { throw new Error("direct inspection attempted a worker"); } };
          const wasm = await import("./pkg/noon_web.js"); await wasm.default();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const canvas = document.querySelector("#scene"), errors = [], trace = [];
          const renderer = mode === "indicate"
            ? await wasm.createDirectInspectionIndicateRenderer(canvas.transferControlToOffscreen())
            : await wasm.createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), mode === "finished-program");
          if (mode !== "indicate") renderer.setPointerFillSelection(4);
          const original = renderer.nativeInspectionScroll.bind(renderer);
          renderer.nativeInspectionScroll = (...args) => {
            const before = renderer.debugPointerPresentationJson(), time = renderer.time();
            const result = original(...args);
            trace.push({ args, result: result ?? null, before, after: renderer.debugPointerPresentationJson(), time, afterTime: renderer.time() });
            return result;
          };
          const wheel = (delta = -500 * Math.log(2), x = 340, y = 170) => {
            const rect = canvas.getBoundingClientRect();
            const event = new WheelEvent("wheel", { cancelable: true, deltaMode: 0, deltaY: delta,
              clientX: rect.left + x, clientY: rect.top + y });
            canvas.dispatchEvent(event); return event.defaultPrevented;
          };
          let driver;
          const detach = attachNativeInputs(renderer, canvas, { inspectionZoom: true,
            onInput: () => driver?.wake(), onError: error => errors.push(String(error)) });
          const unpresentedConsumed = wheel();
          driver = createDirectExecutionWakeDriver(renderer);
          globalThis.inspectionTest = { canvas, renderer, driver, detach, errors, trace, wheel };
          return { unpresentedConsumed };
        }, mode);
        assert.equal(before.unpresentedConsumed, false);
        const settled = async () => {
          await page.waitForFunction(() => inspectionTest.driver.stats().idle);
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          assert.deepEqual(errors, []); assert.deepEqual(await page.evaluate(() => inspectionTest.errors), []);
        };
        const shot = async label => {
          const bytes = await page.locator("#scene").screenshot();
          await writeFile(path.join(output, `${backend}-${mode}-${label}.png`), bytes);
          return PNG.sync.read(bytes);
        };
        if (mode === "indicate") {
          await page.waitForFunction(() => inspectionTest.renderer.time() >= 1 && inspectionTest.renderer.time() < 3);
          const outcome = await page.evaluate(() => {
            const t = inspectionTest, frame = t.renderer.debugSelectionFrameJson();
            const consumed = t.wheel(-500 * Math.log(2), 320, 180);
            return { consumed, frame, afterFrame: t.renderer.debugSelectionFrameJson(), trace: t.trace.at(-1) };
          });
          assert.equal(outcome.consumed, true);
          assert.equal(outcome.frame, outcome.afterFrame, "wheel does not change animation publication or authored time");
          assert.equal(outcome.trace.time, outcome.trace.afterTime);
          await shot("active-zoom"); await settled();
          const status = await page.evaluate(() => JSON.parse(inspectionTest.renderer.debugPointerPresentationJson()));
          assert.ok(Math.abs(status.camera[2] - 4) < 1e-5);
          assert.equal(status.presentedInspectionRevision, status.inspectionRevision);
          assert.equal(await page.evaluate(() => inspectionTest.renderer.time()), 4);
          const image = await shot("restored");
          // Pure-blue circle is back after Indicate; midpoint yellow is not retained.
          const center = 4 * (180 * image.width + 320);
          assert.ok(image.data[center] < 8 && image.data[center + 1] < 8 && image.data[center + 2] > 245);
          entry.checks.push("real-Indicate-wheel-coexistence", "no-time-or-shape-write-from-wheel", "completion-restores-blue-with-zoom-retained");
        } else {
          await settled();
          const baseline = await shot("baseline"), b = blue(baseline);
          const authored = await page.evaluate(() => inspectionTest.renderer.debugSelectionFrameJson());
          assert.equal(await page.evaluate(() => inspectionTest.wheel()), true); await settled();
          const zoomed = await shot("zoomed"), z = blue(zoomed);
          assert.ok(z.n / b.n > 3.7 && z.n / b.n < 4.3, `zoomed area ratio ${z.n / b.n}`);
          assert.ok(Math.abs(z.x - (340 + (b.x - 340) * 2)) < 1.5, "cursor anchored x centroid");
          assert.ok(Math.abs(z.y - (170 + (b.y - 170) * 2)) < 1.5, "cursor anchored y centroid");
          assert.equal(await page.evaluate(() => inspectionTest.renderer.debugSelectionFrameJson()), authored);
          // A fresh precise click must target the now-zoomed shape, not its old location.
          const rect = await page.locator("#scene").boundingBox();
          await page.mouse.click(rect.x + z.x, rect.y + z.y); await settled();
          const selected = await shot("picked-after-zoom");
          // The shared overlay is translucent. Qualify the entire transformed
          // fill and untouched exterior, not an invented opaque-yellow value.
          entry.selectionPixels = assertSelectionPixels(zoomed, selected, SHAPES[0], {
            ...VIEW, cameraHeight: VIEW.cameraHeight / 2,
            centerX: (340 - VIEW.width / 2) * VIEW.cameraHeight / VIEW.height / 2,
            centerY: (VIEW.height / 2 - 170) * VIEW.cameraHeight / VIEW.height / 2,
          });
          await page.mouse.click(rect.x + 620, rect.y + 340); await settled();
          assert.deepEqual((await shot("cleared")).data, zoomed.data);
          assert.equal(await page.evaluate(() => inspectionTest.wheel(500 * Math.log(2))), true); await settled();
          const restored = await shot("zoom-out"), r = blue(restored);
          assert.ok(Math.abs(r.n / b.n - 1) < 0.01 && Math.abs(r.x - b.x) < 0.25 && Math.abs(r.y - b.y) < 0.25);
          const burst = await page.evaluate(() => Array.from({ length: 32 }, () => inspectionTest.wheel(-10)));
          assert.deepEqual(burst, [true, ...Array(31).fill(false)]); await settled();
          const final = await page.evaluate(() => JSON.parse(inspectionTest.renderer.debugPointerPresentationJson()));
          assert.equal(final.inspectionRevision, "3"); assert.equal(final.presentedInspectionRevision, "3");
          assert.equal(await page.evaluate(() => inspectionTest.renderer.time()), 0);
          entry.pixels = { baseline: b, zoomed: z, restored: r };
          entry.checks.push("unpresented-wheel-rejected", "anchored-zoom-in-out-pixels", "precise-picking-after-zoom", "bounded-burst-no-reassociation", "idle-no-authored-time-change");
        }
        assert.deepEqual(errors, []); assert.deepEqual(await page.evaluate(() => inspectionTest.errors), []);
        entry.status = "passed";
      } catch (error) { entry.status = "failed"; entry.error = String(error.stack ?? error); throw error; }
      finally { await page.close(); }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server?.close();
}

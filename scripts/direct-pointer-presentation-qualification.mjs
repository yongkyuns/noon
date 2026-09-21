// Actual same-context canvas presentation and DOM admission. No worker or scene codec.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { SHAPES, shapeSurfaceCenter, assertExactPixels, assertSelectionPixels }
  from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_POINTER_PRESENTATION_ARTIFACTS
  ?? path.join(root, "browser-smoke-artifacts/direct-pointer-presentation");
const report = { status: "running", cases: [] };
await mkdir(output, { recursive: true });
let server, browser;
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    for (const program of [false, true]) {
      const name = `${backend}-${program ? "program" : "session"}`;
      const result = { name, status: "running", checks: [] }; report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
      page.setDefaultTimeout(60_000);
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      const settled = async () => {
        await page.waitForFunction(() => receiptTest.driver.stats().idle);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(errors, []);
        assert.deepEqual(await page.evaluate(() => receiptTest.errors), []);
      };
      const receipt = () => page.evaluate(() => JSON.parse(receiptTest.renderer.debugPointerPresentationJson()));
      const image = async label => {
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${name}-${label}.png`), bytes); return PNG.sync.read(bytes);
      };
      const click = async shape => {
        const rect = await page.locator("#scene").boundingBox(); const point = shapeSurfaceCenter(shape);
        await page.mouse.click(rect.x + point.x, rect.y + point.y); await settled();
      };
      const clear = async () => {
        const rect = await page.locator("#scene").boundingBox();
        await page.mouse.click(rect.x + 20, rect.y + 320); await settled();
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        const first = await page.evaluate(async program => {
          globalThis.Worker = class { constructor() { throw new Error("direct receipt qualification attempted a Worker"); } };
          const { default: init, createDirectPointerSelectionRenderer } = await import("./pkg/noon_web.js");
          await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const canvas = document.querySelector("#scene"), errors = [], trace = [];
          const renderer = await createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), program);
          renderer.setPointerFillSelection(4);
          const original = renderer.nativePointerInput.bind(renderer);
          renderer.nativePointerInput = (...args) => {
            const before = JSON.parse(renderer.debugPointerPresentationJson());
            const admitted = original(...args) !== undefined;
            trace.push({ args, admitted, before }); return admitted ? false : undefined;
          };
          let driver;
          const detach = attachNativeInputs(renderer, canvas, { onInput: () => driver?.wake(), onError: error => errors.push(String(error)) });
          const event = (type, id, x, y, buttons = 0) => {
            const rect = canvas.getBoundingClientRect();
            canvas.dispatchEvent(new PointerEvent(type, { pointerId: id, pointerType: "mouse", isPrimary: true,
              clientX: rect.left + x, clientY: rect.top + y, button: type === "pointermove" ? -1 : 0, buttons }));
          };
          const unpainted = JSON.parse(renderer.debugPointerPresentationJson());
          event("pointerdown", 77, 248, 171, 1); event("pointerup", 77, 248, 171);
          const beforePaint = trace.slice();
          driver = createDirectExecutionWakeDriver(renderer);
          globalThis.receiptTest = { canvas, renderer, driver, detach, trace, errors, event };
          return { unpainted, beforePaint };
        }, program);
        assert.equal(first.unpainted.presented, null);
        assert.equal(first.beforePaint.length, 1, "rejected press must suppress its release");
        assert.equal(first.beforePaint[0].admitted, false);
        await settled();
        assert.equal(await page.evaluate(() => receiptTest.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        let status = await receipt(); assert.equal(status.presented, status.current); assert.equal(status.refreshPending, false);
        result.checks.push("unpresented-input-rejected", "successful-presentation-installs-exact-receipt");
        const baseline = await image("baseline");
        const authored = await page.evaluate(() => receiptTest.renderer.debugSelectionFrameJson());
        await click(SHAPES[0]);
        const selected = await image("selected");
        result.pixels = assertSelectionPixels(baseline, selected, SHAPES[0]);
        assert.equal(await page.evaluate(() => receiptTest.renderer.debugSelectionFrameJson()), authored);
        await clear(); assertExactPixels(await image("clear"), baseline, "clear restores scene exactly");
        result.checks.push("precise-click-and-overlay-only-clear");

        if (!program) {
          // A publication-only change must become presented without requiring
          // another pointer event to discover its stale receipt.
          const oldReceipt = await receipt();
          await page.evaluate(() => { receiptTest.renderer.seekDirect(0.25); receiptTest.driver.wake(); });
          await settled(); status = await receipt();
          assert.notEqual(status.current, oldReceipt.current);
          assert.equal(status.presented, status.current, "publication-only wake refreshes the receipt");
          assertExactPixels(await image("publication-only-refresh"), baseline, "static seek preserves pixels");
          await page.evaluate(() => { receiptTest.renderer.seekDirect(0); receiptTest.driver.wake(); });
          await settled();
          result.checks.push("publication-only-wake-refresh-without-input");
        }

        // The release is collected synchronously before the notification microtask
        // can paint the resize/seek. It must never be retagged to that newer frame.
        for (const invalidation of program ? ["surface"] : ["publication", "surface"]) {
          const rect = await page.locator("#scene").boundingBox(); const point = shapeSurfaceCenter(SHAPES[0]);
          await page.mouse.move(rect.x + point.x, rect.y + point.y); await page.mouse.down();
          const rejected = await page.evaluate(({ invalidation, point }) => {
            const t = receiptTest, press = t.trace.findLast(row => row.args[0] === "press");
            if (invalidation === "publication") t.renderer.seekDirect(0.25);
            else t.renderer.resize(1280, 720);
            const before = JSON.parse(t.renderer.debugPointerPresentationJson());
            t.event("pointerup", press.args[2], point.x, point.y);
            return { before, release: t.trace.at(-1) };
          }, { invalidation, point });
          assert.equal(rejected.release.args[0], "release"); assert.equal(rejected.release.admitted, false);
          if (invalidation === "publication") assert.notEqual(rejected.before.presented, rejected.before.current);
          else assert.equal(rejected.before.presented, null);
          await page.mouse.up(); await settled();
          status = await receipt(); assert.equal(status.presented, status.current); assert.equal(status.refreshPending, false);
          const recovered = await image(`${invalidation}-rejected`);
          // A backing-size change can alter antialiasing; the fresh baseline is
          // captured before a new click rather than reusing old-resolution pixels.
          await click(SHAPES[0]);
          assertSelectionPixels(recovered, await image(`${invalidation}-fresh`), SHAPES[0]);
          await clear(); assertExactPixels(await image(`${invalidation}-clear`), recovered, "fresh contact clears exactly");
          result.checks.push(`${invalidation}-reject-cancel-fresh-contact`);
        }
        // Real mouse down/up straddle a complete surface reset and repaint.
        // No intervening pointer event can do the cancellation for the host.
        for (const reset of ["backing-resize", "suspend-resume"]) {
          const rect = await page.locator("#scene").boundingBox();
          const point = shapeSurfaceCenter(SHAPES[0]);
          await page.mouse.move(rect.x + point.x, rect.y + point.y);
          await page.mouse.down(); await settled();
          const beforeReset = await page.evaluate(() => ({
            trace: receiptTest.trace.length,
            authored: receiptTest.renderer.debugSelectionFrameJson(),
          }));
          await page.evaluate(reset => {
            const t = receiptTest;
            if (reset === "suspend-resume") t.renderer.resize(0, 0);
            t.renderer.resize(960, 540);
            t.driver.wake();
          }, reset);
          await settled();
          assert.equal(await page.evaluate(() => receiptTest.trace.length), beforeReset.trace,
            "surface reset/repaint must not rely on another DOM occurrence");
          status = await receipt(); assert.equal(status.presented, status.current);
          const repainted = await image(`${reset}-before-release`);
          await page.mouse.up(); await settled();
          assertExactPixels(await image(`${reset}-after-release`), repainted,
            `repainted-surface-release-must-not-click: ${reset}`);
          assert.equal(await page.evaluate(() => receiptTest.renderer.debugSelectionFrameJson()), beforeReset.authored);
          await click(SHAPES[0]);
          assertSelectionPixels(repainted, await image(`${reset}-fresh`), SHAPES[0]);
          await clear(); assertExactPixels(await image(`${reset}-clear`), repainted, `${reset} clear`);
          result.checks.push(`${reset}-repaint-before-release-cancels-and-recovers`);
        }
        if (!program) {
          await page.evaluate(() => { receiptTest.renderer.seekDirect(0); receiptTest.driver.wake(); }); await settled();
        }
        const beforeView = await receipt();
        await page.evaluate(() => { receiptTest.canvas.style.marginLeft = "12px"; window.dispatchEvent(new Event("scroll")); });
        await settled(); status = await receipt();
        assert.equal(status.presented, status.current); assert.deepEqual(status.view, beforeView.view);
        result.checks.push("same-sized-dom-view-change-represented");
        const idle = await page.evaluate(() => {
          const t = receiptTest; const before = t.driver.stats();
          for (let i = 0; i < 128; ++i) { if (t.renderer.render()) throw new Error("clean frame redrew"); }
          return { before, after: t.driver.stats() };
        });
        assert.deepEqual(idle.before, idle.after); result.checks.push("clean-paused-host-does-not-redraw");
        result.status = "passed"; console.log(`[PASS] ${name}: ${result.checks.join(", ")}`);
      } catch (error) { result.status = "failed"; result.error = String(error.stack ?? error); throw error; }
      finally {
        await page.evaluate(() => { receiptTest.detach(); receiptTest.driver.stop(); receiptTest.renderer.free(); }).catch(() => {});
        await page.close();
      }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); console.error(report.error); process.exitCode = 1; }
finally { await browser?.close(); await server?.close(); await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2)); }

// Actual DOM -> authoring worker -> Rust session -> render worker -> pixels.
// This entrypoint requires the patched WASM build. It never substitutes a mock
// player, generated image or DOM-only result for the full qualification.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { VIEW, SHAPES, selectionFixtureSource, shapeSurfaceCenter,
  assertExactPixels, assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_POINTER_SELECTION_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/pointer-selection"));
const source = selectionFixtureSource();
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const report = { scope: "worker-rust-raster", status: "running", pythonSourceSha256: hash(source), cases: [] };
await mkdir(output, { recursive: true });
let server, browser;
try {
  // A missing or stale package is an explicit failure, not a skipped test.
  for (const name of ["noon_web.js", "noon_web_bg.wasm", "noon_web.d.ts"]) {
    const file = path.join(root, "web/pkg", name);
    assert.ok((await stat(file).catch(() => null))?.isFile(),
      `patched WASM package missing: web/pkg/${name}; run scripts/build-web-demo.sh`);
  }
  const declarations = await readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8");
  assert.match(declarations, /setPointerFillSelection\(/, "WASM package predates session selection configuration");
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  report.collectorSha256 = hash(await readFile(path.join(root, "web/browser-pointer-input.js")));
  const { PNG } = await import("pngjs");
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend),
      ...(process.env.NOON_CHROMIUM_EXECUTABLE
        ? { executablePath: process.env.NOON_CHROMIUM_EXECUTABLE } : { channel: "chromium" }),
    });
    report.browser = browser.version();
    for (const transportMode of ["transferable", "shared"]) {
      const result = { backend, transportMode, status: "running", steps: [] };
      report.cases.push(result);
      const context = await browser.newContext({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
      const page = await context.newPage();
      page.setDefaultTimeout(90_000);
      const pageErrors = [];
      page.on("pageerror", error => pageErrors.push(String(error)));
      const prefix = `${backend}-${transportMode}`;
      const image = async label => {
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${prefix}-${label}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const debug = () => page.evaluate(() => pointerSelection.execution.debugFrame());
      const metrics = () => page.evaluate(async () => (await pointerSelection.execution.metrics()).metrics);
      const drain = () => page.evaluate(() => pointerSelection.execution.state());
      const settled = async ({ fence = false } = {}) => {
        await drain();
        // For negative/no-op assertions, cross the existing acknowledgement
        // barrier before reading pixels. Positive click/clear checks deliberately
        // wait for autonomous presentation first; this cannot supply their wake.
        if (fence) await page.evaluate(() => pointerSelection.execution.advanceTo(0));
        await page.waitForFunction(async () => {
          if (pointerSelection.errors.length) throw new Error(pointerSelection.errors.join("; "));
          const { metrics } = await pointerSelection.execution.metrics();
          return metrics.ready && metrics.retained && metrics.presentedFrames > 0 &&
            !metrics.needsPresent && metrics.bufferedDeltas === 0;
        });
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(pageErrors, []);
        assert.deepEqual(await page.evaluate(() => pointerSelection.errors), []);
      };
      const presentAfter = async before => {
        await drain();
        await page.waitForFunction(async frames =>
          (await pointerSelection.execution.metrics()).metrics.presentedFrames > frames,
        before.presentedFrames);
        await settled();
      };
      const point = async shape => {
        const bounds = await page.locator("#scene").boundingBox();
        assert.ok(bounds);
        const center = shapeSurfaceCenter(shape);
        return { x: bounds.x + center.x, y: bounds.y + center.y };
      };
      const click = async shape => {
        const location = await point(shape);
        await page.mouse.click(location.x, location.y);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async ({ source, transportMode, view }) => {
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
          const canvas = document.querySelector("#scene");
          canvas.width = view.width; canvas.height = view.height;
          canvas.style.width = `${view.width}px`; canvas.style.height = `${view.height}px`;
          const authoring = new PythonAuthoringClient();
          const errors = [];
          const record = error => errors.push(String(error));
          const execution = new AuthoringExecutionClient(canvas, { onError: record, onRecoverableError: record });
          window.pointerSelection = { authoring, execution, errors };
          const authored = await authoring.run(source);
          if (authored.duration !== 0 || !authored.semanticExecution ||
              authored.semanticExecution.continuationGeneration != null) {
            throw new Error("selection fixture must return a static shared context");
          }
          await execution.startSemanticExecution(authored.semanticExecution, {
            authoringClient: authoring, initiallyPaused: true, transportMode,
          });
          await execution.advanceTo(0);
        }, { source, transportMode, view: VIEW });
        await settled();
        assert.equal((await metrics()).backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal((await metrics()).transportMode, transportMode);
        const baseline = await image("baseline"), authored = await debug();
        assert.equal(authored.time, 0); assert.equal(authored.present_object_count, 3);
        assert.equal((await drain()).playing, false);
        await page.evaluate(() => pointerSelection.execution.setPointerFillSelection(4));
        await settled({ fence: true });
        assertExactPixels(await image("enabled"), baseline, "enable is not a click");

        let before = await metrics();
        await click(SHAPES[0]); await presentAfter(before);
        const circle = await image("circle");
        result.steps.push({ name: "circle", ...assertSelectionPixels(baseline, circle, SHAPES[0]) });
        assert.deepEqual(await debug(), authored, "selection must not mutate authored frame/publication");

        // Repeat a click, then observe a bounded quiet interval. The harness's
        // RAF callbacks are observations, not replacement engine scheduling.
        before = await metrics(); await click(SHAPES[0]); await settled({ fence: true });
        await page.evaluate(() => new Promise(resolve => {
          let frames = 12;
          const observe = () => { if (--frames === 0) resolve(); else requestAnimationFrame(observe); };
          requestAnimationFrame(observe);
        }));
        assert.equal((await metrics()).presentedFrames, before.presentedFrames, "same selection must settle");
        assertExactPixels(await image("same-selection"), circle, "repeat click");
        result.steps.push({ name: "settled-repeat", status: "passed" });

        // Synchronous DOM dispatch creates an explicit coalesced out-and-back
        // history through the production collector. It is intentionally marked
        // synthetic, unlike the real mouse clicks above and below.
        await page.evaluate(() => {
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          const make = (type, dx, buttons) => new PointerEvent(type, {
            pointerId: 97, pointerType: "mouse", isPrimary: true,
            clientX: rect.left + dx, clientY: rect.top + 320,
            button: type === "pointermove" ? -1 : 0, buttons,
          });
          canvas.dispatchEvent(make("pointerdown", 20, 1));
          const samples = [make("pointermove", 60, 1), make("pointermove", 20, 1)];
          const parent = make("pointermove", 20, 1);
          Object.defineProperty(parent, "getCoalescedEvents", { value: () => samples });
          canvas.dispatchEvent(parent); canvas.dispatchEvent(make("pointerup", 20, 0));
        });
        await settled({ fence: true });
        assertExactPixels(await image("coalesced-return"), circle, "out-and-back must not clear selection");
        result.steps.push({ name: "coalesced-no-click", stimulus: "injected-coalesced-pointer-events", status: "passed" });

        before = await metrics(); await click(SHAPES[1]); await presentAfter(before);
        result.steps.push({ name: "rectangle", ...assertSelectionPixels(baseline, await image("rectangle"), SHAPES[1]) });
        assert.deepEqual(await debug(), authored);

        before = await metrics();
        const bounds = await page.locator("#scene").boundingBox();
        await page.mouse.click(bounds.x + 20, bounds.y + 320); await presentAfter(before);
        assertExactPixels(await image("cleared"), baseline, "background clear");
        assert.deepEqual(await debug(), authored);
        result.steps.push({ name: "clear", status: "passed" });

        // A press cancelled by surface exit cannot be completed by re-entry.
        const center = await point(SHAPES[0]);
        await page.mouse.move(center.x, center.y); await page.mouse.down();
        await page.mouse.move(850, 550); await page.mouse.move(center.x, center.y); await page.mouse.up();
        await settled({ fence: true });
        assertExactPixels(await image("cancelled"), baseline, "cancelled gesture");
        result.steps.push({ name: "cancelled-click", stimulus: "trusted-browser-mouse", status: "passed" });

        before = await metrics(); await click(SHAPES[0]); await presentAfter(before);
        before = await metrics();
        await page.evaluate(() => pointerSelection.execution.setPointerFillSelection(null));
        await presentAfter(before);
        assertExactPixels(await image("disabled"), baseline, "disable clears overlay");
        await click(SHAPES[1]); await settled({ fence: true });
        assertExactPixels(await image("disabled-click"), baseline, "disabled policy remains disabled");
        assert.deepEqual(await debug(), authored);
        assert.equal((await drain()).playing, false);
        result.steps.push({ name: "disable", status: "passed" });
        result.status = "passed";
        console.log(`[PASS] ${prefix}: actual paused click/clear pixels and unchanged authored frame`);
      } catch (error) {
        result.status = "failed"; result.error = String(error.stack ?? error);
        throw error;
      } finally {
        await page.evaluate(() => {
          window.pointerSelection?.execution.terminate(); window.pointerSelection?.authoring.terminate();
        }).catch(() => {});
        await context.close();
      }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error);
  console.error(report.error); process.exitCode = 1;
} finally {
  await browser?.close(); await server?.close();
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
}

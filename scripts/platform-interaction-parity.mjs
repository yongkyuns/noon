// Same declared selection scene and ordered DOM input trace through the direct
// Rust/WASM path and Python authoring worker. This qualifies filled-shape
// selection only; it does not claim click-action or drag parity.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { VIEW, SHAPES, selectionFixtureSource, shapeSurfaceCenter, assertExactPixels }
  from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_PLATFORM_INTERACTION_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/platform-interaction-parity"));
const source = selectionFixtureSource();
const hash = value => createHash("sha256").update(value).digest("hex");
const trace = [
  { id: "select-circle", point: () => shapeSurfaceCenter(SHAPES[0]) },
  { id: "select-rectangle", point: () => shapeSurfaceCenter(SHAPES[1]) },
  { id: "clear-background", point: () => ({ x: VIEW.width - 20, y: VIEW.height - 20 }) },
];
const report = {
  status: "running",
  scope: "static-filled-shape-selection",
  pythonSourceSha256: hash(source),
  rustFixture: "noon::example_scenes::pointer_selection::scene",
  cases: [],
};
let server, browser;
await mkdir(output, { recursive: true });

function equalPixels(left, right, label) {
  assert.deepEqual([left.width, left.height], [right.width, right.height], `${label}: dimensions`);
  assertExactPixels(left, right, label, VIEW);
}

async function image(page, label, backend, pathName) {
  const bytes = await page.locator("#scene").screenshot();
  await writeFile(path.join(output, `${backend}-${pathName}-${label}.png`), bytes);
  return PNG.sync.read(bytes);
}

try {
  const declarationPath = path.join(root, "web/pkg/noon_web.d.ts");
  const wasmPath = path.join(root, "web/pkg/noon_web_bg.wasm");
  const declarations = await readFile(declarationPath, "utf8");
  assert.match(declarations, /createDirectPointerSelectionRenderer\(/,
    "build the patched web package with NOON_RENDERER_SMOKE=1 before this qualification");
  report.wasmSha256 = hash(await readFile(wasmPath));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });

  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, channel: "chromium", args: browserArgs(backend) });
    const result = { backend, status: "running", checkpoints: [] };
    report.cases.push(result);
    const pages = {};
    const errors = { direct: [], worker: [] };
    try {
      for (const pathName of ["direct", "worker"]) {
        const page = await browser.newPage({ viewport: { width: 640, height: 360 }, deviceScaleFactor: 1 });
        pages[pathName] = page;
        page.setDefaultTimeout(90_000);
        page.on("pageerror", error => errors[pathName].push(String(error)));
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      }

      await pages.direct.evaluate(async () => {
        const nativeWorker = globalThis.Worker;
        globalThis.Worker = class { constructor() { throw new Error("direct path created a worker"); } };
        try {
          const { default: init, createDirectPointerSelectionRenderer } = await import("./pkg/noon_web.js");
          await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const canvas = document.querySelector("#scene"), errors = [];
          canvas.width = 640; canvas.height = 360;
          const renderer = await createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), false, false);
          renderer.setPointerFillSelection(4);
          const driver = createDirectExecutionWakeDriver(renderer);
          const detach = attachNativeInputs(renderer, canvas, {
            onInput: () => driver.wake(), onError: error => errors.push(String(error)),
          });
          globalThis.parity = { renderer, driver, detach, errors };
        } finally { globalThis.Worker = nativeWorker; }
      });

      await pages.worker.evaluate(async ({ source, view }) => {
        const { PythonAuthoringClient } = await import("./authoring-client.js");
        const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
        const canvas = document.querySelector("#scene"), errors = [];
        canvas.width = view.width; canvas.height = view.height;
        canvas.style.width = `${view.width}px`; canvas.style.height = `${view.height}px`;
        const authoring = new PythonAuthoringClient();
        const execution = new AuthoringExecutionClient(canvas, {
          onError: error => errors.push(String(error)), onRecoverableError: error => errors.push(String(error)),
        });
        const authored = await authoring.run(source);
        if (authored.duration !== 0 || authored.semanticExecution?.continuationGeneration != null) {
          throw new Error("parity scene must be a static shared context");
        }
        await execution.startSemanticExecution(authored.semanticExecution, {
          authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
        });
        await execution.advanceTo(0);
        await execution.setPointerFillSelection(4);
        globalThis.parity = { authoring, execution, errors };
      }, { source, view: VIEW });

      const settleDirect = async before => {
        await pages.direct.waitForFunction(count => parity.driver.stats().presentedFrames > count, before);
        await pages.direct.waitForFunction(() => parity.driver.stats().idle);
        await pages.direct.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await pages.direct.evaluate(() => parity.errors), []);
      };
      const workerFrames = () => pages.worker.evaluate(async () =>
        (await parity.execution.metrics()).metrics.presentedFrames);
      const settleWorker = async before => {
        await pages.worker.waitForFunction(async count => {
          if (parity.errors.length) throw new Error(parity.errors.join("; "));
          const { metrics } = await parity.execution.metrics();
          return metrics.ready && metrics.retained && metrics.presentedFrames > count &&
            !metrics.needsPresent && metrics.bufferedDeltas === 0;
        }, before);
        await pages.worker.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await pages.worker.evaluate(() => parity.errors), []);
      };
      await pages.direct.waitForFunction(() => parity.driver.stats().idle);
      await pages.worker.waitForFunction(async () => {
        const { metrics } = await parity.execution.metrics();
        return metrics.ready && metrics.retained && metrics.presentedFrames > 0 && !metrics.needsPresent;
      });
      const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
      assert.equal(await pages.direct.evaluate(() => parity.renderer.rendererBackend()), expectedBackend);
      const initialWorkerMetrics = await pages.worker.evaluate(async () => (await parity.execution.metrics()).metrics);
      assert.equal(initialWorkerMetrics.backend, expectedBackend);
      assert.equal(initialWorkerMetrics.transportMode, "transferable");
      assert.equal(await pages.direct.evaluate(() => parity.renderer.objectCount()), 3);
      assert.equal((await pages.worker.evaluate(() => parity.execution.debugFrame())).present_object_count, 3);

      const directBaseline = await image(pages.direct, "baseline", backend, "direct");
      const workerBaseline = await image(pages.worker, "baseline", backend, "worker");
      equalPixels(directBaseline, workerBaseline, `${backend} baseline`);
      let directPixels = directBaseline;
      let workerPixels = workerBaseline;
      result.checkpoints.push({ id: "baseline", directSha256: hash(directPixels.data), workerSha256: hash(workerPixels.data) });

      for (const action of trace) {
        const point = action.point();
        const directBefore = await pages.direct.evaluate(() => parity.driver.stats().presentedFrames);
        const workerBefore = await workerFrames();
        for (const pathName of ["direct", "worker"]) {
          const bounds = await pages[pathName].locator("#scene").boundingBox();
          assert.ok(bounds, `${pathName} canvas must be visible`);
          await pages[pathName].mouse.click(bounds.x + point.x, bounds.y + point.y);
        }
        await Promise.all([settleDirect(directBefore), settleWorker(workerBefore)]);
        const priorDirect = directPixels;
        const priorWorker = workerPixels;
        directPixels = await image(pages.direct, action.id, backend, "direct");
        workerPixels = await image(pages.worker, action.id, backend, "worker");
        if (action.id === "clear-background") {
          assertExactPixels(directPixels, directBaseline, `${backend} direct clear returns to baseline`, VIEW);
          assertExactPixels(workerPixels, workerBaseline, `${backend} worker clear returns to baseline`, VIEW);
        } else {
          assert.notDeepEqual(directPixels.data, priorDirect.data, `${backend} direct ${action.id} changes pixels`);
          assert.notDeepEqual(workerPixels.data, priorWorker.data, `${backend} worker ${action.id} changes pixels`);
        }
        equalPixels(directPixels, workerPixels, `${backend} ${action.id}`);
        result.checkpoints.push({ id: action.id, directSha256: hash(directPixels.data), workerSha256: hash(workerPixels.data) });
      }

      const directFrame = await pages.direct.evaluate(() => JSON.parse(parity.renderer.debugSelectionFrameJson()));
      const workerFrame = await pages.worker.evaluate(() => parity.execution.debugFrame());
      assert.equal(directFrame.time, 0); assert.equal(workerFrame.time, 0);
      result.status = "passed";
    } catch (error) {
      result.status = "failed"; result.error = String(error.stack ?? error); throw error;
    } finally {
      await pages.direct?.evaluate(() => { parity.detach(); parity.driver.stop(); parity.renderer.free(); }).catch(() => {});
      for (const page of Object.values(pages)) await page.close();
      await browser.close(); browser = null;
    }
    assert.deepEqual(errors, { direct: [], worker: [] });
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); throw error;
} finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server?.close();
}

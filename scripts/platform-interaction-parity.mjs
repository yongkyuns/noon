// Paired selection, click-action, and drag fixtures receive ordered DOM input
// through direct Rust/WASM and Python-worker hosts.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { VIEW, SHAPES, selectionFixtureSource, shapeSurfaceCenter, assertExactPixels,
  assertSelectionPixels }
  from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_PLATFORM_INTERACTION_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/platform-interaction-parity"));
const source = selectionFixtureSource();
const movingSource = selectionFixtureSource({ moving: true });
const clickGlowPulseSource = await readFile(path.join(root, "web/python/examples/click_glow_pulse.py"), "utf8");
const clickIndicateSource = source
  .replace("Click a filled shape; background clears", "Click a filled shape; it restores automatically")
  .replace("        self.add(shape0, shape1, label)", [
    "        self.add(shape0, shape1, label)",
    "        self.on_click(shape0, Indicate(shape0, run_time=1.0))",
    "        self.on_click(shape1, Indicate(shape1, run_time=1.0))",
  ].join("\n"));
const movingClickIndicateSource = selectionFixtureSource({ moving: true })
  .replace("Click a filled shape; background clears", "Click a filled shape; it restores automatically")
  .replace("        self.add(shape0, shape1, label)", [
    "        self.add(shape0, shape1, label)",
    "        self.on_click(shape0, Indicate(shape0, run_time=1.0))",
    "        self.on_click(shape1, Indicate(shape1, run_time=1.0))",
  ].join("\n"));
const dragSource = `from noon import *

class DragFixture(Scene):
    def construct(self):
        target = Circle(radius=0.9, color=Color(0, 0, 1), fill_opacity=1, stroke_width=0)
        unrelated = Square(side_length=1, color=Color(0, 0.75, 0), fill_opacity=1, stroke_width=0).shift([2.5, 1, 0])
        self.add(target, unrelated)
        self.set_drag_targets(target)
`;
const hash = value => createHash("sha256").update(value).digest("hex");
const trace = [
  { id: "select-circle", point: () => shapeSurfaceCenter(SHAPES[0]) },
  { id: "select-rectangle", point: () => shapeSurfaceCenter(SHAPES[1]) },
  { id: "clear-background", point: () => ({ x: VIEW.width - 20, y: VIEW.height - 20 }) },
];
const report = {
  status: "running",
  scope: "click-glow-pulse-static-and-authored-motion-click-indicate-moving-selection-and-translation-drag",
  pythonSourceSha256: hash(source),
  movingPythonSourceSha256: hash(movingSource),
  clickIndicatePythonSourceSha256: hash(clickIndicateSource),
  movingClickIndicatePythonSourceSha256: hash(movingClickIndicateSource),
  clickGlowPulsePythonSourceSha256: hash(clickGlowPulseSource),
  dragPythonSourceSha256: hash(dragSource),
  rustFixture: "noon::example_scenes::pointer_selection::scene",
  movingRustFixture: "noon::example_scenes::pointer_selection::moving_selection_session",
  clickIndicateRustFixture: "noon::example_scenes::pointer_selection::click_indicate_scene",
  clickGlowPulseRustFixture: "noon::example_scenes::click_glow_pulse::execution",
  dragRustFixture: "createDirectNativeDragSmokeRenderer",
  cases: [],
};
let server, browser;
await mkdir(output, { recursive: true });

function equalPixels(left, right, label) {
  assert.deepEqual([left.width, left.height], [right.width, right.height], `${label}: dimensions`);
  assertExactPixels(left, right, label, VIEW);
}

function matchedVisibleObjectState(objects) {
  const quantize = value => {
    if (typeof value === "number") return Number(value.toFixed(5));
    if (Array.isArray(value)) return value.map(quantize);
    if (value && typeof value === "object") {
      return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, quantize(child)]));
    }
    return value;
  };
  return objects.map(({ appearance, bounds, center, fill, morph, present, reveal, transform }) =>
    quantize({ appearance, bounds, center, fill, morph, present, reveal, transform }));
}

async function image(page, label, backend, pathName) {
  const bytes = await page.locator("#scene").screenshot();
  await writeFile(path.join(output, `${backend}-${pathName}-${label}.png`), bytes);
  return PNG.sync.read(bytes);
}

async function openParityPages(pages, errors) {
  for (const pathName of ["direct", "worker"]) {
    const page = await browser.newPage({ viewport: { width: 640, height: 360 }, deviceScaleFactor: 1 });
    pages[pathName] = page;
    page.setDefaultTimeout(90_000);
    page.on("pageerror", error => errors[pathName].push(String(error)));
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  }
}

async function movingParity(backend) {
  const pages = {};
  const errors = { direct: [], worker: [] };
  const checkpoints = [];
  const settlePaint = () => Promise.all(Object.values(pages).map(page =>
    page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))));
  try {
    await openParityPages(pages, errors);
    await pages.direct.evaluate(async () => {
      const nativeWorker = globalThis.Worker;
      globalThis.Worker = class { constructor() { throw new Error("direct path created a worker"); } };
      try {
        const { default: init, createDirectMovingPointerSelectionRenderer } = await import("./pkg/noon_web.js");
        await init();
        const { attachNativeInputs } = await import("./native-inputs.js");
        const canvas = document.querySelector("#scene"), errors = [];
        canvas.width = 640; canvas.height = 360;
        const renderer = await createDirectMovingPointerSelectionRenderer(canvas.transferControlToOffscreen());
        renderer.setPointerFillSelection(4);
        for (let attempt = 0; attempt < 60; attempt++) {
          if (renderer.render()) break;
          if (attempt === 59) throw new Error("direct moving fixture did not present its initial frame");
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        const detach = attachNativeInputs(renderer, canvas, {
          onInput: () => {}, onError: error => errors.push(String(error)),
        });
        globalThis.parity = { renderer, detach, errors };
      } finally { globalThis.Worker = nativeWorker; }
    });
    await pages.worker.evaluate(async (source) => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const canvas = document.querySelector("#scene"), errors = [];
      canvas.width = 640; canvas.height = 360;
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas, {
        onError: error => errors.push(String(error)),
        onRecoverableError: error => errors.push(String(error)),
      });
      let resolveAttached, rejectAttached;
      const attached = new Promise((resolve, reject) => {
        resolveAttached = resolve; rejectAttached = reject;
      });
      const authored = authoring.run(source, {}, {
        async onSemanticContinuation(registration) {
          try {
            await execution.startSemanticExecution(registration.semanticExecution, {
              authoringClient: authoring, pacing: "external_samples", transportMode: "transferable",
            });
            resolveAttached();
          } catch (error) { rejectAttached(error); }
        },
      });
      authored.catch(rejectAttached);
      await attached;
      await execution.setPointerFillSelection(4);
      globalThis.parity = { authoring, execution, authored, errors };
    }, movingSource);

    const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
    assert.equal(await pages.direct.evaluate(() => parity.renderer.rendererBackend()), expectedBackend);
    assert.equal((await pages.worker.evaluate(async () => (await parity.execution.metrics()).metrics)).backend,
      expectedBackend);
    let selectedMidpoint;
    for (const time of [0, 1, 1.5, 1.8]) {
      await pages.direct.evaluate(async time => {
        const { renderer } = parity;
        let pending;
        if (time === 1.5) {
          // Seek is a replay boundary and intentionally clears transient selection.
          // Advance the same live session so the selected target follows its transform.
          const anchor = performance.now();
          renderer.directWakeDirectiveJson(anchor);
          pending = renderer.advanceDirectRealtime(anchor + 500);
        } else {
          pending = renderer.seekDirect(time);
        }
        if (!pending) return;
        for (let attempt = 0; attempt < 60; attempt++) {
          if (renderer.render()) return;
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error(`direct moving selection did not present at ${time}`);
      }, time);
      assert.ok(Math.abs(await pages.direct.evaluate(() => parity.renderer.time()) - time) < 1e-5);
      await pages.worker.evaluate(time => parity.execution.sampleToAuthoredTime(time), time);
      await pages.worker.waitForFunction(async () => {
        const { metrics } = await parity.execution.metrics();
        return metrics.ready && metrics.retained && !metrics.needsPresent && metrics.bufferedDeltas === 0;
      });
      await settlePaint();
      const direct = await image(pages.direct, `moving-${time}`, backend, "direct");
      const worker = await image(pages.worker, `moving-${time}`, backend, "worker");
      equalPixels(direct, worker, `${backend} moving ${time}`);
      checkpoints.push({ id: `moving-${time}`, directSha256: hash(direct.data), workerSha256: hash(worker.data) });
      if (time === 1.5) {
        assert.notDeepEqual(direct.data, selectedMidpoint.data,
          `${backend} selected shape keeps moving after the click`);
        const workerBefore = await pages.worker.evaluate(async () =>
          (await parity.execution.metrics()).metrics.presentedFrames);
        for (const pathName of ["direct", "worker"]) {
          const bounds = await pages[pathName].locator("#scene").boundingBox();
          assert.ok(bounds);
          await pages[pathName].mouse.click(bounds.x + VIEW.width - 20, bounds.y + VIEW.height - 20);
        }
        await pages.direct.evaluate(async () => {
          for (let attempt = 0; attempt < 60; attempt++) {
            if (parity.renderer.render()) return;
            await new Promise(resolve => setTimeout(resolve, 10));
          }
          throw new Error("direct moving selection did not clear after background click");
        });
        await pages.worker.waitForFunction(async count => {
          const { metrics } = await parity.execution.metrics();
          return metrics.presentedFrames > count && !metrics.needsPresent;
        }, workerBefore);
        await settlePaint();
        const clearedDirect = await image(pages.direct, "moving-cleared-1.5", backend, "direct");
        const clearedWorker = await image(pages.worker, "moving-cleared-1.5", backend, "worker");
        assertSelectionPixels(clearedDirect, direct, { ...SHAPES[0], x: SHAPES[0].x + 1.35 });
        assertSelectionPixels(clearedWorker, worker, { ...SHAPES[0], x: SHAPES[0].x + 1.35 });
        equalPixels(clearedDirect, clearedWorker, `${backend} moving cleared`);
        checkpoints.push({ id: "moving-cleared-1.5", directSha256: hash(clearedDirect.data),
          workerSha256: hash(clearedWorker.data) });
      }
      if (time === 1.8) {
        // The moving circle now overlaps the later painter-order rectangle.
        // A shared hit test must pick the visible front target on both hosts.
        const overlap = shapeSurfaceCenter({ x: 0.8, y: 0 });
        const directRevision = await pages.direct.evaluate(() => String(parity.renderer.directSceneRevision()));
        const workerFrame = await pages.worker.evaluate(() => parity.execution.debugFrame());
        assert.ok(Math.abs(workerFrame.objects[0].center[0] - 0.02) < 1e-5);
        const workerBefore = await pages.worker.evaluate(async () =>
          (await parity.execution.metrics()).metrics.presentedFrames);
        for (const pathName of ["direct", "worker"]) {
          const bounds = await pages[pathName].locator("#scene").boundingBox();
          assert.ok(bounds);
          await pages[pathName].mouse.click(bounds.x + overlap.x, bounds.y + overlap.y);
        }
        await pages.direct.evaluate(async () => {
          for (let attempt = 0; attempt < 60; attempt++) {
            if (parity.renderer.render()) return;
            await new Promise(resolve => setTimeout(resolve, 10));
          }
          throw new Error("direct overlap selection did not present after click");
        });
        await pages.worker.waitForFunction(async count => {
          const { metrics } = await parity.execution.metrics();
          return metrics.presentedFrames > count && !metrics.needsPresent && metrics.bufferedDeltas === 0;
        }, workerBefore);
        await settlePaint();
        const selectedDirect = await image(pages.direct, "moving-overlap-selected-1.8", backend, "direct");
        const selectedWorker = await image(pages.worker, "moving-overlap-selected-1.8", backend, "worker");
        assertSelectionPixels(direct, selectedDirect, SHAPES[1]);
        assertSelectionPixels(worker, selectedWorker, SHAPES[1]);
        equalPixels(selectedDirect, selectedWorker, `${backend} moving overlap selection`);
        assert.equal(await pages.direct.evaluate(() => String(parity.renderer.directSceneRevision())), directRevision);
        assert.equal((await pages.worker.evaluate(() => parity.execution.debugFrame())).publication.scene_revision,
          workerFrame.publication.scene_revision);
        checkpoints.push({ id: "moving-overlap-selected-1.8", directSha256: hash(selectedDirect.data),
          workerSha256: hash(selectedWorker.data) });
      }
      if (time === 1) {
        const point = shapeSurfaceCenter({ ...SHAPES[0], x: SHAPES[0].x + 0.9 });
        const workerBefore = await pages.worker.evaluate(async () =>
          (await parity.execution.metrics()).metrics.presentedFrames);
        for (const pathName of ["direct", "worker"]) {
          const bounds = await pages[pathName].locator("#scene").boundingBox();
          assert.ok(bounds);
          await pages[pathName].mouse.click(bounds.x + point.x, bounds.y + point.y);
        }
        await pages.direct.evaluate(async () => {
          for (let attempt = 0; attempt < 60; attempt++) {
            if (parity.renderer.render()) return;
            await new Promise(resolve => setTimeout(resolve, 10));
          }
          throw new Error("direct moving selection did not present after click");
        });
        await pages.worker.waitForFunction(async count => {
          const { metrics } = await parity.execution.metrics();
          return metrics.presentedFrames > count &&
            !metrics.needsPresent && metrics.bufferedDeltas === 0;
        }, workerBefore);
        await settlePaint();
        const selectedDirect = await image(pages.direct, "moving-selected-1", backend, "direct");
        const selectedWorker = await image(pages.worker, "moving-selected-1", backend, "worker");
        assert.notDeepEqual(selectedDirect.data, direct.data, `${backend} direct midpoint selection visible`);
        assert.notDeepEqual(selectedWorker.data, worker.data, `${backend} worker midpoint selection visible`);
        assertSelectionPixels(direct, selectedDirect, { ...SHAPES[0], x: SHAPES[0].x + 0.9 });
        assertSelectionPixels(worker, selectedWorker, { ...SHAPES[0], x: SHAPES[0].x + 0.9 });
        equalPixels(selectedDirect, selectedWorker, `${backend} moving selected midpoint`);
        selectedMidpoint = selectedDirect;
        checkpoints.push({ id: "moving-selected-1", directSha256: hash(selectedDirect.data),
          workerSha256: hash(selectedWorker.data) });
      }
    }
    assert.deepEqual(errors, { direct: [], worker: [] });
    return checkpoints;
  } finally {
    await pages.direct?.evaluate(() => { parity.detach(); parity.renderer.free(); }).catch(() => {});
    await pages.worker?.evaluate(() => { parity.execution.terminate(); parity.authoring.terminate(); }).catch(() => {});
    for (const page of Object.values(pages)) await page.close();
  }
}

async function dragParity(backend) {
  const pages = {}, errors = { direct: [], worker: [] }, checkpoints = [];
  try {
    await openParityPages(pages, errors);
    await pages.direct.evaluate(async () => {
      const nativeWorker = globalThis.Worker;
      globalThis.Worker = class { constructor() { throw new Error("direct drag created a worker"); } };
      try {
        const { default: init, createDirectNativeDragSmokeRenderer } = await import("./pkg/noon_web.js");
        await init();
        const { attachNativeInputs } = await import("./native-inputs.js");
        const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
        const canvas = document.querySelector("#scene"), errors = [];
        canvas.width = 640; canvas.height = 360;
        const renderer = await createDirectNativeDragSmokeRenderer(canvas.transferControlToOffscreen());
        const driver = createDirectExecutionWakeDriver(renderer);
        const detach = attachNativeInputs(renderer, canvas, {
          onInput: () => driver.wake(), onError: error => errors.push(String(error)),
        });
        globalThis.dragParity = { renderer, driver, detach, errors };
      } finally { globalThis.Worker = nativeWorker; }
    });
    await pages.worker.evaluate(async source => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const canvas = document.querySelector("#scene"), errors = [];
      canvas.width = 640; canvas.height = 360;
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas, {
        onError: error => errors.push(String(error)),
        onRecoverableError: error => errors.push(String(error)),
      });
      const authored = await authoring.run(source);
      if (authored.duration !== 0 || authored.semanticExecution?.continuationGeneration != null) {
        throw new Error("drag parity must be a static, native interaction scene");
      }
      await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
      });
      await execution.advanceTo(0);
      globalThis.dragParity = { authoring, execution, errors };
    }, dragSource);
    await pages.direct.waitForFunction(() =>
      dragParity.driver.stats().idle && dragParity.driver.stats().presentedFrames > 0);
    await pages.worker.waitForFunction(async () => {
      const { metrics } = await dragParity.execution.metrics();
      return metrics.ready && metrics.retained && metrics.presentedFrames > 0 && !metrics.needsPresent;
    });
    const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
    assert.equal(await pages.direct.evaluate(() => dragParity.renderer.rendererBackend()), expectedBackend);
    assert.equal((await pages.worker.evaluate(async () => (await dragParity.execution.metrics()).metrics)).backend,
      expectedBackend);
    const initialRevision = await pages.direct.evaluate(() => String(dragParity.renderer.directSceneRevision()));
    const initialWorkerFrame = await pages.worker.evaluate(() => dragParity.execution.debugFrame());
    assert.equal(initialWorkerFrame.present_object_count, 2);
    const initialWorkerRevision = initialWorkerFrame.publication.scene_revision;
    const capture = async label => {
      const direct = await image(pages.direct, label, backend, "direct-drag");
      const worker = await image(pages.worker, label, backend, "worker-drag");
      equalPixels(direct, worker, `${backend} drag ${label}`);
      checkpoints.push({ id: label, directSha256: hash(direct.data), workerSha256: hash(worker.data) });
      return direct;
    };
    const baseline = await capture("baseline");
    const start = { x: VIEW.width / 2, y: VIEW.height / 2 };
    const end = { x: start.x + VIEW.height / VIEW.cameraHeight, y: start.y };
    const directBeforeMove = await pages.direct.evaluate(() => dragParity.driver.stats().presentedFrames);
    const workerBeforeMove = await pages.worker.evaluate(async () =>
      (await dragParity.execution.metrics()).metrics.presentedFrames);
    for (const pathName of ["direct", "worker"]) {
      const bounds = await pages[pathName].locator("#scene").boundingBox();
      assert.ok(bounds);
      await pages[pathName].mouse.move(bounds.x + start.x, bounds.y + start.y);
      await pages[pathName].mouse.down();
      await pages[pathName].mouse.move(bounds.x + end.x, bounds.y + end.y);
    }
    await pages.direct.waitForFunction(count =>
      dragParity.driver.stats().presentedFrames > count && dragParity.driver.stats().idle,
    directBeforeMove);
    await pages.worker.waitForFunction(async count => {
      const { metrics } = await dragParity.execution.metrics();
      return metrics.presentedFrames > count && !metrics.needsPresent && metrics.bufferedDeltas === 0;
    }, workerBeforeMove);
    const moved = await capture("effective-move");
    assert.notDeepEqual(moved.data, baseline.data, `${backend} drag must move a visible target`);
    assert.equal(await pages.direct.evaluate(() => String(dragParity.renderer.directSceneRevision())), initialRevision,
      `${backend} samples must not author scene revisions`);
    const movedWorkerFrame = await pages.worker.evaluate(() => dragParity.execution.debugFrame());
    assert.equal(movedWorkerFrame.publication.scene_revision, initialWorkerRevision);
    assert.ok(Math.abs(movedWorkerFrame.objects[0].center[0] - 1) < 1e-5);
    assert.ok(Math.abs(movedWorkerFrame.objects[1].center[0] - 2.5) < 1e-5);
    for (const pathName of ["direct", "worker"]) await pages[pathName].mouse.up();
    await pages.direct.waitForFunction(initial =>
      dragParity.driver.stats().idle && dragParity.renderer.directSceneRevision() === BigInt(initial) + 1n,
    initialRevision);
    await pages.worker.waitForFunction(async initial => {
      const { metrics } = await dragParity.execution.metrics();
      const frame = await dragParity.execution.debugFrame();
      return !metrics.needsPresent && metrics.bufferedDeltas === 0 &&
        frame.publication.scene_revision === initial + 1;
    }, initialWorkerRevision);
    const released = await capture("released");
    assertExactPixels(released, moved, `${backend} released drag retains the effective value`);
    assert.equal(BigInt(await pages.direct.evaluate(() => String(dragParity.renderer.directSceneRevision()))),
      BigInt(initialRevision) + 1n, `${backend} release must reconcile exactly one authored edit`);
    assert.equal((await pages.worker.evaluate(() => dragParity.execution.debugFrame())).publication.scene_revision,
      initialWorkerRevision + 1, `${backend} worker release must reconcile exactly one authored edit`);
    assert.deepEqual(errors, { direct: [], worker: [] });
    return checkpoints;
  } finally {
    await pages.direct?.evaluate(() => { dragParity.detach(); dragParity.driver.stop(); dragParity.renderer.free(); }).catch(() => {});
    await pages.worker?.evaluate(() => { dragParity.execution.terminate(); dragParity.authoring.terminate(); }).catch(() => {});
    for (const page of Object.values(pages)) await page.close();
  }
}

async function clickIndicateParity(backend, { moving = false } = {}) {
  const pages = {}, errors = { direct: [], worker: [] }, checkpoints = [];
  const waitUntil = async (label, probe, timeoutMs = 90_000) => {
    const deadline = Date.now() + timeoutMs;
    do {
      if (await probe()) return;
      await new Promise(resolve => setTimeout(resolve, 16));
    } while (Date.now() < deadline);
    throw new Error(`${label} did not settle before ${timeoutMs} ms`);
  };
  try {
    await openParityPages(pages, errors);
    await pages.direct.evaluate(async moving => {
      const nativeWorker = globalThis.Worker;
      globalThis.Worker = class { constructor() { throw new Error("direct click-Indicate path created a worker"); } };
      try {
        const { default: init, createDirectPointerSelectionRenderer,
          createDirectMovingClickIndicateRenderer } = await import("./pkg/noon_web.js");
        await init();
        const { attachNativeInputs } = await import("./native-inputs.js");
        const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
        const canvas = document.querySelector("#scene"), errors = [];
        canvas.width = 640; canvas.height = 360;
        const renderer = moving
          ? await createDirectMovingClickIndicateRenderer(canvas.transferControlToOffscreen())
          : await createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), false, true);
        // Match the worker's external-sample pacing. The wake driver still
        // advances input effects, while authored motion stays at the selected time.
        if (moving) renderer.pauseDirect(performance.now());
        const driver = createDirectExecutionWakeDriver(renderer);
        const detach = attachNativeInputs(renderer, canvas, {
          onInput: () => driver.wake(), onError: error => errors.push(String(error)),
        });
        globalThis.clickIndicateParity = { renderer, driver, detach, errors, moving };
      } finally { globalThis.Worker = nativeWorker; }
    }, moving);
    await pages.worker.evaluate(async ({ source, moving }) => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const canvas = document.querySelector("#scene"), errors = [];
      canvas.width = 640; canvas.height = 360;
      canvas.style.width = "640px"; canvas.style.height = "360px";
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas, {
        onError: error => errors.push(String(error)),
        onRecoverableError: error => errors.push(String(error)),
      });
      let authored;
      if (moving) {
        let resolveAttached, rejectAttached;
        const attached = new Promise((resolve, reject) => {
          resolveAttached = resolve; rejectAttached = reject;
        });
        authored = authoring.run(source, {}, {
          async onSemanticContinuation(registration) {
            try {
              await execution.startSemanticExecution(registration.semanticExecution, {
                authoringClient: authoring, pacing: "external_samples", transportMode: "transferable",
              });
              resolveAttached();
            } catch (error) { rejectAttached(error); }
          },
        });
        authored.catch(rejectAttached);
        await attached;
      } else {
        authored = await authoring.run(source);
      }
      if (!moving) {
        if (authored.duration !== 0 || authored.semanticExecution?.continuationGeneration != null) {
          throw new Error("click-Indicate parity must remain paused at authored time zero");
        }
        await execution.startSemanticExecution(authored.semanticExecution, {
          authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
        });
        await execution.advanceTo(0);
      }
      globalThis.clickIndicateParity = { authoring, execution, errors, moving };
    }, { source: moving ? movingClickIndicateSource : clickIndicateSource, moving });

    let matchedAuthoredTime = moving ? 1 : 0;
    if (moving) {
      await pages.direct.evaluate(async () => {
        const { renderer } = clickIndicateParity;
        if (!renderer.seekDirect(1)) throw new Error("direct moving Indicate fixture did not seek to midpoint");
        for (let attempt = 0; attempt < 60; attempt++) {
          if (renderer.render()) return;
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error("direct moving Indicate fixture did not present its midpoint");
      });
      await pages.worker.evaluate(() => clickIndicateParity.execution.sampleToAuthoredTime(1));
      await waitUntil("moving click-Indicate midpoint", () => pages.worker.evaluate(async () => {
        const frame = await clickIndicateParity.execution.debugFrame();
        const { metrics } = await clickIndicateParity.execution.metrics();
        return frame.time === 1 && Math.abs(frame.objects[0].center[0] + 0.7) < 1e-5 &&
          metrics.ready && !metrics.needsPresent && metrics.bufferedDeltas === 0;
      }));
      await pages.direct.waitForFunction(() => clickIndicateParity.renderer.time() === 1);
    }

    const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
    assert.equal(await pages.direct.evaluate(() => clickIndicateParity.renderer.rendererBackend()), expectedBackend);
    assert.equal((await pages.worker.evaluate(async () =>
      (await clickIndicateParity.execution.metrics()).metrics)).backend, expectedBackend);
    await pages.direct.waitForFunction(() => clickIndicateParity.driver.stats().idle);
    await waitUntil("worker baseline", () => pages.worker.evaluate(async () => {
      const { metrics } = await clickIndicateParity.execution.metrics();
      return metrics.ready && metrics.retained && metrics.presentedFrames > 0 && !metrics.needsPresent;
    }));
    const capture = async (host, label) => image(pages[host], label, backend, `click-indicate-${host}`);
    const baseline = {
      direct: await capture("direct", "baseline"),
      worker: await capture("worker", "baseline"),
    };
    equalPixels(baseline.direct, baseline.worker, `${backend} click-Indicate baseline`);
    const initial = {
      direct: await pages.direct.evaluate(() => JSON.parse(clickIndicateParity.renderer.debugSelectionFrameJson())),
      worker: await pages.worker.evaluate(() => clickIndicateParity.execution.debugFrame()),
    };
    const authoredTime = moving ? 1 : 0;
    assert.equal(initial.direct.time, authoredTime); assert.equal(initial.worker.time, authoredTime);
    const revision = {
      direct: String(initial.direct.publication.scene_revision),
      worker: String(initial.worker.publication.scene_revision),
    };
    const completionRevision = { ...revision };
    const point = shapeSurfaceCenter({ ...SHAPES[0], x: SHAPES[0].x + (moving ? 0.9 : 0) });
    const frameFor = host => pages[host].evaluate(host => host === "direct"
      ? JSON.parse(clickIndicateParity.renderer.debugSelectionFrameJson())
      : clickIndicateParity.execution.debugFrame(), host);
    const activeFrameFor = async (host, presentedBefore) => {
      const deadline = Date.now() + 5000;
      do {
        const frame = await frameFor(host);
        const object = frame.objects[0];
        const presented = host === "direct"
          ? await pages.direct.evaluate(before =>
            clickIndicateParity.driver.stats().presentedFrames > before, presentedBefore)
          : await pages.worker.evaluate(async before => {
            const { metrics } = await clickIndicateParity.execution.metrics();
            return metrics.presentedFrames > before && !metrics.needsPresent &&
              metrics.bufferedDeltas === 0;
          }, presentedBefore);
        if (presented && Math.abs(object.transform.scale.x) >
            Math.abs(initial[host].objects[0].transform.scale.x) * 1.05 &&
            object.fill?.red > 0.5 &&
            object.fill?.green > 0.5 && object.fill?.blue < 0.5) return frame;
        await new Promise(resolve => setTimeout(resolve, 10));
      } while (Date.now() < deadline);
      const diagnostics = await pages[host].evaluate(async host => ({
        frame: host === "direct"
          ? JSON.parse(clickIndicateParity.renderer.debugSelectionFrameJson())
          : await clickIndicateParity.execution.debugFrame(),
        ...(host === "worker"
          ? { metrics: (await clickIndicateParity.execution.metrics()).metrics,
            errors: clickIndicateParity.errors }
          : {}),
      }), host);
      throw new Error(`${host} click did not reach active Rust Indicate state: ${JSON.stringify(diagnostics)}`);
    };
    const active = {};
    for (const host of ["direct", "worker"]) {
      if (moving && host === "direct") {
        // Begin the independently qualified realtime-motion case only after
        // both hosts have captured the same paused midpoint picture.
        await pages.direct.evaluate(() => {
          clickIndicateParity.renderer.resumeDirect();
          clickIndicateParity.driver.wake();
        });
      }
      const presentedBefore = host === "direct"
        ? await pages.direct.evaluate(() => clickIndicateParity.driver.stats().presentedFrames)
        : (await pages.worker.evaluate(async () =>
          (await clickIndicateParity.execution.metrics()).metrics.presentedFrames));
      const bounds = await pages[host].locator("#scene").boundingBox();
      assert.ok(bounds);
      await pages[host].mouse.click(bounds.x + point.x, bounds.y + point.y);
      active[host] = await activeFrameFor(host, presentedBefore);
      if (host === "direct") {
        // The direct driver and frame probe can observe the same RAF boundary
        // in opposite order. Flush the already-active session frame before the
        // screenshot so the pixel assertion measures that exact state.
        await pages.direct.evaluate(() => clickIndicateParity.renderer.render());
      }
      const pixels = await capture(host, "active");
      if (moving && host === "direct") {
        assert.ok(active[host].time >= 1 && active[host].time <= 2,
          "direct click interaction leaves authored motion running within its segment");
      } else {
        assert.equal(active[host].time, authoredTime,
          `${host} click action preserves its authored sample`);
      }
      assert.equal(String(active[host].publication.scene_revision), revision[host],
        `${host} click action must not author a scene edit`);
      assert.ok(Math.abs(active[host].objects[0].transform.scale.x) >
        Math.abs(initial[host].objects[0].transform.scale.x) * 1.05,
        `${host} Rust Indicate must visibly scale the target`);
      assert.notDeepEqual(pixels.data, baseline[host].data, `${host} active Indicate changes pixels`);
      checkpoints.push({ id: `${host}-active`, authoredTime: active[host].time,
        sceneRevision: String(active[host].publication.scene_revision), scaleX: active[host].objects[0].transform.scale.x });
    }
    for (const host of ["direct", "worker"]) {
      if (host === "direct") {
        await pages.direct.waitForFunction(() => clickIndicateParity.driver.stats().idle);
      } else {
        await waitUntil("worker Indicate restoration", () => pages.worker.evaluate(async base => {
          const frame = await clickIndicateParity.execution.debugFrame();
          const { metrics } = await clickIndicateParity.execution.metrics();
          const restored = frame.objects[0];
          // Scale can settle below its tolerance before the interaction's
          // yellow-to-authored-fill transition reaches its exact endpoint.
          // Require the original four color channels too; never promote a
          // nearly-restored transient as a completed state.
          const fillRestored = restored?.fill != null && base.fill != null &&
            ["red", "green", "blue", "alpha"].every(channel =>
              restored.fill[channel] === base.fill[channel]);
          return Math.abs(restored.transform.scale.x - base.transform.scale.x) < 1e-5 &&
            fillRestored && !metrics.needsPresent && metrics.bufferedDeltas === 0;
        }, initial.worker.objects[0]));
      }
    }
    if (moving) {
      const directFinal = await pages.direct.evaluate(() => ({
        time: clickIndicateParity.renderer.time(),
        frame: JSON.parse(clickIndicateParity.renderer.debugSelectionFrameJson()),
        wakeDriver: clickIndicateParity.driver.stats(),
      }));
      assert.equal(directFinal.wakeDriver.idle, true, "direct authored driver settles before final comparison");
      assert.ok(Number.isFinite(directFinal.time) && directFinal.time >= 2 && directFinal.time < 2 + 1 / 30,
        `direct authored motion settles at its segment endpoint: ${JSON.stringify(directFinal)}`);
      assert.ok(Number.isFinite(directFinal.frame.time) && directFinal.frame.time >= 2 &&
        directFinal.frame.time < 2 + 1 / 30,
      "direct authored frame settles within one RAF of the segment endpoint");
      matchedAuthoredTime = 2;
      await pages.worker.evaluate(time => clickIndicateParity.execution.sampleToAuthoredTime(time), matchedAuthoredTime);
      await waitUntil("worker final authored sample", () => pages.worker.evaluate(async time => {
        const frame = await clickIndicateParity.execution.debugFrame();
        const { metrics } = await clickIndicateParity.execution.metrics();
        return frame.time === time &&
          metrics.ready && !metrics.needsPresent && metrics.bufferedDeltas === 0;
      }, matchedAuthoredTime));
      const workerFinal = await pages.worker.evaluate(() => clickIndicateParity.execution.debugFrame());
      assert.equal(workerFinal.time, matchedAuthoredTime, "worker is sampled to direct's canonical authored time");
      // Source lowering assigns different node IDs, and zero-width stroke metadata
      // differs even though it cannot affect display. Compare painter-ordered visible
      // state at sub-pixel float precision; the following RGBA assertion stays exact.
      assert.deepEqual(matchedVisibleObjectState(workerFinal.objects),
        matchedVisibleObjectState(directFinal.frame.objects),
        "matched authored samples have identical ordered visible object state");
      completionRevision.direct = String(directFinal.frame.publication.scene_revision);
      completionRevision.worker = String(workerFinal.publication.scene_revision);
    }
    const restored = {
      direct: await capture("direct", "restored"),
      worker: await capture("worker", "restored"),
    };
    for (const host of ["direct", "worker"]) {
      if (!moving) {
        assertExactPixels(restored[host], baseline[host], `${backend} ${host} click-Indicate restoration`, VIEW);
      }
      const frame = await frameFor(host);
      if (moving && host === "direct") {
        assert.ok(Number.isFinite(frame.time) && frame.time >= 2 && frame.time < 2 + 1 / 30,
          "direct restored frame is at its bounded realtime endpoint");
      } else {
        assert.equal(frame.time, matchedAuthoredTime,
          `${host} restored frame is at the matched authored sample`);
      }
      assert.equal(String(frame.publication.scene_revision), completionRevision[host],
        `${host} completion leaves the matched authored revision unchanged`);
    }
    equalPixels(restored.direct, restored.worker, `${backend} restored click-Indicate`);
    const workerFramesBeforeIdleCheck = await pages.worker.evaluate(async () =>
      (await clickIndicateParity.execution.metrics()).metrics.presentedFrames);
    await pages.worker.evaluate(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const workerFramesAfterIdleCheck = await pages.worker.evaluate(async () =>
      (await clickIndicateParity.execution.metrics()).metrics.presentedFrames);
    assert.equal(workerFramesAfterIdleCheck, workerFramesBeforeIdleCheck,
      "settled worker Indicate stays idle across two RAFs");
    const directIdle = await pages.direct.evaluate(() => ({
      idle: clickIndicateParity.driver.stats().idle,
      redrew: clickIndicateParity.renderer.render(),
    }));
    assert.equal(directIdle.idle, true); assert.equal(directIdle.redrew, false);
    checkpoints.push({ id: "restored-and-idle", directSha256: hash(restored.direct.data),
      workerSha256: hash(restored.worker.data) });
    assert.deepEqual(errors, { direct: [], worker: [] });
    assert.deepEqual(await pages.direct.evaluate(() => clickIndicateParity.errors), []);
    assert.deepEqual(await pages.worker.evaluate(() => clickIndicateParity.errors), []);
    return checkpoints;
  } finally {
    await pages.direct?.evaluate(() => {
      clickIndicateParity.detach(); clickIndicateParity.driver.stop(); clickIndicateParity.renderer.free();
    }).catch(() => {});
    await pages.worker?.evaluate(() => {
      clickIndicateParity.execution.terminate(); clickIndicateParity.authoring.terminate();
    }).catch(() => {});
    for (const page of Object.values(pages)) await page.close();
  }
}

// Inspect actual halo pixels outside the known source coverage. Independent wall
// clocks may sample different amplitudes, so active cross-host equality belongs
// to the deterministic native oracle; baseline and restored equality stay exact.
function inspectClickGlowHalo(before, after, target, label) {
  assert.deepEqual([after.width, after.height], [VIEW.width, VIEW.height]);
  const center = shapeSurfaceCenter(target);
  const halfExtent = 0.55 * VIEW.height / VIEW.cameraHeight;
  let haloPixels = 0;
  let sourceInteriorPixels = 0;
  for (let y = 0; y < VIEW.height; y++) {
    for (let x = 0; x < VIEW.width; x++) {
      const dx = Math.abs(x + 0.5 - center.x), dy = Math.abs(y + 0.5 - center.y);
      const distance = target.kind === "circle" ? Math.hypot(dx, dy) : Math.max(dx, dy);
      const offset = (y * VIEW.width + x) * 4;
      const difference = Math.max(...[0, 1, 2].map(channel =>
        Math.abs(after.data[offset + channel] - before.data[offset + channel])));
      if (distance < halfExtent - 2) {
        sourceInteriorPixels++;
        assert.equal(difference, 0, `${label}: opaque source interior changed at ${x},${y}`);
      }
      if (difference > 2) {
        assert.ok(dx < halfExtent + 48 && dy < halfExtent + 48,
          `${label}: unrelated pixel changed at ${x},${y}`);
        if (distance > halfExtent + 2) haloPixels++;
      }
    }
  }
  assert.ok(sourceInteriorPixels > 500, `${label}: source interior control is too small`);
  return { haloPixels, sourceInteriorPixels };
}

async function clickGlowPulseParity(backend) {
  const pages = {}, errors = { direct: [], worker: [] }, checkpoints = [];
  const targets = [
    { kind: "circle", x: -2, y: 0, index: 0, label: "persistent" },
    { kind: "rectangle", x: 2, y: 0, index: 1, label: "temporary" },
  ];
  const frameFor = host => pages[host].evaluate(host => host === "direct"
    ? JSON.parse(clickGlowPulseParity.renderer.debugSelectionFrameJson())
    : clickGlowPulseParity.execution.debugFrame(), host);
  const waitUntil = async (label, probe, timeoutMs = 90_000) => {
    const deadline = Date.now() + timeoutMs;
    do {
      const result = await probe();
      if (result) return result;
      await new Promise(resolve => setTimeout(resolve, 10));
    } while (Date.now() < deadline);
    throw new Error(`${label} did not settle before ${timeoutMs} ms`);
  };
  const capture = (host, label) => image(pages[host], label, backend, `click-glow-pulse-${host}`);
  try {
    await openParityPages(pages, errors);
    await pages.direct.evaluate(async () => {
      const nativeWorker = globalThis.Worker;
      globalThis.Worker = class { constructor() { throw new Error("direct click-GlowPulse path created a worker"); } };
      try {
        const { default: init, createDirectClickGlowPulseRenderer } = await import("./pkg/noon_web.js");
        await init();
        const { attachNativeInputs } = await import("./native-inputs.js");
        const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
        const canvas = document.querySelector("#scene"), errors = [];
        canvas.width = 640; canvas.height = 360;
        const renderer = await createDirectClickGlowPulseRenderer(canvas.transferControlToOffscreen());
        const driver = createDirectExecutionWakeDriver(renderer);
        const detach = attachNativeInputs(renderer, canvas, {
          onInput: () => driver.wake(), onError: error => errors.push(String(error)),
        });
        globalThis.clickGlowPulseParity = { renderer, driver, detach, errors };
      } finally { globalThis.Worker = nativeWorker; }
    });
    await pages.worker.evaluate(async source => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const canvas = document.querySelector("#scene"), errors = [];
      canvas.width = 640; canvas.height = 360;
      canvas.style.width = "640px"; canvas.style.height = "360px";
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas, {
        onError: error => errors.push(String(error)),
        onRecoverableError: error => errors.push(String(error)),
      });
      const authored = await authoring.run(source);
      if (authored.duration !== 0 || authored.semanticExecution?.continuationGeneration != null) {
        throw new Error("click-GlowPulse example must finish at authored time zero");
      }
      await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
      });
      await execution.advanceTo(0);
      globalThis.clickGlowPulseParity = { authoring, execution, errors };
    }, clickGlowPulseSource);

    const expectedBackend = backend === "webgpu" ? "WebGPU" : "WebGL2";
    assert.equal(await pages.direct.evaluate(() => clickGlowPulseParity.renderer.rendererBackend()), expectedBackend);
    await pages.direct.waitForFunction(() => clickGlowPulseParity.driver.stats().idle);
    await waitUntil("worker click-GlowPulse baseline", () => pages.worker.evaluate(async () => {
      const { metrics } = await clickGlowPulseParity.execution.metrics();
      return metrics.ready && metrics.retained && metrics.presentedFrames > 0 &&
        !metrics.needsPresent && metrics.bufferedDeltas === 0;
    }));
    const workerMetrics = await pages.worker.evaluate(async () =>
      (await clickGlowPulseParity.execution.metrics()).metrics);
    assert.equal(workerMetrics.backend, expectedBackend);
    assert.equal(workerMetrics.transportMode, "transferable");
    const initial = { direct: await frameFor("direct"), worker: await frameFor("worker") };
    const baseline = { direct: await capture("direct", "baseline"), worker: await capture("worker", "baseline") };
    equalPixels(baseline.direct, baseline.worker, `${backend} click-GlowPulse baseline`);
    for (const host of ["direct", "worker"]) {
      assert.equal(initial[host].time, 0);
      assert.equal(initial[host].interactions_active, false);
      assert.equal(initial[host].present_object_count, 2);
      assert.equal(initial[host].objects[0].effects[0].definition.intensity, 0.35);
      assert.equal(initial[host].objects[1].effects[0], undefined, `${host}: binding must not preinstall a neutral glow`);
    }

    const restored = {};
    for (const target of targets) {
      for (const host of ["direct", "worker"]) {
        const point = shapeSurfaceCenter(target);
        const bounds = await pages[host].locator("#scene").boundingBox();
        assert.ok(bounds);
        const click = () => pages[host].mouse.click(bounds.x + point.x, bounds.y + point.y);
        await click();
        const baselineIntensity = initial[host].objects[target.index].effects[0]?.definition.intensity ?? 0;
        const active = await waitUntil(`${host} ${target.label} active glow`, async () => {
          const frame = await frameFor(host);
          return frame.interactions_active &&
            frame.objects[target.index].effects[0]?.definition.intensity > baselineIntensity + 0.15 && frame;
        }, 5000);
        // A second genuine DOM click reaches the same active attachment. Exact
        // original-deadline suppression is checked with native deterministic ticks.
        await click();
        const repeated = await frameFor(host);
        assert.equal(repeated.interactions_active, true);
        assert.deepEqual(repeated.objects[target.index].effects[0].attachment,
          active.objects[target.index].effects[0].attachment, `${host}: retrigger changed attachment identity`);
        assert.equal(repeated.time, 0, `${host}: input advanced authored time`);
        assert.deepEqual(matchedVisibleObjectState(repeated.objects), matchedVisibleObjectState(initial[host].objects),
          `${host}: glow pulse changed ordinary source geometry or style`);
        const { pixels, coverage } = await waitUntil(`${host} ${target.label} presented halo`, async () => {
          if (host === "direct") await pages.direct.evaluate(() => clickGlowPulseParity.renderer.render());
          const pixels = await capture(host, `${target.label}-active`);
          const coverage = inspectClickGlowHalo(baseline[host], pixels, target, `${backend} ${host} ${target.label}`);
          return coverage.haloPixels > 40 && { pixels, coverage };
        }, 5000);
        checkpoints.push({ id: `${host}-${target.label}-active`, backend: expectedBackend,
          transport: host === "worker" ? "transferable" : "direct",
          attachment: active.objects[target.index].effects[0].attachment,
          observedIntensity: active.objects[target.index].effects[0].definition.intensity,
          ...coverage, sha256: hash(pixels.data) });

        await waitUntil(`${host} ${target.label} exact restoration`, async () => {
          const frame = await frameFor(host);
          if (frame.interactions_active) return false;
          assert.deepEqual(frame.objects[target.index].effects[0], initial[host].objects[target.index].effects[0],
            `${host}: completion did not restore or retire the exact attachment`);
          if (host === "direct") return pages.direct.evaluate(() => clickGlowPulseParity.driver.stats().idle);
          return pages.worker.evaluate(async () => {
            const { metrics } = await clickGlowPulseParity.execution.metrics();
            return !metrics.needsPresent && metrics.bufferedDeltas === 0;
          });
        });
        await pages[host].evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        restored[host] = await capture(host, `${target.label}-restored`);
        assertExactPixels(restored[host], baseline[host], `${backend} ${host} ${target.label} restoration`, VIEW);
        const finalFrame = await frameFor(host);
        assert.equal(finalFrame.time, 0);
        assert.deepEqual(finalFrame.objects.map(object => object.effects), initial[host].objects.map(object => object.effects));
        if (host === "direct") {
          const idle = await pages.direct.evaluate(() => ({
            idle: clickGlowPulseParity.driver.stats().idle,
            redrew: clickGlowPulseParity.renderer.render(),
          }));
          assert.deepEqual(idle, { idle: true, redrew: false });
        } else {
          const before = await pages.worker.evaluate(async () =>
            (await clickGlowPulseParity.execution.metrics()).metrics.presentedFrames);
          await pages.worker.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          const after = await pages.worker.evaluate(async () =>
            (await clickGlowPulseParity.execution.metrics()).metrics.presentedFrames);
          assert.equal(after, before, "settled click-GlowPulse must stop presenting");
        }
        checkpoints.push({ id: `${host}-${target.label}-restored-idle`, sha256: hash(restored[host].data) });
      }
      equalPixels(restored.direct, restored.worker, `${backend} ${target.label} restored click-GlowPulse`);
    }

    // A genuine viewer seek must retire a visible input pulse through the same
    // source owner, even when the authored playhead is already at the target.
    const temporary = targets[1];
    const sought = {};
    for (const host of ["direct", "worker"]) {
      const point = shapeSurfaceCenter(temporary);
      const bounds = await pages[host].locator("#scene").boundingBox();
      assert.ok(bounds);
      await pages[host].mouse.click(bounds.x + point.x, bounds.y + point.y);
      const active = await waitUntil(`${host} temporary glow before seek`, async () => {
        const frame = await frameFor(host);
        return frame.interactions_active && frame.objects[1].effects[0]?.definition.intensity > 0.15 && frame;
      }, 5000);
      const visible = await waitUntil(`${host} presented halo before seek`, async () => {
        if (host === "direct") await pages.direct.evaluate(() => clickGlowPulseParity.renderer.render());
        const pixels = await capture(host, "active-pulse-before-seek");
        const coverage = inspectClickGlowHalo(baseline[host], pixels, temporary,
          `${backend} ${host} temporary before seek`);
        return coverage.haloPixels > 40 && coverage;
      }, 5000);
      await pages[host].evaluate(async ({ host, attachment }) => {
        const current = host === "direct"
          ? JSON.parse(clickGlowPulseParity.renderer.debugSelectionFrameJson())
          : await clickGlowPulseParity.execution.debugFrame();
        if (!current.interactions_active ||
            JSON.stringify(current.objects[1].effects[0]?.attachment) !== JSON.stringify(attachment)) {
          throw new Error("seek must cancel the exact active attachment whose halo was observed");
        }
        if (host === "direct") {
          clickGlowPulseParity.renderer.seekDirect(0);
          clickGlowPulseParity.driver.wake();
        } else {
          await clickGlowPulseParity.execution.seek(0);
        }
      }, { host, attachment: active.objects[1].effects[0].attachment });
      await waitUntil(`${host} seek retirement`, async () => {
        const frame = await frameFor(host);
        if (frame.interactions_active || frame.objects[1].effects[0] !== undefined || frame.time !== 0) return false;
        assert.deepEqual(frame.objects[0].effects[0], initial[host].objects[0].effects[0],
          `${host}: seeking temporary pulse changed the persistent attachment`);
        if (host === "direct") return pages.direct.evaluate(() => clickGlowPulseParity.driver.stats().idle);
        return pages.worker.evaluate(async () => {
          const { metrics } = await clickGlowPulseParity.execution.metrics();
          return !metrics.needsPresent && metrics.bufferedDeltas === 0;
        });
      });
      await pages[host].evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      sought[host] = await capture(host, "active-pulse-seek");
      assertExactPixels(sought[host], baseline[host], `${backend} ${host} active-pulse seek`, VIEW);
      checkpoints.push({ id: `${host}-active-pulse-seek`, attachment: active.objects[1].effects[0].attachment,
        ...visible, sha256: hash(sought[host].data) });
    }
    equalPixels(sought.direct, sought.worker, `${backend} click-GlowPulse seek parity`);

    // Replace the source while its input-owned attachment is still live. Worker
    // replacement uses the same fresh Run/reconcile seam as the Playground;
    // direct Rust destroys its old typed source and creates a new one.
    const reloaded = {};
    for (const host of ["direct", "worker"]) {
      const previousSession = host === "worker" ? await pages.worker.evaluate(async () =>
        (await clickGlowPulseParity.execution.metrics()).metrics.presentedSession) : null;
      const point = shapeSurfaceCenter(temporary);
      const bounds = await pages[host].locator("#scene").boundingBox();
      assert.ok(bounds);
      await pages[host].mouse.click(bounds.x + point.x, bounds.y + point.y);
      const active = await waitUntil(`${host} temporary glow before reload`, async () => {
        const frame = await frameFor(host);
        return frame.interactions_active && frame.objects[1].effects[0]?.definition.intensity > 0.15 && frame;
      }, 5000);
      assert.equal(active.time, 0, `${host}: reload click advanced authored time`);
      assert.deepEqual(active.objects.map(object => object.id), initial[host].objects.map(object => object.id),
        `${host}: reload click changed the current source`);
      assert.deepEqual(matchedVisibleObjectState(active.objects), matchedVisibleObjectState(initial[host].objects),
        `${host}: reload click changed ordinary source geometry or style`);
      const { pixels, coverage } = await waitUntil(`${host} presented halo before reload`, async () => {
        if (host === "direct") await pages.direct.evaluate(() => clickGlowPulseParity.renderer.render());
        const pixels = await capture(host, "active-pulse-before-reload");
        const coverage = inspectClickGlowHalo(baseline[host], pixels, temporary,
          `${backend} ${host} temporary before reload`);
        return coverage.haloPixels > 40 && { pixels, coverage };
      }, 5000);
      checkpoints.push({ id: `${host}-active-pulse-before-reload`,
        attachment: active.objects[1].effects[0].attachment, previousSession,
        ...coverage, sha256: hash(pixels.data) });
      let replacement;
      if (host === "direct") {
        replacement = await pages.direct.evaluate(async attachment => {
          const previous = clickGlowPulseParity;
          const active = JSON.parse(previous.renderer.debugSelectionFrameJson());
          if (!active.interactions_active || active.objects[1].effects[0] === undefined) {
            throw new Error("direct source replacement must begin with an active temporary attachment");
          }
          if (JSON.stringify(active.objects[1].effects[0].attachment) !== JSON.stringify(attachment)) {
            throw new Error("direct source replacement must retire the attachment whose halo was observed");
          }
          previous.detach(); previous.driver.stop();
          previous.renderer.retireExecutionSource();
          const retired = JSON.parse(previous.renderer.debugSelectionFrameJson());
          if (retired.interactions_active || retired.objects[1].effects[0] !== undefined) {
            throw new Error("direct source retirement must clear the old invocation and temporary attachment before free");
          }
          if (retired.time !== active.time ||
              JSON.stringify(retired.objects[0].effects[0]) !== JSON.stringify(active.objects[0].effects[0])) {
            throw new Error("direct source retirement changed authored time or the persistent attachment");
          }
          previous.renderer.free();
          const oldCanvas = document.querySelector("#scene");
          const canvas = oldCanvas.cloneNode(false);
          oldCanvas.replaceWith(canvas);
          const { createDirectClickGlowPulseRenderer } = await import("./pkg/noon_web.js");
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const renderer = await createDirectClickGlowPulseRenderer(canvas.transferControlToOffscreen());
          const driver = createDirectExecutionWakeDriver(renderer);
          const errors = previous.errors;
          const detach = attachNativeInputs(renderer, canvas, {
            onInput: () => driver.wake(), onError: error => errors.push(String(error)),
          });
          globalThis.clickGlowPulseParity = { renderer, driver, detach, errors };
          return { retiredAttachment: active.objects[1].effects[0].attachment,
            retiredPublication: active.publication, retirementPublication: retired.publication,
            retiredBeforeFree: true, recreated: true };
        }, active.objects[1].effects[0].attachment);
        await pages.direct.waitForFunction(() => clickGlowPulseParity.driver.stats().idle);
      } else {
        assert.ok(previousSession != null, "worker reload click requires a presented source session");
        replacement = await pages.worker.evaluate(async ({ source, previousSession, attachment }) => {
          const { execution, authoring } = clickGlowPulseParity;
          if ((await execution.metrics()).metrics.presentedSession !== previousSession) {
            throw new Error("worker source changed between the reload click and retirement");
          }
          const active = await execution.debugFrame();
          if (!active.interactions_active || active.objects[1].effects[0] === undefined) {
            throw new Error("worker source replacement must begin with an active temporary attachment");
          }
          if (JSON.stringify(active.objects[1].effects[0].attachment) !== JSON.stringify(attachment)) {
            throw new Error("worker source replacement must retire the attachment whose halo was observed");
          }
          // Authoring also writes this worker's canonical store. Await the old
          // source's release before Run can make its input publication stale.
          await execution.retireExecutionSource();
          const authored = await authoring.run(source);
          const result = await execution.reconcileSemanticExecution(authored.semanticExecution, {
            authoringClient: authoring,
          });
          const fresh = await execution.debugFrame();
          if (fresh.interactions_active || fresh.objects[1].effects[0] !== undefined) {
            throw new Error("fresh Run retained an old input-owned attachment or invocation");
          }
          await execution.pause();
          return { retiredAttachment: active.objects[1].effects[0].attachment,
            retiredPublication: active.publication, previousSession, session: result.ready.session,
            releasedBeforeAuthoring: true };
        }, { source: clickGlowPulseSource, previousSession, attachment: active.objects[1].effects[0].attachment });
        assert.notEqual(replacement.session, replacement.previousSession, "fresh Run must replace the execution session");
        await waitUntil("worker fresh Run presentation", () => pages.worker.evaluate(async session => {
          const { metrics } = await clickGlowPulseParity.execution.metrics();
          return metrics.presentedSession === session && metrics.ready && !metrics.needsPresent &&
            metrics.bufferedDeltas === 0;
        }, replacement.session));
      }
      const fresh = await frameFor(host);
      assert.equal(fresh.time, 0);
      assert.equal(fresh.interactions_active, false);
      assert.equal(fresh.objects[1].effects[0], undefined, `${host}: retired temporary generation survived source replacement`);
      assert.deepEqual(fresh.objects[0].effects[0].definition, initial[host].objects[0].effects[0].definition);
      assert.deepEqual(matchedVisibleObjectState(fresh.objects), matchedVisibleObjectState(initial[host].objects));
      await pages[host].evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      reloaded[host] = await capture(host, "active-pulse-reload");
      assertExactPixels(reloaded[host], baseline[host], `${backend} ${host} active-pulse source replacement`, VIEW);
      if (host === "direct") {
        assert.equal(await pages.direct.evaluate(() => clickGlowPulseParity.driver.stats().idle), true);
        assert.equal(await pages.direct.evaluate(() => clickGlowPulseParity.renderer.render()), false);
      } else {
        const before = await pages.worker.evaluate(async () =>
          (await clickGlowPulseParity.execution.metrics()).metrics.presentedFrames);
        await pages.worker.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const after = await pages.worker.evaluate(async () =>
          (await clickGlowPulseParity.execution.metrics()).metrics.presentedFrames);
        assert.equal(after, before, "replaced source must remain idle after old input is retired");
      }
      checkpoints.push({ id: `${host}-active-pulse-reload`, ...replacement, sha256: hash(reloaded[host].data) });
    }
    equalPixels(reloaded.direct, reloaded.worker, `${backend} click-GlowPulse fresh source parity`);
    assert.deepEqual(errors, { direct: [], worker: [] });
    assert.deepEqual(await pages.direct.evaluate(() => clickGlowPulseParity.errors), []);
    assert.deepEqual(await pages.worker.evaluate(() => clickGlowPulseParity.errors), []);
    console.log(`CLICK_GLOW_PULSE_PASS backend=${expectedBackend} transport=transferable profiles=2 actual_dom=true active_halo=true restoration_exact=true temporary_retirement=true active_seek_exact=true active_reload_exact=true idle=true`);
    return checkpoints;
  } finally {
    await pages.direct?.evaluate(() => {
      clickGlowPulseParity.detach(); clickGlowPulseParity.driver.stop();
      clickGlowPulseParity.renderer.retireExecutionSource();
      clickGlowPulseParity.renderer.free();
    }).catch(() => {});
    await pages.worker?.evaluate(() => {
      clickGlowPulseParity.execution.terminate(); clickGlowPulseParity.authoring.terminate();
    }).catch(() => {});
    for (const page of Object.values(pages)) await page.close();
  }
}

try {
  const declarationPath = path.join(root, "web/pkg/noon_web.d.ts");
  const wasmPath = path.join(root, "web/pkg/noon_web_bg.wasm");
  const declarations = await readFile(declarationPath, "utf8");
  assert.match(declarations, /createDirectPointerSelectionRenderer\(/,
    "build the patched web package with NOON_RENDERER_SMOKE=1 before this qualification");
  assert.match(declarations, /createDirectMovingPointerSelectionRenderer\(/,
    "build the moving-selection renderer-smoke package before this qualification");
  assert.match(declarations, /createDirectMovingClickIndicateRenderer\(/,
    "build the authored-motion click-Indicate renderer-smoke package before this qualification");
  assert.match(declarations, /createDirectClickGlowPulseRenderer\(/,
    "build the click-GlowPulse renderer-smoke package before this qualification");
  assert.match(declarations, /createDirectNativeDragSmokeRenderer\(/,
    "build the native-drag renderer-smoke package before this qualification");
  report.wasmSha256 = hash(await readFile(wasmPath));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });

  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, channel: "chromium", args: browserArgs(backend) });
    const result = { backend, status: "running", checkpoints: [] };
    report.cases.push(result);
    const pages = {};
    const errors = { direct: [], worker: [] };
    try {
      await openParityPages(pages, errors);

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
          assert.notDeepEqual(directPixels.data, priorDirect.data, `${backend} direct clear changes the selected image`);
          assert.notDeepEqual(workerPixels.data, priorWorker.data, `${backend} worker clear changes the selected image`);
          assertExactPixels(directPixels, directBaseline, `${backend} direct clear returns to baseline`, VIEW);
          assertExactPixels(workerPixels, workerBaseline, `${backend} worker clear returns to baseline`, VIEW);
        } else {
          assert.notDeepEqual(directPixels.data, priorDirect.data, `${backend} direct ${action.id} changes pixels`);
          assert.notDeepEqual(workerPixels.data, priorWorker.data, `${backend} worker ${action.id} changes pixels`);
          assert.notDeepEqual(directPixels.data, directBaseline.data, `${backend} direct ${action.id} is visibly selected`);
          assert.notDeepEqual(workerPixels.data, workerBaseline.data, `${backend} worker ${action.id} is visibly selected`);
        }
        equalPixels(directPixels, workerPixels, `${backend} ${action.id}`);
        result.checkpoints.push({ id: action.id, directSha256: hash(directPixels.data), workerSha256: hash(workerPixels.data) });
      }

      const directFrame = await pages.direct.evaluate(() => JSON.parse(parity.renderer.debugSelectionFrameJson()));
      const workerFrame = await pages.worker.evaluate(() => parity.execution.debugFrame());
      assert.equal(directFrame.time, 0); assert.equal(workerFrame.time, 0);
    } catch (error) {
      result.status = "failed"; result.error = String(error.stack ?? error); throw error;
    } finally {
      await pages.direct?.evaluate(() => { parity.detach(); parity.driver.stop(); parity.renderer.free(); }).catch(() => {});
      for (const page of Object.values(pages)) await page.close();
    }
    try {
      assert.deepEqual(errors, { direct: [], worker: [] });
      result.movingCheckpoints = await movingParity(backend);
      result.dragCheckpoints = await dragParity(backend);
      result.clickIndicateCheckpoints = await clickIndicateParity(backend);
      result.movingClickIndicateCheckpoints = await clickIndicateParity(backend, { moving: true });
      result.clickGlowPulseCheckpoints = await clickGlowPulseParity(backend);
      result.status = "passed";
    } catch (error) {
      result.status = "failed"; result.error = String(error.stack ?? error); throw error;
    } finally {
      await browser.close(); browser = null;
    }
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); throw error;
} finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  await browser?.close(); await server?.close();
}

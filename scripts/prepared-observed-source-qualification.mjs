// Initial playback admission through the actual Python, engine and render workers.
// Source counters are observations; the harness never supplies authored samples.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs, classifyRendererGpuIdentity } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_OBSERVED_AUTHORING_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-observed-source");
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const encode = value => JSON.stringify(value, (_, item) => typeof item === "bigint" ? item.toString() : item, 2);
const source = `from noon import *
from js import JSON, postMessage
import json
events, samples = [], []
def observe():
    postMessage(JSON.parse(json.dumps({"channel": "observed.qualification", "events": events, "samples": samples})))
def mark(value):
    events.append(value)
    observe()
class OriginalSource(Scene):
    def construct(self):
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(target)
        identity = target.id
        def update(obj, dt):
            assert dt >= 0
            samples.append(dt)
            obj.move_to(sum(samples) * RIGHT)
            observe()
        target.add_updater(update)
        mark("start")
        self.wait(1)
        assert abs(target.get_center()[0] - 1) < 1e-6
        mark("first")
        self.wait(1)
        assert target.id == identity
        assert abs(target.get_center()[0] - 2) < 1e-6
        assert abs(sum(samples) - 2) < 1e-9
        assert len(samples) > 2
        mark("end")
    def tear_down(self):
        mark("cleanup")
`;
const causalSource = source.slice(0, source.indexOf("class OriginalSource")) + `class InputSource(Scene):
    def construct(self):
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(target)
        draggable = Circle(radius=0.2, color=BLUE, fill_opacity=1, stroke_width=0).shift(4 * RIGHT)
        self.add(draggable)
        self.set_drag_targets(draggable)
        self.on_click(target, Indicate(target, color=RED, run_time=0.5))
        mark("start")
        self.wait(2)
        target.shift(RIGHT)
        mark("end")
    def tear_down(self):
        mark("cleanup")
`;
const lateSource = source.replace("        target.add_updater(update)",
  '        mark("prefix")\n        self.wait(1)\n        target.add_updater(update)');
const lateCausalSource = causalSource.replace("        self.set_drag_targets(draggable)",
  '        mark("prefix")\n        self.wait(1)\n        self.set_drag_targets(draggable)');
const reference = `from noon import *
class FinalPicture(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0).shift(2 * RIGHT))
`;
const causalReference = `from noon import *
class FinalInputPicture(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0).shift(RIGHT))
        self.add(Circle(radius=0.2, color=BLUE, fill_opacity=1, stroke_width=0).shift(5 * RIGHT))
`;
const heldSource = causalSource.replace("        target.shift(RIGHT)\n", "");
const heldReference = causalReference.replace(".shift(RIGHT))", ")");
const report = {
  status: "running", scope: "observed admission, incremental reader-to-source handoff, pause/resume and completed-source input; original interpreter, pointer receipts and final pixels; no recovery, latency or default-cutover claim",
  lateSourceSha256: hash(lateSource),
  lateCausalSourceSha256: hash(lateCausalSource),
  sourceSha256: hash(source), referenceSha256: hash(reference), cases: [],
  causalSourceSha256: hash(causalSource),
  causalReferenceSha256: hash(causalReference),
  heldSourceSha256: hash(heldSource), heldReferenceSha256: hash(heldReference),
  harnessSha256: hash(await readFile(fileURLToPath(import.meta.url))),
  gallerySourceSha256: hash(await readFile(path.join(root, "web/python/examples/showcase_always_redraw.py"))),
};
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
let server, unisolatedServer, browser;

async function start(page, transportMode, arm) {
  const sources = { ordinary: source, incremental: source, reference, causal: causalSource, "causal-reference": causalReference,
    held: heldSource, "held-reference": heldReference, "late-ordinary": lateSource, "late-incremental": lateSource, "late-causal": lateCausalSource };
  await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  await page.evaluate(async ({ source, transportMode, preparedPlayback, initiallyPaused }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
    const host = { errors: [], events: [], samples: [], registrations: 0, outcome: null, attached: false };
    host.failure = new Promise((_, reject) => { host.rejectFailure = reject; });
    void host.failure.catch(() => {});
    globalThis.observedAuthoring = host;
    const worker = new Worker("./python-worker.js", { type: "module" });
    worker.addEventListener("message", event => {
      if (event.data?.channel !== "observed.qualification") return;
      event.stopImmediatePropagation();
      host.events = event.data.events; host.samples = event.data.samples;
    });
    host.authoring = new PythonAuthoringClient(worker);
    const canvas = document.querySelector("#scene");
    canvas.width = 800; canvas.height = 400;
    canvas.style.width = "800px"; canvas.style.height = "400px";
    host.execution = new AuthoringExecutionClient(canvas, {
      onError: error => host.errors.push(String(error)),
      onRecoverableError: error => host.errors.push(String(error)),
    });
    async function attach(descriptor, duration) {
      const ready = await host.execution.startSemanticExecution(descriptor, {
        authoringClient: host.authoring, transportMode, loopDurationSeconds: Math.max(1, duration),
        initiallyPaused,
      });
      const preview = document.createElement("section"); document.body.append(preview);
      host.controls = new PlaygroundPlaybackControls(host.execution, preview);
      const state = await host.execution.state();
      host.controls.setControllable(state.replaySupported === true);
      host.controls.observe(state);
      host.ready = ready; host.attached = true;
    }
    host.run = host.authoring.run(source, {}, {
      preparedPlayback,
      async onSemanticContinuation(registration) {
        host.registrations++;
        await attach(registration.semanticExecution, registration.duration);
      },
    });
    void host.run.then(async result => {
      if (host.registrations === 0) await attach(result.semanticExecution, result.duration);
      host.outcome = { status: "finished", duration: result.duration };
    }).catch(error => { host.outcome = { status: "failed", error: String(error) }; host.rejectFailure(error); });
  }, { source: sources[arm], transportMode, preparedPlayback: !["ordinary", "late-ordinary"].includes(arm), initiallyPaused: ["incremental", "causal", "held", "late-incremental", "late-causal"].includes(arm) });
}

async function observeSource(page) {
  return page.evaluate(async () => {
    const h = observedAuthoring;
    const state = h.attached ? await Promise.race([h.execution.state(), h.failure]) : null;
    if (state) h.controls.observe(state);
    return { attached: h.attached, outcome: h.outcome, errors: h.errors, events: h.events, samples: h.samples, registrations: h.registrations,
      ...(state ? { state, metrics: (await Promise.race([h.execution.metrics({ includeGpuIdentity: true }), h.failure])).metrics } : {}) };
  });
}

async function untilSource(page, predicate) {
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const state = await observeSource(page);
    assert.deepEqual(state.errors, []);
    assert.notEqual(state.outcome?.status, "failed", encode(state.outcome));
    if (predicate(state)) return state;
    await page.waitForTimeout(25);
  }
  assert.fail(`source observation deadline: ${encode(await observeSource(page))}`);
}

async function clickWhileFrozen(page, prefix, x) {
  const before = await observeSource(page);
  assert.equal(before.state.playing, false);
  const capture = async name => {
    const bytes = await page.locator("#scene").screenshot();
    await writeFile(path.join(output, `${prefix}-${name}.png`), bytes);
    return PNG.sync.read(bytes);
  };
  const baseline = await capture("before");
  const rect = await page.locator("#scene").boundingBox();
  await page.mouse.click(rect.x + x, rect.y + 200);
  await untilSource(page, s => s.metrics.presentedFrames > before.metrics.presentedFrames + 1);
  await page.waitForTimeout(120);
  assert.ok(!(await capture("during")).data.equals(baseline.data), "click must animate while authored time is frozen");
  // Cold GPU work and bounded interaction catch-up can delay completion.
  const deadline = Date.now() + 10_000;
  let restored;
  while (Date.now() < deadline) {
    await page.waitForTimeout(100);
    restored = await capture("after");
    if (restored.data.equals(baseline.data)) break;
  }
  assert.ok(restored.data.equals(baseline.data), "click must restore exact pixels");
  const after = await observeSource(page);
  assert.equal(after.state.time, before.state.time);
  assert.deepEqual(after.events, before.events); assert.deepEqual(after.samples, before.samples);
  return { before, after };
}

await mkdir(output, { recursive: true });
try {
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  report.runtimeIdentity = JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8"));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  unisolatedServer = await serveRepository(root, 0, { crossOriginIsolated: false });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await chromium.launch({
      headless: true, ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : { channel: "chromium" }),
      args: browserArgs(backend, { gpuMode: process.env.NOON_RENDER_GPU_MODE ?? "software" }),
    });
    report.browserVersion = browser.version();
    for (const transportMode of ["transferable", "shared"]) {
      const pictures = new Map();
      for (const arm of ["late-incremental", "late-causal", "late-ordinary", "ordinary", "incremental", "reference", "causal", "causal-reference", "held", "held-reference"]) {
        const entry = { backend, transportMode, arm, status: "running", observations: [] };
        report.cases.push(entry);
        const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
        page.setDefaultTimeout(180_000); await cache.install(page.context());
        const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
        const watchdog = setTimeout(async () => {
          try {
            const diagnostic = await page.evaluate(() => {
              const h = observedAuthoring;
              return { attached: h.attached, outcome: h.outcome, errors: h.errors, events: h.events, samples: h.samples,
                controls: { ...document.querySelector(".playback-controls")?.dataset } };
            });
            await writeFile(path.join(output, `${backend}-${transportMode}-${arm}-pending.json`), encode(diagnostic));
            await page.screenshot({ path: path.join(output, `${backend}-${transportMode}-${arm}-pending.png`) });
          } catch (error) { process.stderr.write(`diagnostic failed: ${error}\n`); }
        }, 30_000);
        try {
          process.stdout.write(`starting ${backend}/${transportMode}/${arm}\n`);
          await start(page, transportMode, arm);
          if (arm === "late-incremental" || arm === "late-causal") {
            const initial = await untilSource(page, s => s.attached && s.metrics?.ready && s.metrics.presentedFrames > 0 && !s.metrics.needsPresent);
            entry.initial = initial;
            assert.equal(initial.state.preparedPlayback, true);
            assert.equal(initial.state.preparedFrontier, 1);
            assert.equal(initial.state.time, 0);
            assert.equal(initial.events[0], "prefix");
            assert.ok(initial.events.length <= 2, "source cannot complete the required segment before playback demand");
            assert.deepEqual(initial.samples, []);
            // The cached seek supplies actual lookahead demand, independent of
            // whether the initial observation arrived before its queued turn.
            await page.evaluate(() => observedAuthoring.execution.seek(0.5));
            await untilSource(page, s => s.events.includes("start"));
            await page.evaluate(() => observedAuthoring.execution.seek(0));
            await page.waitForTimeout(220);
            const cached = await observeSource(page);
            assert.equal(cached.state.preparedPlayback, true);
            assert.deepEqual(cached.events, ["prefix", "start"]);
            assert.deepEqual(cached.samples, []);
            await page.getByRole("button", { name: "Play animation", exact: true }).click();
            const handoff = await untilSource(page, s => s.state?.sourceExecution === "observed" && s.state.time >= 1.1);
            entry.handoff = { initial, cached, handoff };
            assert.equal(handoff.metrics.presentedSession, initial.metrics.presentedSession + 1);
            assert.equal(handoff.metrics.rendererRebuilds, initial.metrics.rendererRebuilds + 1);
            assert.equal(handoff.metrics.gpuGeneration, 1, "the replacement renderer begins its own GPU generation");
            assert.equal(handoff.state.replaySupported, false);
            const controls = await page.locator(".playback-controls").evaluate(el => ({ ...el.dataset }));
            assert.equal(controls.controllable, "false");
            assert.equal(controls.buffering, "false");
            await page.getByRole("button", { name: "Pause animation", exact: true }).click();
            const paused = await untilSource(page, s => !s.state.playing && !s.metrics.needsPresent && s.metrics.bufferedDeltas === 0);
            await page.waitForTimeout(220);
            const frozen = await observeSource(page);
            assert.equal(frozen.state.time, paused.state.time);
            assert.deepEqual(frozen.samples, paused.samples);
            assert.deepEqual(frozen.events, paused.events);
            entry.handoff = { initial, cached, handoff, controls, paused, frozen };
            if (arm === "late-causal") entry.handoff.pausedClick = await clickWhileFrozen(page, `${backend}-${transportMode}-${arm}-handoff-click`, 400);
            await page.getByRole("button", { name: "Play animation", exact: true }).click();
          }
          if (["incremental", "causal", "held"].includes(arm)) {
            const paused = await untilSource(page, s => s.attached && s.metrics?.ready && s.metrics.presentedFrames > 0 && !s.metrics.needsPresent);
            assert.equal(paused.state.time, 0); assert.equal(paused.state.playing, false);
            assert.equal(paused.state.sourcePlaybackControllable, true);
            assert.deepEqual(paused.events, ["start"]);
            if (arm === "causal" || arm === "held") {
              entry.pausedClick = await clickWhileFrozen(page, `${backend}-${transportMode}-${arm}-paused-click`, 400);
            } else await page.waitForTimeout(220);
            const idle = await observeSource(page);
            assert.equal(idle.state.time, 0); assert.deepEqual(idle.samples, paused.samples);
            await page.getByRole("button", { name: "Play animation", exact: true }).click();
            await untilSource(page, s => s.state.playing && s.state.time >= 0.15);
            await page.getByRole("button", { name: "Pause animation", exact: true }).click();
            const mid = await untilSource(page, s => !s.state.playing && !s.metrics.needsPresent && s.metrics.bufferedDeltas === 0);
            assert.equal(mid.outcome, null);
            await page.waitForTimeout(220);
            const frozen = await observeSource(page);
            assert.equal(frozen.state.time, mid.state.time); assert.deepEqual(frozen.samples, mid.samples);
            assert.deepEqual(frozen.events, mid.events);
            assert.equal(frozen.metrics.presentedFrames, mid.metrics.presentedFrames, "paused source should settle without new pictures");
            entry.pause = { initial: paused, interaction: idle, mid, frozen };
            await page.getByRole("button", { name: "Play animation", exact: true }).click();
            if (arm === "held") {
              const rect = await page.locator("#scene").boundingBox();
              await page.mouse.move(rect.x + 600, rect.y + 200);
              await page.mouse.down();
              await page.mouse.move(rect.x + 625, rect.y + 200);
              entry.heldContact = await observeSource(page);
              assert.equal(entry.heldContact.outcome, null, "gesture starts before source completion");
              process.stdout.write(`held ${backend}/${transportMode} at ${entry.heldContact.state.time}\n`);
            }
          }
          // Async predicates must be awaited here, not passed as truthy Promises
          // to Playwright's waitForFunction.
          const deadline = Date.now() + 180_000;
          let observed;
          while (Date.now() < deadline) {
            observed = await observeSource(page);
            assert.deepEqual(observed.errors, []);
            assert.notEqual(observed.outcome?.status, "failed", JSON.stringify(observed.outcome));
            if (entry.observations.at(-1)?.events.length !== observed.events.length) entry.observations.push(observed);
            if (observed.outcome && observed.metrics?.ready && observed.metrics.presentedFrames > 0 &&
                !observed.metrics.needsPresent && !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0) break;
            await page.waitForTimeout(25);
          }
          assert.equal(observed?.outcome?.status, "finished", encode(observed));
          assert.ok(observed.metrics.ready && observed.metrics.presentedFrames > 0 &&
            !observed.metrics.needsPresent && !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0,
          "final picture must settle before the deadline");
          entry.final = observed;
          assert.equal(observed.metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
          assert.equal(observed.metrics.transportMode, transportMode);
          entry.gpu = classifyRendererGpuIdentity(observed.metrics.rendererGpuIdentity, backend);
          if (!["reference", "causal-reference", "held-reference"].includes(arm)) {
            assert.equal(observed.registrations, 1);
            assert.deepEqual(observed.events, [...(arm.startsWith("late-") ? ["prefix"] : []),
              ...(["causal", "held", "late-causal"].includes(arm) ? ["start", "end", "cleanup"] : ["start", "first", "end", "cleanup"])]);
            assert.equal(observed.state.sourceExecution, "observed");
            assert.equal(observed.state.replaySupported, false);
            assert.notEqual(observed.state.preparedPlayback, true);
            assert.equal(observed.state.time, arm.startsWith("late-") ? 3 : 2);
            assert.equal(observed.state.sourcePlaybackControllable, false);
            if (!["causal", "held", "late-causal"].includes(arm)) assert.ok(Math.abs(observed.samples.reduce((a, b) => a + b, 0) - 2) < 1e-9);
          }
          if (arm === "causal" || arm === "held" || arm === "late-causal") {
            const rect = await page.locator("#scene").boundingBox();
            if (arm !== "held") {
              await page.mouse.move(rect.x + 600, rect.y + 200);
              await page.mouse.down();
            }
            await page.mouse.move(rect.x + 650, rect.y + 200);
            await page.mouse.up();
            await untilSource(page, s => !s.metrics.needsPresent && s.metrics.bufferedDeltas === 0);
            entry.completedClick = await clickWhileFrozen(page, `${backend}-${transportMode}-${arm}-completed-click`, arm === "held" ? 400 : 450);
            entry.afterInput = await observeSource(page);
            assert.equal(entry.afterInput.state.time, arm === "late-causal" ? 3 : 2);
            assert.deepEqual(entry.afterInput.events, [...(arm === "late-causal" ? ["prefix"] : []), "start", "end", "cleanup"]);
            assert.equal(entry.afterInput.metrics.gpuGeneration, observed.metrics.gpuGeneration);
            assert.equal(entry.afterInput.metrics.rendererRebuilds, observed.metrics.rendererRebuilds);
            assert.equal(entry.afterInput.metrics.presentedSession, observed.metrics.presentedSession);
          }
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          const bytes = await page.locator("#scene").screenshot();
          const image = PNG.sync.read(bytes);
          entry.file = `${backend}-${transportMode}-${arm}.png`;
          await writeFile(path.join(output, entry.file), bytes);
          entry.rgbaSha256 = hash(image.data);
          let bluePixels = 0;
          for (let i = 0; i < image.data.length; i += 4) if (image.data[i + 2] > image.data[i] + 30) bluePixels++;
          assert.ok(bluePixels > 100, "blank output cannot prove callback rendering");
          pictures.set(arm, image);
          assert.deepEqual(pageErrors, []);
          entry.status = "passed";
          process.stdout.write(`passed ${backend}/${transportMode}/${arm}\n`);
        } catch (error) { entry.status = "failed"; entry.error = String(error.stack ?? error); throw error; }
        finally {
          clearTimeout(watchdog);
          await page.evaluate(() => { observedAuthoring.execution.terminate(); observedAuthoring.authoring.terminate(); }).catch(() => {});
          await page.close();
          await writeFile(path.join(output, "report.json"), encode(report));
        }
      }
      for (const [actual, reference] of [["ordinary", "reference"], ["incremental", "reference"], ["causal", "causal-reference"], ["held", "held-reference"], ["late-ordinary", "reference"], ["late-incremental", "reference"], ["late-causal", "causal-reference"]]) {
        const image = pictures.get(actual), expected = pictures.get(reference);
        assert.equal(image.width, expected.width); assert.equal(image.height, expected.height);
        assert.ok(image.data.equals(expected.data), `${backend}/${transportMode}/${actual} final pixels must equal the static reference`);
      }
      const entry = { backend, transportMode, arm: "original-always-redraw-gallery", status: "running" };
      report.cases.push(entry);
      const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
      page.setDefaultTimeout(180_000); await cache.install(page.context());
      const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
      try {
        await page.addInitScript(() => {
          globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ = { authoringRunOptions: () => ({ preparedPlayback: true }) };
        });
        const galleryServer = transportMode === "shared" ? server : unisolatedServer;
        await page.goto(`${galleryServer.baseUrl}/web/index.html?example=showcase-always-redraw&catalog=showcase`);
        const deadline = Date.now() + 180_000;
        let observed;
        while (Date.now() < deadline) {
          observed = await page.evaluate(async () => {
            const gallery = globalThis.__noonExampleGallery;
            let metrics;
            try { metrics = (await gallery?.executionMetrics())?.metrics; } catch {}
            return { metrics, inFlight: gallery?.runInFlight, run: gallery?.runPhaseMetrics,
              selected: gallery?.selectedExampleId, controls: { ...document.querySelector(".playback-controls")?.dataset },
              status: document.querySelector("#patch-status")?.dataset.state, problem: document.querySelector("#authoring-problem")?.textContent };
          });
          assert.notEqual(observed.status, "error", observed.problem);
          if (observed.run?.runGeneration && !entry.generation) entry.generation = observed.run.runGeneration;
          if (!entry.pause && observed.inFlight && observed.metrics?.time >= 1.3) {
            await page.getByRole("button", { name: "Pause animation", exact: true }).click();
            await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "false" &&
              document.querySelector("#status")?.dataset.playbackPhase === "paused");
            const paused = await page.locator(".playback-controls").evaluate(element => ({ ...element.dataset }));
            await page.waitForTimeout(220);
            const frozen = await page.locator(".playback-controls").evaluate(element => ({ ...element.dataset }));
            assert.equal(frozen.elapsedSeconds, paused.elapsedSeconds);
            assert.equal(frozen.playing, "false");
            entry.pause = { paused, frozen };
            await page.getByRole("button", { name: "Play animation", exact: true }).click();
          }
          if (observed.status === "applied" && !observed.inFlight && observed.metrics?.ready &&
              !observed.metrics.needsPresent && !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0) break;
          await page.waitForTimeout(25);
        }
        assert.equal(observed?.status, "applied", encode(observed));
        assert.ok(observed.metrics?.ready && !observed.metrics.needsPresent &&
          !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0,
        "gallery picture must settle before the deadline");
        assert.equal(observed.selected, "showcase-always-redraw");
        assert.ok(entry.pause, "the real gallery must accept pause beyond its initial segment");
        assert.equal(observed.run.runGeneration, entry.generation);
        assert.equal(observed.controls.controllable, "false");
        assert.equal(observed.metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(observed.metrics.transportMode, transportMode);
        assert.deepEqual(pageErrors, []);
        entry.final = observed; entry.file = `${backend}-${transportMode}-gallery.png`;
        await page.locator("#scene").screenshot({ path: path.join(output, entry.file) });
        entry.status = "passed";
        process.stdout.write(`passed ${backend}/${transportMode}/gallery\n`);
      } catch (error) { entry.status = "failed"; entry.error = String(error.stack ?? error); throw error; }
      finally { await page.close(); await writeFile(path.join(output, "report.json"), encode(report)); }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally {
  await browser?.close(); await server?.close(); await unisolatedServer?.close();
  await writeFile(path.join(output, "report.json"), encode(report));
}

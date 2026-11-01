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
def mark(value):
    events.append(value)
    postMessage(JSON.parse(json.dumps({"channel": "observed.qualification", "events": events, "samples": samples})))
class OriginalSource(Scene):
    def construct(self):
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(target)
        identity = target.id
        def update(obj, dt):
            assert dt >= 0
            samples.append(dt)
            obj.shift(dt * RIGHT)
        target.add_updater(update)
        mark("start")
        self.wait(0.5)
        assert abs(target.get_center()[0] - 0.5) < 1e-6
        mark("first")
        self.wait(0.5)
        assert target.id == identity
        assert abs(target.get_center()[0] - 1) < 1e-6
        assert abs(sum(samples) - 1) < 1e-9
        assert len(samples) > 2
        mark("end")
    def tear_down(self):
        mark("cleanup")
`;
const reference = `from noon import *
class FinalPicture(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0).shift(RIGHT))
`;
const report = {
  status: "running", scope: "initial observed admission only; original interpreter, callback order and final pixels; no late handoff, pause, latency or default-cutover claim",
  sourceSha256: hash(source), referenceSha256: hash(reference), cases: [],
  harnessSha256: hash(await readFile(fileURLToPath(import.meta.url))),
  gallerySourceSha256: hash(await readFile(path.join(root, "web/python/examples/showcase_always_redraw.py"))),
};
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
let server, unisolatedServer, browser;

async function start(page, transportMode, preparedPlayback, staticReference) {
  await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  await page.evaluate(async ({ source, transportMode, preparedPlayback }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const host = { errors: [], events: [], samples: [], registrations: 0, outcome: null, attached: false };
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
      });
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
    }).catch(error => { host.outcome = { status: "failed", error: String(error) }; });
  }, { source: staticReference ? reference : source, transportMode, preparedPlayback });
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
      const pictures = [];
      for (const arm of ["ordinary", "incremental", "reference"]) {
        const entry = { backend, transportMode, arm, status: "running", observations: [] };
        report.cases.push(entry);
        const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
        page.setDefaultTimeout(180_000); await cache.install(page.context());
        const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
        try {
          await start(page, transportMode, arm !== "ordinary", arm === "reference");
          // Async predicates must be awaited here, not passed as truthy Promises
          // to Playwright's waitForFunction.
          const deadline = Date.now() + 180_000;
          let observed;
          while (Date.now() < deadline) {
            observed = await page.evaluate(async () => {
              const h = observedAuthoring;
              return { attached: h.attached, outcome: h.outcome, errors: h.errors, events: h.events, samples: h.samples, registrations: h.registrations,
                ...(h.attached ? { state: await h.execution.state(), metrics: (await h.execution.metrics({ includeGpuIdentity: true })).metrics } : {}) };
            });
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
          if (arm !== "reference") {
            assert.equal(observed.registrations, 1);
            assert.deepEqual(observed.events, ["start", "first", "end", "cleanup"]);
            assert.equal(observed.state.sourceExecution, "observed");
            assert.equal(observed.state.replaySupported, false);
            assert.notEqual(observed.state.preparedPlayback, true);
            assert.equal(observed.state.time, 1);
            assert.ok(Math.abs(observed.samples.reduce((a, b) => a + b, 0) - 1) < 1e-9);
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
          pictures.push(image);
          assert.deepEqual(pageErrors, []);
          entry.status = "passed";
        } catch (error) { entry.error = String(error.stack ?? error); throw error; }
        finally {
          await page.evaluate(() => { observedAuthoring.execution.terminate(); observedAuthoring.authoring.terminate(); }).catch(() => {});
          await page.close();
          await writeFile(path.join(output, "report.json"), encode(report));
        }
      }
      for (const image of pictures.slice(0, 2)) {
        assert.equal(image.width, pictures[2].width); assert.equal(image.height, pictures[2].height);
        assert.ok(image.data.equals(pictures[2].data), `${backend}/${transportMode} observed final pixels must equal the static reference`);
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
          if (observed.status === "applied" && !observed.inFlight && observed.metrics?.ready &&
              !observed.metrics.needsPresent && !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0) break;
          await page.waitForTimeout(25);
        }
        assert.equal(observed?.status, "applied", encode(observed));
        assert.ok(observed.metrics?.ready && !observed.metrics.needsPresent &&
          !observed.metrics.resourceBundlePending && observed.metrics.bufferedDeltas === 0,
        "gallery picture must settle before the deadline");
        assert.equal(observed.selected, "showcase-always-redraw");
        assert.equal(observed.run.runGeneration, entry.generation);
        assert.equal(observed.controls.controllable, "false");
        assert.equal(observed.metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(observed.metrics.transportMode, transportMode);
        assert.deepEqual(pageErrors, []);
        entry.final = observed; entry.file = `${backend}-${transportMode}-gallery.png`;
        await page.locator("#scene").screenshot({ path: path.join(output, entry.file) });
        entry.status = "passed";
      } catch (error) { entry.error = String(error.stack ?? error); throw error; }
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

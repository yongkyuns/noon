// Ordinary Python construct through the generated production authoring worker,
// its shared endpoint and the actual render worker. Only user-source counters
// are observed specially; no fixture drives completion, time or prefix delivery.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { installPreparedSourceCounterProbe } from "./prepared-source-counter-probe.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_AUTHORING_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-authoring-worker");
const source = `from noon import *
events = []
def mark(value):
    events.append(value)
class PreparedWorkerScene(Scene):
    def construct(self):
        target = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(target)
        self.on_click(target, Indicate(target, color=RED, run_time=0.8))
        mark("start")
        self.wait(1)
        mark("first")
        self.play(target.animate.shift(2 * RIGHT), run_time=1, rate_func=linear)
        assert abs(target.get_center()[0] - 2) < 1e-6
        mark("moved")
        for i in range(8):
            self.wait(0.25)
            mark(i)
        target.shift(RIGHT)
        assert abs(target.get_center()[0] - 3) < 1e-6
        mark("end")
        assert events == ["start", "first", "moved"] + list(range(8)) + ["end"]
`;
const failureSource = source.slice(0, source.indexOf('        self.play(')) + '        raise RuntimeError("intentional prepared production failure")\n';
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const report = { status: "running", scope: "production Python/render workers with read-only counter probe; no timing or default-cutover claim", sourceSha256: hash(source), cases: [] };
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
const launchOptions = backend => ({
  headless: true, ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : { channel: "chromium" }),
  args: browserArgs(backend, { gpuMode: process.env.NOON_RENDER_GPU_MODE ?? "software" }),
  ...(process.env.NOON_BROWSER_PROXY ? { proxy: { server: process.env.NOON_BROWSER_PROXY, bypass: "127.0.0.1,localhost" } } : {}),
});
let server, browser;

function changedPixels(a, b) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (!a.data.subarray(i, i + 4).equals(b.data.subarray(i, i + 4))) changed++;
  }
  return changed;
}

async function start(page, source, transportMode, cancelBeforeAttach = false, evidence = report) {
  await installPreparedSourceCounterProbe(page.context(), "prepared.qualification", evidence);
  await page.addInitScript(() => {
    globalThis.preparedRenderWorkers = [];
    const NativeWorker = Worker;
    globalThis.Worker = class extends NativeWorker {
      constructor(url, options) {
        super(url, options);
        this.qualificationRenderer = options?.name === "noon-render";
        if (this.qualificationRenderer) preparedRenderWorkers.push(this);
      }
      postMessage(message, transfer) {
        if (this.qualificationRenderer && globalThis.failPreparedReceiverSetup && message.type === "start_engine") {
          globalThis.failPreparedReceiverSetup = false;
          return super.postMessage({ ...message, type: "qualification_recovery_setup_failure" }, transfer);
        }
        return super.postMessage(message, transfer);
      }
    };
  });
  await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  await page.evaluate(async ({ source, transportMode, cancelBeforeAttach }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
    const worker = new Worker("./python-worker.js", { type: "module" });
    const host = { events: [], errors: [], failures: [], registration: null, outcome: null, attached: false };
    globalThis.preparedAuthoring = host;
    // Observe explicit user-program side effects without altering source control.
    worker.addEventListener("message", event => {
      if (event.data?.channel !== "prepared.qualification") return;
      event.stopImmediatePropagation();
      host.events = event.data.events;
    });
    host.authoring = new PythonAuthoringClient(worker);
    const canvas = document.querySelector("#scene");
    canvas.width = 800; canvas.height = 400; canvas.style.width = "800px"; canvas.style.height = "400px";
    host.execution = new AuthoringExecutionClient(canvas, {
      inspectionZoom: true,
      onError: (error, owner) => { host.errors.push(String(error)); host.failures.push({ owner, message: String(error) }); },
      onRecoverableError: error => host.errors.push(String(error)),
    });
    host.run = host.authoring.run(source, {}, {
      preparedPlayback: true, preparationLookaheadSeconds: 0.75,
      async onSemanticContinuation(registration) {
        host.registration = registration;
        if (cancelBeforeAttach) {
          await host.authoring.cancelSemanticContinuation(registration.semanticExecution.contextId, registration.generation, "cancel before prepared attachment");
          return;
        }
        await host.execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: host.authoring, transportMode, initiallyPaused: true,
          loopDurationSeconds: Math.max(1, registration.duration),
        });
        const preview = document.createElement("section");
        preview.id = "prepared-controls";
        document.body.append(preview);
        host.controls = new PlaygroundPlaybackControls(host.execution, preview, {
          onError: error => host.errors.push(String(error)),
        });
        host.controls.observe(await host.execution.state());
        host.attached = true;
      },
    });
    void host.run.then(result => { host.outcome = { status: "finished", result }; }, error => { host.outcome = { status: "failed", error: String(error) }; });
  }, { source, transportMode, cancelBeforeAttach });
}

await mkdir(output, { recursive: true });
try {
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch(launchOptions(backend));
    for (const transportMode of ["transferable", "shared"]) {
      const entry = { backend, transportMode, status: "running", checks: [] }; report.cases.push(entry);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
      page.setDefaultTimeout(180_000); await cache.install(page.context());
      const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
      const state = () => page.evaluate(async () => {
        const state = await preparedAuthoring.execution.state();
        preparedAuthoring.controls?.observe(state);
        return { ...state, events: preparedAuthoring.events, outcome: preparedAuthoring.outcome, errors: preparedAuthoring.errors };
      });
      const metrics = () => page.evaluate(async () => (await preparedAuthoring.execution.metrics()).metrics);
      const capture = async name => {
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${backend}-${transportMode}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const waitState = async predicate => {
        const deadline = Date.now() + 30_000;
        while (Date.now() < deadline) {
          const value = await state();
          if (predicate(value)) return value;
          assert.deepEqual(value.errors, []);
          await page.waitForTimeout(25);
        }
        assert.fail(`prepared state did not settle: ${JSON.stringify(await state())}`);
      };
      try {
        await start(page, source, transportMode, false, entry);
        await page.waitForFunction(() => preparedAuthoring.attached || preparedAuthoring.outcome?.status === "failed");
        assert.equal(await page.evaluate(() => preparedAuthoring.attached), true, JSON.stringify(await page.evaluate(() => preparedAuthoring.outcome)));
        await page.waitForFunction(async () => {
          const { metrics } = await preparedAuthoring.execution.metrics();
          return metrics.ready && metrics.presentedFrames > 0 && !metrics.needsPresent && metrics.bufferedDeltas === 0;
        });
        const initial = await state();
        assert.equal(initial.preparedPlayback, true); assert.equal(initial.playing, false);
        assert.equal(initial.time, 0); assert.equal(initial.preparedFrontier, 1); assert.equal(initial.durationSeconds, null);
        assert.deepEqual(initial.events, ["start"]);
        const m = await metrics();
        assert.equal(m.backend, backend === "webgpu" ? "WebGPU" : "WebGL2"); assert.equal(m.transportMode, transportMode);
        entry.backendIdentity = m.backend;
        const idleFrames = m.presentedFrames;
        await page.waitForTimeout(160);
        assert.equal((await metrics()).presentedFrames, idleFrames);
        assert.deepEqual((await state()).events, ["start"]);
        entry.checks.push("real-production-workers", "initially-paused-lookahead", "satisfied-demand-sleeps");

        const seekInput = page.locator(".playback-prepared-seek");
        assert.equal(await seekInput.isVisible(), true);
        assert.equal(await seekInput.isEnabled(), true);
        assert.equal(await page.locator(".playback-scrubber").isVisible(), false);
        assert.equal(await page.locator(".playback-time").textContent(), "0.00 / — s");
        await seekInput.fill("1");
        await seekInput.press("Tab");
        assert.match(await page.locator(".playback-live-status").textContent(), /before 1 s/);
        const atOpenBoundary = await state();
        assert.equal(atOpenBoundary.time, 0);
        assert.deepEqual(atOpenBoundary.events, ["start"]);
        assert.deepEqual(atOpenBoundary.errors, []);
        entry.checks.push("shared-controls-open-history", "controls-refuse-open-boundary");
        await seekInput.fill("0.5");
        await seekInput.press("Tab");
        await waitState(s => s.preparedFrontier === 2 && s.events.length === 2);
        const baseline = await capture("historical");
        assert.equal((await state()).time, 0.5); assert.deepEqual((await state()).events, ["start", "first"]);
        const rect = await page.locator("#scene").boundingBox();
        const beforeClick = (await metrics()).presentedFrames;
        await page.mouse.click(rect.x + 400, rect.y + 200);
        await page.waitForFunction(async before => (await preparedAuthoring.execution.metrics()).metrics.presentedFrames > before + 1, beforeClick);
        entry.highlightPixels = changedPixels(baseline, await capture("historical-click"));
        assert.ok(entry.highlightPixels > 100, "historical click must visibly animate after the producer moved the target");
        assert.equal((await state()).time, 0.5);
        await page.waitForTimeout(950);
        assert.equal(changedPixels(baseline, await capture("click-restored")), 0);
        entry.checks.push("navigation-resumes-same-source", "historical-click-autonomous-wakes", "click-restores-exact-pixels");
        const recover = async name => {
          const before = await state();
          const result = await page.evaluate(async () => {
            const canvas = document.querySelector("#scene");
            const ready = await preparedAuthoring.execution.restart();
            return { canvasReplaced: canvas !== document.querySelector("#scene"), engine: ready.engine };
          });
          assert.equal(result.canvasReplaced, true);
          assert.equal(result.engine.preparedPlayback, true);
          await page.waitForFunction(async () => {
            const m = (await preparedAuthoring.execution.metrics()).metrics;
            return m.ready && m.presentedFrames > 0 && !m.needsPresent && m.bufferedDeltas === 0;
          });
          const after = await state();
          assert.equal(after.time, before.time); assert.equal(after.preparedFrontier, before.preparedFrontier);
          assert.equal(after.durationSeconds, before.durationSeconds); assert.equal(after.playing, before.playing);
          assert.deepEqual(after.events, before.events);
          assert.equal(changedPixels(baseline, await capture(name)), 0);
          return { before, after, ...result };
        };
        entry.openRecovery = await recover("recovered-open");
        entry.checks.push("normal-client-replaces-open-receiver-without-source-reexecution");

        const rejected = await page.evaluate(async () => {
          const before = await preparedAuthoring.execution.state();
          let failure;
          try { await preparedAuthoring.execution.seek(99); } catch (error) { failure = String(error); }
          return { before, after: await preparedAuthoring.execution.state(), failure };
        });
        assert.ok(rejected.failure); assert.equal(rejected.after.time, rejected.before.time);
        assert.equal(rejected.after.preparedFrontier, rejected.before.preparedFrontier);
        entry.checks.push("unavailable-seek-is-atomic");

        await page.getByRole("button", { name: "Play animation", exact: true }).click();
        await page.waitForFunction(() => preparedAuthoring.outcome !== null, null, { timeout: 30_000 });
        const finished = await state();
        assert.equal(finished.outcome.status, "finished", JSON.stringify(finished.outcome));
        assert.equal(finished.outcome.result.duration, 4); assert.equal(finished.durationSeconds, 4);
        assert.equal(finished.outcome.result.semanticExecution.preparedPlayback, true);
        assert.deepEqual(finished.events, ["start", "first", "moved", 0, 1, 2, 3, 4, 5, 6, 7, "end"]);
        assert.equal(await seekInput.isVisible(), false);
        const slider = page.locator(".playback-scrubber");
        assert.equal(await slider.isVisible(), true);
        assert.equal(await slider.getAttribute("max"), "4");
        await page.getByRole("button", { name: "Pause animation", exact: true }).click();
        await page.waitForFunction(() => preparedAuthoring.controls.element.dataset.busy === "false");
        await slider.fill("4");
        await waitState(s => s.time === 4 && s.playing === false);
        assert.equal((await state()).time, 4);
        assert.ok(changedPixels(baseline, await capture("terminal")) > 100);
        await slider.fill("0.5");
        await waitState(s => s.time === 0.5 && s.playing === false);
        assert.equal(changedPixels(baseline, await capture("rewind")), 0);
        entry.checks.push("controls-terminal-slider-and-rewind");
        entry.completedRecovery = await recover("recovered-completed");
        assert.equal((await state()).time, 0.5);
        await page.evaluate(() => preparedRenderWorkers.at(-1).postMessage({
          channel: "noon.render", protocolVersion: 1, type: "qualification_renderer_failure",
        }));
        await page.waitForFunction(() => preparedAuthoring.failures.some(f => f.owner === "render") &&
          preparedAuthoring.failures.some(f => f.owner === "engine"));
        const failure = await page.evaluate(() => ({ failures: preparedAuthoring.failures,
          errors: preparedAuthoring.errors.splice(0) }));
        assert.ok(failure.errors.length >= 2);
        assert.ok(failure.errors.every(error => /qualification_renderer_failure/.test(error)), JSON.stringify(failure));
        entry.renderFailure = failure;
        entry.failedReceiverRecovery = await recover("recovered-render-error");
        entry.checks.push("real-render-controller-error-retains-reader", "failed-receiver-replaced-without-source-reexecution");
        assert.deepEqual((await state()).errors, []); assert.deepEqual(pageErrors, []);
        entry.checks.push("autonomous-playback-refills", "source-statements-execute-once", "terminal-adoption-keeps-reader", "exact-terminal-and-rewind", "completed-history-recovery-keeps-original-reader");
        entry.status = "passed";
      } catch (error) {
        entry.error = String(error.stack ?? error);
        entry.state = await state().catch(() => null);
        entry.host = await page.evaluate(() => ({ events: preparedAuthoring?.events, outcome: preparedAuthoring?.outcome, errors: preparedAuthoring?.errors })).catch(() => null);
        await page.screenshot({ path: path.join(output, `${backend}-${transportMode}-failure.png`) }).catch(() => {});
        throw error;
      } finally { await page.close(); }
    }
    await browser.close(); browser = null;
  }
  // Failure and bootstrap cancellation use the same actual worker, with no
  // callback/fixture substitute. Backends/transports are covered above.
  browser = await playwright.chromium.launch(launchOptions("webgl"));
  for (const mode of ["source-error", "cancel-bootstrap", "cancel-waiting", "cancel-recovery", "fail-recovery-setup"]) {
    const entry = { mode, status: "running", checks: [] }; report.cases.push(entry);
    const page = await browser.newPage(); page.setDefaultTimeout(180_000); await cache.install(page.context());
    try {
      await start(page, mode === "source-error" ? failureSource : source, "transferable", mode === "cancel-bootstrap", entry);
      if (mode !== "cancel-bootstrap") {
        await page.waitForFunction(() => preparedAuthoring.attached || preparedAuthoring.outcome?.status === "failed");
        assert.equal(await page.evaluate(() => preparedAuthoring.attached), true);
        if (mode === "source-error") await page.evaluate(() => preparedAuthoring.execution.seek(0.5));
        else if (mode === "fail-recovery-setup") {
          const settled = async () => page.waitForFunction(async () => {
            const m = (await preparedAuthoring.execution.metrics()).metrics;
            return m.ready && m.presentedFrames > 0 && !m.needsPresent && m.bufferedDeltas === 0;
          });
          await settled();
          const before = await page.locator("#scene").screenshot();
          const failure = await page.evaluate(async () => {
            globalThis.failPreparedReceiverSetup = true;
            try { await preparedAuthoring.execution.restart(); return null; }
            catch (error) { return String(error); }
          });
          assert.match(failure, /qualification_recovery_setup_failure/);
          assert.equal(await page.evaluate(() => preparedAuthoring.outcome), null);
          await page.evaluate(() => preparedAuthoring.execution.restart());
          await settled();
          const after = await page.locator("#scene").screenshot();
          assert.equal(changedPixels(PNG.sync.read(before), PNG.sync.read(after)), 0);
          await writeFile(path.join(output, "failed-recovery-setup-before.png"), before);
          await writeFile(path.join(output, "failed-recovery-setup-retried.png"), after);
          entry.recoveryFailure = failure;
          entry.checks.push("failed-normal-renderer-setup-retains-reader", "newer-receiver-retry-restores-exact-picture");
          await page.evaluate(() => preparedAuthoring.authoring.cancelSemanticContinuation(
            preparedAuthoring.registration.semanticExecution.contextId, preparedAuthoring.registration.generation, "cancel after receiver retry"));
        } else if (mode === "cancel-recovery") {
          let unblock, requested;
          const gate = new Promise(resolve => { unblock = resolve; });
          const seen = new Promise(resolve => { requested = resolve; });
          await page.route("**/execution-render-worker.js", async route => {
            const response = await route.fetch(); requested(); await gate;
            await route.fulfill({ response }).catch(() => {}); // Termination can cancel this request.
          });
          await page.evaluate(() => {
            preparedAuthoring.recovering = preparedAuthoring.execution.restart().then(
              () => ({ status: "unexpected success" }), error => ({ status: "cancelled", error: String(error) }));
          });
          await seen;
          await page.evaluate(() => preparedAuthoring.execution.terminate()); unblock();
          const recovery = await page.evaluate(() => preparedAuthoring.recovering);
          assert.equal(recovery.status, "cancelled"); entry.recovery = recovery;
          entry.checks.push("termination-before-receiver-attachment-retires-original-source");
        } else await page.evaluate(() => preparedAuthoring.authoring.cancelSemanticContinuation(preparedAuthoring.registration.semanticExecution.contextId, preparedAuthoring.registration.generation, "cancel demand-paused source"));
      }
      await page.waitForFunction(() => preparedAuthoring.outcome !== null);
      const outcome = await page.evaluate(() => preparedAuthoring.outcome);
      assert.equal(outcome.status, "failed");
      assert.match(outcome.error, mode === "source-error" ? /intentional prepared production failure/ : mode === "cancel-recovery" ? /cancel|retired/i : /cancel/i);
      assert.deepEqual(await page.evaluate(() => preparedAuthoring.events), mode === "source-error" ? ["start", "first"] : ["start"]);
      if (mode === "source-error") {
        const status = await page.evaluate(() => preparedAuthoring.execution.state());
        assert.equal(status.durationSeconds, null); assert.equal(status.preparedFrontier, 1);
        await page.evaluate(state => preparedAuthoring.controls.observe(state), status);
        assert.equal(await page.locator(".playback-scrubber").isVisible(), false);
        const seekInput = page.locator(".playback-prepared-seek");
        assert.equal(await seekInput.isEnabled(), true);
        await seekInput.fill("0.25");
        await seekInput.press("Tab");
        await page.waitForFunction(async () => (await preparedAuthoring.execution.state()).time === 0.25);
        entry.checks.push("controls-retain-partial-history-after-source-error");
      }
      const reused = await page.evaluate(async () => {
        const result = await preparedAuthoring.authoring.run("from noon import *\nclass NextSource(Scene):\n    def construct(self):\n        self.add(Circle())\n");
        await preparedAuthoring.authoring.releaseSemanticExecution(result.semanticExecution.contextId);
        return { duration: result.duration, terminated: preparedAuthoring.authoring.terminated };
      });
      assert.equal(reused.duration, 0); assert.equal(reused.terminated, false);
      entry.checks.push("single-terminal-source-outcome", "no-source-reexecution", "fresh-run-reuses-healthy-interpreter");
      if (mode === "source-error") entry.checks.push("partial-history-remains-seekable", "failure-does-not-seal");
      entry.status = "passed";
    } catch (error) { entry.error = String(error.stack ?? error); throw error; }
    finally { await page.close(); }
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); process.exitCode = 1; }
finally {
  report.resourceCache = cache.stats();
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  await browser?.close(); await server?.close();
}
console.log(JSON.stringify(report));

// Real DOM -> Python authoring worker -> shared Rust -> render owner -> pixels.
// No alternate zoom formula, scene mirror or host animation clock is used here.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_WORKER_INSPECTION_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/worker-inspection");
const source = `from noon import *
class InspectionFixture(Scene):
    def construct(self):
        circle = Circle(radius=0.4, stroke_width=0, fill_color=BLUE, fill_opacity=1)
        circle.shift(2 * RIGHT)
        self.add(circle)
`;
const animatedSource = `${source}        self.play(Indicate(circle, color=RED), run_time=1)
`;
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const report = { status: "running", sources: { static: hash(source), indicate: hash(animatedSource) }, cases: [] };
let server, browser;
function coloredPixels(image, predicate = (r, g, b) => b > 100 && b > r * 1.4 && g > r * 1.3) {
  let n = 0, x = 0, y = 0;
  for (let row = 0; row < image.height; row++) for (let col = 0; col < image.width; col++) {
    const i = 4 * (row * image.width + col), [r, g, b] = image.data.subarray(i, i + 3);
    if (predicate(r, g, b)) { n++; x += col + 0.5; y += row + 0.5; }
  }
  assert.ok(n > 100, "fixture must contain visible fill pixels");
  return { n, x: x / n, y: y / n };
}
await mkdir(output, { recursive: true });
try {
  assert.match(await readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8"), /scrollInspectionViewJson\(/);
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, channel: "chromium", args: browserArgs(backend) });
    for (const transportMode of ["transferable", "shared"]) for (const mode of ["static", "indicate"]) {
      const entry = { backend, transportMode, mode, status: "running", checks: [] }; report.cases.push(entry);
      const page = await browser.newPage({ viewport: { width: 1000, height: 650 }, deviceScaleFactor: 1 });
      page.setDefaultTimeout(90_000);
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      const image = async label => {
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${backend}-${transportMode}-${mode}-${label}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const metrics = () => page.evaluate(async () => (await workerInspection.execution.metrics()).metrics);
      const settled = async before => {
        await page.waitForFunction(async before => {
          if (workerInspection.errors.length) throw new Error(workerInspection.errors.join("; "));
          const { metrics: m } = await workerInspection.execution.metrics();
          return m.ready && m.retained && m.presentedFrames > (before ?? 0) && !m.needsPresent && m.bufferedDeltas === 0;
        }, before);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(errors, []); assert.deepEqual(await page.evaluate(() => workerInspection.errors), []);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async ({ source, transportMode, mode }) => {
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
          const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
          const canvas = document.querySelector("#scene");
          canvas.width = 800; canvas.height = 400; canvas.style.width = "800px"; canvas.style.height = "400px";
          const errors = [], pending = [], outcomes = [];
          const authoring = new PythonAuthoringClient();
          const execution = new AuthoringExecutionClient(canvas, { inspectionZoom: true,
            onError: error => errors.push(String(error)), onRecoverableError: error => errors.push(String(error)) });
          const original = ExecutionWorkerClient.prototype.scrollInspectionView;
          ExecutionWorkerClient.prototype.scrollInspectionView = function (...args) {
            const result = original.apply(this, args); pending.push(result);
            void result.then(value => outcomes.push(value.inspectionScrollChanged)).catch(() => {});
            return result;
          };
          // Delay only the actual control-port packet, after collection-time
          // snapshotting. The normal engine and renderer still decide acceptance.
          const post = MessagePort.prototype.postMessage;
          const delayed = { enabled: false, packets: [] };
          MessagePort.prototype.postMessage = function (...args) {
            if (delayed.enabled && args[0]?.channel === "noon.engine" && args[0]?.type === "inspection_scroll") {
              delayed.packets.push({ port: this, args }); return;
            }
            return post.apply(this, args);
          };
          delayed.flush = () => { delayed.enabled = false; for (const {port, args} of delayed.packets.splice(0)) post.apply(port, args); };
          const wheel = delta => {
            const rect = canvas.getBoundingClientRect();
            const event = new WheelEvent("wheel", { cancelable: true, deltaMode: 0, deltaY: delta,
              clientX: rect.left + 450, clientY: rect.top + 200 });
            canvas.dispatchEvent(event); return event.defaultPrevented;
          };
          window.workerInspection = { authoring, execution, errors, pending, outcomes, delayed, wheel };
          if (mode === "indicate") {
            // The normal continuation owner samples authored time explicitly.
            // No wall-time wait or fabricated renderer receipt drives this test.
            let resolveAttached, rejectAttached;
            const attached = new Promise((resolve, reject) => { resolveAttached = resolve; rejectAttached = reject; });
            const run = authoring.run(source, {}, {
              async onSemanticContinuation(registration) {
                await execution.startSemanticExecution(registration.semanticExecution, {
                  authoringClient: authoring, transportMode, pacing: "external_samples",
                  loopDurationSeconds: registration.duration,
                });
                resolveAttached();
              },
            });
            void run.catch(rejectAttached);
            workerInspection.run = run;
            await attached;
            await execution.sampleToAuthoredTime(0);
          } else {
            const authored = await authoring.run(source);
            if (authored.duration !== 0 || authored.semanticExecution?.continuationGeneration != null) {
              throw new Error("inspection fixture must return a static shared context");
            }
            await execution.startSemanticExecution(authored.semanticExecution, { authoringClient: authoring, initiallyPaused: true, transportMode });
            await execution.advanceTo(0);
          }
          await execution.setPointerFillSelection(4);
        }, { source: mode === "indicate" ? animatedSource : source, transportMode, mode });
        await settled();
        assert.equal((await metrics()).backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal((await metrics()).transportMode, transportMode);
        const authored = await page.evaluate(() => workerInspection.execution.debugFrame());
        const baseline = await image("baseline"), b = coloredPixels(baseline);
        if (mode === "indicate") {
          const startState = await page.evaluate(() => workerInspection.execution.state());
          const midpointSample = await page.evaluate(() => workerInspection.execution.sampleToAuthoredTime(0.5));
          assert.equal(midpointSample.time, 0.5);
          await settled();
          const midpointFrame = await page.evaluate(() => workerInspection.execution.debugFrame());
          const midpoint = await image("midpoint");
          const bright = (r, g, b) => Math.max(r, g, b) > 100;
          const m = coloredPixels(midpoint, bright);
          assert.ok(Math.abs(m.n / b.n - 1.44) < 0.12, "real Indicate must enlarge the target at its midpoint");
          // RED keeps the yellow selection overlay observable at the midpoint.
          const centerPixel = 4 * (Math.floor(m.y) * midpoint.width + Math.floor(m.x));
          assert.ok(midpoint.data[centerPixel] > midpoint.data[centerPixel + 2] &&
            midpoint.data[centerPixel] > midpoint.data[centerPixel + 1], "real Indicate must tint the target at its midpoint");
          const before = (await metrics()).presentedFrames;
          assert.equal(await page.evaluate(() => workerInspection.wheel(-500 * Math.log(2))), true);
          await page.evaluate(() => Promise.all(workerInspection.pending)); await settled(before);
          const zoomed = await image("midpoint-zoomed"), z = coloredPixels(zoomed, bright);
          assert.ok(Math.abs(z.n / m.n - 4) < 0.15, "inspection zoom must quadruple the active indication's area");
          assert.ok(Math.abs(z.x - (450 + 2 * (m.x - 450))) < 0.75 &&
            Math.abs(z.y - (200 + 2 * (m.y - 200))) < 0.75, "active indication uses cursor-anchored inspection view");
          assert.deepEqual(await page.evaluate(() => workerInspection.execution.debugFrame()), midpointFrame,
            "view-only input cannot advance the active animation or mutate its effective state");
          assert.equal((await page.evaluate(() => workerInspection.execution.state())).time, 0.5);
          const rect = await page.locator("#scene").boundingBox();
          let presented = (await metrics()).presentedFrames;
          await page.mouse.click(rect.x + z.x, rect.y + z.y); await settled(presented);
          entry.selectionPixels = assertSelectionPixels(zoomed, await image("midpoint-picked"), {
            kind: "circle", radius: 0.4, x: 2, y: 0, scaleX: 1.2, scaleY: 1.2, rotation: 0,
          }, { width: 800, height: 400, cameraHeight: 4, centerX: 0.5, centerY: 0 });
          presented = (await metrics()).presentedFrames;
          await page.evaluate(() => workerInspection.execution.setPointerFillSelection(null)); await settled(presented);
          assert.deepEqual((await image("midpoint-cleared")).data, zoomed.data);
          const completed = await page.evaluate(async () => {
            const h = workerInspection;
            const final = await h.execution.sampleToAuthoredTime(1, { stopAtSourceCompletion: true });
            const authored = await h.run;
            await h.execution.reconcileSemanticExecution({
              contextId: authored.semanticExecution.contextId,
              callbackSessionId: authored.semanticExecution.callbackSessionId ?? null,
              continuationGeneration: null,
            }, { authoringClient: h.authoring, loopDurationSeconds: authored.duration });
            await h.execution.pause();
            return { final, duration: authored.duration };
          });
          assert.equal(completed.final.sourceCompleted, true);
          assert.equal(completed.final.time, 1); assert.equal(completed.duration, 1);
          await settled();
          const restored = coloredPixels(await image("restored-at-retained-zoom"));
          assert.ok(Math.abs(restored.n / b.n - 4) < 0.15 &&
            Math.abs(restored.x - (450 + 2 * (b.x - 450))) < 0.75 && Math.abs(restored.y - b.y) < 0.75,
            "indication completion must restore blue geometry without resetting inspection zoom");
          presented = (await metrics()).presentedFrames;
          assert.equal(await page.evaluate(() => workerInspection.wheel(500 * Math.log(2))), true);
          await page.evaluate(() => Promise.all(workerInspection.pending)); await settled(presented);
          assert.deepEqual((await image("restored-at-original-view")).data, baseline.data,
            "normal indication completion and inverse zoom must recover the exact original image");
          const idleFrame = await page.evaluate(() => workerInspection.execution.debugFrame());
          const idlePresented = (await metrics()).presentedFrames;
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          assert.deepEqual(await page.evaluate(() => workerInspection.execution.debugFrame()), idleFrame);
          assert.equal((await metrics()).presentedFrames, idlePresented, "completed inspection must settle without polling frames");
          assert.deepEqual(await page.evaluate(() => workerInspection.outcomes), [true, true]);
          entry.checks.push("real-indicate-midpoint", "zoom-during-active-continuation", "no-authored-time-advance",
            "pick-through-active-composed-view", "exact-overlay-clear", "normal-completion-retains-zoom", "exact-final-restoration", "idle-settles");
          entry.pixels = { baseline: b, midpoint: m, zoomed: z, restored };
          entry.startState = { time: startState.time, playing: startState.playing };
          entry.status = "passed";
          continue;
        }
        let before = (await metrics()).presentedFrames;
        assert.equal(await page.evaluate(() => workerInspection.wheel(-500 * Math.log(2))), true);
        await page.evaluate(() => Promise.all(workerInspection.pending)); await settled(before);
        const zoomed = await image("zoomed"), z = coloredPixels(zoomed);
        assert.ok(Math.abs(z.n / b.n - 4) < 0.15, "zoom must quadruple circle area");
        assert.ok(Math.abs(z.x - (450 + 2 * (b.x - 450))) < 0.75, "cursor anchored x projection");
        assert.ok(Math.abs(z.y - (200 + 2 * (b.y - 200))) < 0.75, "cursor anchored y projection");
        assert.deepEqual(await page.evaluate(() => workerInspection.execution.debugFrame()), authored);
        assert.equal((await page.evaluate(() => workerInspection.execution.state())).playing, false);
        const rect = await page.locator("#scene").boundingBox();
        before = (await metrics()).presentedFrames;
        await page.mouse.click(rect.x + z.x, rect.y + z.y); await settled(before);
        const selected = await image("picked");
        entry.selectionPixels = assertSelectionPixels(zoomed, selected, {
          kind: "circle", radius: 0.4, x: 2, y: 0, scaleX: 1, scaleY: 1, rotation: 0,
        }, { width: 800, height: 400, cameraHeight: 4, centerX: 0.5, centerY: 0 });
        // Captured wheel A waits while the real selection clear presents B.
        await page.evaluate(() => { workerInspection.delayed.enabled = true; workerInspection.wheel(-100); });
        await page.waitForFunction(() => workerInspection.delayed.packets.length === 1);
        before = (await metrics()).presentedFrames;
        await page.evaluate(() => workerInspection.execution.setPointerFillSelection(null)); await settled(before);
        assert.deepEqual((await image("clear-before-delayed")).data, zoomed.data);
        await page.evaluate(async () => { workerInspection.delayed.flush(); await Promise.all(workerInspection.pending); });
        await settled();
        assert.equal(await page.evaluate(() => workerInspection.outcomes.at(-1)), null, "old wheel cannot be relabelled to clear-frame B");
        assert.deepEqual((await image("delayed-rejected")).data, zoomed.data);
        before = (await metrics()).presentedFrames;
        assert.equal(await page.evaluate(() => workerInspection.wheel(500 * Math.log(2))), true);
        await page.evaluate(() => Promise.all(workerInspection.pending)); await settled(before);
        const restored = coloredPixels(await image("restored"));
        assert.ok(Math.abs(restored.n / b.n - 1) < 0.02 && Math.abs(restored.x - b.x) < 0.5 && Math.abs(restored.y - b.y) < 0.5);
        before = (await metrics()).presentedFrames;
        const burst = await page.evaluate(() => Array.from({length: 32}, () => workerInspection.wheel(-10)));
        assert.deepEqual(burst, [true, ...Array(31).fill(false)]);
        await page.evaluate(() => Promise.all(workerInspection.pending)); await settled(before);
        assert.deepEqual(await page.evaluate(() => workerInspection.outcomes), [true, null, true, true]);
        assert.deepEqual(await page.evaluate(() => workerInspection.execution.debugFrame()), authored);
        entry.checks.push("anchored-pixel-zoom", "pick-through-composed-view", "exact-overlay-clear", "delayed-wheel-rejected", "reverse-zoom", "one-in-flight-burst", "authored-time-and-frame-unchanged");
        entry.pixels = { baseline: b, zoomed: z, restored }; entry.status = "passed";
      } catch (error) {
        entry.status = "failed"; entry.error = String(error.stack ?? error);
        await page.screenshot({ path: path.join(output, `${backend}-${transportMode}-${mode}-failure.png`) }).catch(() => {});
        throw error;
      } finally { await page.close(); }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally { await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2)); await browser?.close(); await server?.close(); }

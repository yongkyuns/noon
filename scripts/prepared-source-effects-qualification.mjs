// Unmodified production worker: opaque source effects stay behind real waits.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_SOURCE_EFFECTS_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-observed-source/effects");
await mkdir(output, { recursive: true });
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const encode = value => JSON.stringify(value, (_, field) => typeof field === "bigint" ? field.toString() : field, 2);
const message = 'JSON.parse(\'{"channel":"source.effects.qualification","effect":"after-wait"}\')';
const direct = `from noon import *
from js import JSON, postMessage
class Opaque(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0))
        self.wait(1)
        postMessage(${message})
        self.wait(1)
`;
const helper = direct.replace("from js import JSON, postMessage", `from js import JSON, postMessage as sink
def emit():
    sink(${message})`).replace(`        postMessage(${message})`, "        emit()");
const poison = `from noon import *
from js import JSON, postMessage
original = Circle.shift
def traced_shift(self, *args, **kwargs):
    postMessage(${message})
    return original(self, *args, **kwargs)
Circle.shift = traced_shift
class Seed(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0))
`;
const closed = `from noon import *
class Closed(Scene):
    def construct(self):
        circle = Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0)
        self.add(circle)
        self.wait(1)
        circle.shift(RIGHT)
        self.wait(1)
`;
const namespaceEscape = `from noon import *
saved = __builtins__
__builtins__ = saved
def emit():
    module = saved["__import__"]("js")
    json_object = saved["getattr"](module, "JSON")
    parse = saved["getattr"](json_object, "parse")
    send = saved["getattr"](module, "postMessage")
    send(parse('{"channel":"source.effects.qualification","effect":"after-wait"}'))
class IntrinsicSource(Scene):
    def construct(self):
        self.add(Circle(radius=0.6, color=BLUE, fill_opacity=1, stroke_width=0))
        self.wait(1)
        emit()
        self.wait(1)
`;
const worker = await readFile(path.join(root, "web/python-worker.js"));
const report = { status: "running", scope: "unmodified generated authoring/render workers; opaque effects, alias/helper dispatch and tainted-library admission; real pause/resume and original completion; no startup/performance or default-readiness claim",
  workerSha256: hash(worker), wasmSha256: hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm"))),
  runtimeIdentity: JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8")),
  sources: Object.fromEntries(Object.entries({ direct, helper, poison, closed, namespaceEscape }).map(([key, value]) => [key, { sha256: hash(value), source: value }])), cases: [] };
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
let server, browser;
async function observe(page) {
  return page.evaluate(async () => {
    const h = sourceEffects;
    return { outcome: h.outcome, effects: h.effects, errors: h.errors, registrations: h.registrations,
      state: h.attached ? await h.execution.state() : null,
      metrics: h.attached ? (await h.execution.metrics()).metrics : null };
  });
}
async function until(page, predicate) {
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const value = await observe(page);
    assert.deepEqual(value.errors, []); assert.notEqual(value.outcome?.status, "failed", encode(value));
    if (predicate(value)) return value;
    await page.waitForTimeout(25);
  }
  assert.fail(`source effects deadline: ${encode(await observe(page))}`);
}
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await chromium.launch({ headless: true,
      ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : { channel: "chromium" }),
      args: browserArgs(backend, { gpuMode: process.env.NOON_RENDER_GPU_MODE ?? "software" }) });
    report.browserVersion = browser.version();
    for (const transportMode of ["transferable", "shared"]) for (const scenario of ["direct", "helper", "tainted", "namespace-escape"]) {
      const pictures = [];
      for (const preparedPlayback of [false, true]) {
        const entry = { backend, transportMode, scenario, preparedPlayback, status: "running" }; report.cases.push(entry);
        const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
        page.setDefaultTimeout(180_000); await cache.install(page.context());
        const pageErrors = []; page.on("pageerror", error => pageErrors.push(String(error)));
        try {
          await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
          await page.evaluate(async ({ transportMode, preparedPlayback }) => {
            const { PythonAuthoringClient } = await import("./authoring-client.js");
            const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
            const h = { effects: [], errors: [], attached: false, outcome: null, registrations: 0 };
            globalThis.sourceEffects = h;
            const worker = new Worker("./python-worker.js", { type: "module" });
            worker.addEventListener("message", event => {
              if (event.data?.channel !== "source.effects.qualification") return;
              event.stopImmediatePropagation(); h.effects.push(event.data.effect);
            });
            h.authoring = new PythonAuthoringClient(worker);
            h.start = source => {
              h.execution?.terminate();
              const canvas = document.createElement("canvas"); canvas.id = "scene"; canvas.width = 800; canvas.height = 400;
              document.querySelector("#scene").replaceWith(canvas);
              h.execution = new AuthoringExecutionClient(canvas, {
                onError: error => h.errors.push(String(error)), onRecoverableError: error => h.errors.push(String(error)),
              });
              h.effects = []; h.attached = false; h.outcome = null; h.registrations = 0;
              const attach = async (descriptor, duration) => {
                await h.execution.startSemanticExecution(descriptor, { authoringClient: h.authoring, transportMode,
                  loopDurationSeconds: Math.max(1, duration), initiallyPaused: true });
                h.attached = true;
              };
              h.run = h.authoring.run(source, {}, { preparedPlayback,
                onSemanticContinuation: async registration => { h.registrations++; await attach(registration.semanticExecution, registration.duration); } });
              void h.run.then(async result => {
                if (h.registrations === 0) await attach(result.semanticExecution, result.duration);
                h.outcome = { status: "finished", duration: result.duration };
              }).catch(error => { h.outcome = { status: "failed", error: String(error) }; });
            };
          }, { transportMode, preparedPlayback });
          if (scenario === "tainted") {
            await page.evaluate(source => sourceEffects.start(source), poison);
            entry.seed = await until(page, value => value.outcome?.status === "finished" && value.metrics?.ready && !value.metrics.needsPresent);
          }
          await page.evaluate(source => sourceEffects.start(source), scenario === "tainted" ? closed : scenario === "direct" ? direct : scenario === "helper" ? helper : namespaceEscape);
          entry.initial = await until(page, value => value.state && value.metrics?.ready && value.metrics.presentedFrames > 0 && !value.metrics.needsPresent);
          assert.equal(entry.initial.state.sourceExecution, "observed");
          assert.notEqual(entry.initial.state.preparedPlayback, true);
          assert.equal(entry.initial.state.replaySupported, false);
          assert.equal(entry.initial.state.time, 0); assert.equal(entry.initial.state.playing, false);
          assert.equal(entry.initial.registrations, 1);
          const baselineEffects = entry.initial.effects.length;
          await page.evaluate(() => sourceEffects.execution.resume());
          await until(page, value => value.state.playing && value.state.time >= 0.2);
          await page.evaluate(() => sourceEffects.execution.pause());
          entry.paused = await until(page, value => !value.state.playing && !value.metrics.needsPresent && value.metrics.bufferedDeltas === 0);
          await page.waitForTimeout(220);
          entry.frozen = await observe(page);
          assert.ok(entry.frozen.state.time > 0 && entry.frozen.state.time < 1);
          assert.equal(entry.frozen.state.time, entry.paused.state.time);
          assert.equal(entry.frozen.effects.length, baselineEffects, "opaque work after wait(1) cannot execute during paused preparation");
          assert.equal(entry.frozen.outcome, null);
          const bytes = await page.locator("#scene").screenshot(), picture = PNG.sync.read(bytes);
          entry.file = `${backend}-${transportMode}-${scenario}-${preparedPlayback}-paused.png`;
          await writeFile(path.join(output, entry.file), bytes); pictures.push(picture);
          await page.evaluate(() => sourceEffects.execution.resume());
          entry.completed = await until(page, value => value.outcome?.status === "finished" && !value.metrics.needsPresent && value.metrics.bufferedDeltas === 0);
          assert.equal(entry.completed.state.time, 2); assert.equal(entry.completed.state.playing, false);
          assert.equal(entry.completed.effects.length, baselineEffects + 1, "external work executes once at its real barrier");
          assert.equal(entry.completed.registrations, 1); assert.deepEqual(pageErrors, []);
          entry.status = "passed"; process.stdout.write(`passed ${backend}/${transportMode}/${scenario}/${preparedPlayback}\n`);
        } catch (error) { entry.status = "failed"; entry.error = String(error.stack ?? error); throw error; }
        finally {
          await page.evaluate(() => { sourceEffects.execution?.terminate(); sourceEffects.authoring?.terminate(); }).catch(() => {});
          await page.close(); await writeFile(path.join(output, "report.json"), encode(report));
        }
      }
      assert.equal(pictures[0].width, pictures[1].width); assert.equal(pictures[0].height, pictures[1].height);
      assert.ok(pictures[0].data.equals(pictures[1].data), "paused observed picture matches the ordinary path byte for byte");
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error.stack ?? error); throw error; }
finally { await browser?.close(); await server?.close(); await writeFile(path.join(output, "report.json"), encode(report)); }

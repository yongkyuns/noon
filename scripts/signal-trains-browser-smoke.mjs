// Product qualification only: neither scene construction nor animation runs in JS.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import pngjs from "pngjs";
import { inspectSignalTrain as inspect, inspectSignalSample, signalTrainTimes as times } from "./signal-trains-pixel-check.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = path.resolve(root, "browser-smoke-artifacts/signal-trains");
const port = Number(process.env.NOON_SIGNAL_TRAINS_PORT ?? 4189);
const base = `http://127.0.0.1:${port}`;
const source = await readFile(path.join(root, "web/python/examples/ordinary_signal_trains.py"), "utf8");
const report = { revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(), runs: [] };
await mkdir(artifacts, { recursive: true });
let serverLog = "";
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root], { stdio: ["ignore", "pipe", "pipe"] });
server.stdout.on("data", (chunk) => { serverLog += chunk; });
server.stderr.on("data", (chunk) => { serverLog += chunk; });
server.on("error", (error) => { serverLog += String(error); });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 90_000);
    })]);
  } finally { clearTimeout(timer); }
}

try {
  let ready = false;
  for (let i = 0; i < 80; i += 1) {
    try { ready = (await fetch(`${base}/web/authoring-errors-smoke.html`)).ok; } catch {}
    if (ready) break;
    await sleep(100);
  }
  assert.ok(ready, `server did not start: ${serverLog}`);
  for (const backend of ["webgpu", "webgl"]) {
    const run = { backend, complete: false, directSamples: [], retainedSeekChecks: 0, pythonSamples: [] };
    report.runs.push(run);
    const browser = await playwright.chromium.launch({ headless: true, args: [
      "--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader",
      ...(backend === "webgpu" ? ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-vulkan=swiftshader", "--disable-vulkan-surface"] : ["--disable-webgpu"]),
    ] });
    try {
      const context = await browser.newContext({ viewport: { width: 700, height: 500 }, deviceScaleFactor: 1 });
      if (backend === "webgl") {
        await context.addInitScript(() => Object.defineProperty(navigator, "gpu", { value: undefined, configurable: true }));
      }
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(`${base}/web/authoring-errors-smoke.html`);
      await bounded(page.evaluate(async () => {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const canvas = document.createElement("canvas");
        canvas.id = "signal-trains";
        canvas.width = 640; canvas.height = 360;
        document.body.replaceChildren(canvas);
        document.body.style.margin = "0";
        const renderer = await wasm.createDirectSignalTrainsRenderer(canvas.transferControlToOffscreen());
        renderer.resize(640, 360);
        async function presentFrame() {
          for (let attempt = 0; attempt < 60; attempt += 1) {
            if (renderer.render()) return;
            await new Promise((resolve) => setTimeout(resolve, 10));
          }
          throw new Error("direct renderer did not present");
        }
        // Creation/resize publishes the initial frame. The direct host must
        // admit it before seekDirect may advance to another publication.
        await presentFrame();
        window.signalTrainsProof = { renderer, canvas, presentFrame };
      }), `${backend} direct initialization`);
      const canvas = page.locator("#signal-trains");
      const direct = [];
      const observations = [];
      for (let i = 0; i < times.length; i += 1) {
        const observation = await page.evaluate(async (time) => {
          const { renderer, presentFrame } = window.signalTrainsProof;
          renderer.seekDirect(time);
          renderer.setCamera(0, 0, 8);
          await presentFrame();
          return { backend: renderer.rendererBackend(), time: renderer.time() };
        }, times[i]);
        assert.equal(observation.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(observation.time, times[i]);
        const image = pngjs.PNG.sync.read(await canvas.screenshot({ path: path.join(artifacts, `${backend}-rust-${i}.png`) }));
        observations.push(inspect(image, i));
        direct.push(image.data);
        run.directSamples.push(times[i]);
      }
      assert.ok(observations[2][0].max > observations[1][0].max, "cyan moves right");
      assert.ok(observations[2][1].min < observations[1][1].min, "yellow moves left");
      assert.deepEqual(direct[0], direct[7], "both rails survive completion");
      for (const index of [7, 2, 0, 4, 1, 5, 3, 7, 0, 2]) {
        await page.evaluate(async (time) => {
          const { renderer, presentFrame } = window.signalTrainsProof;
          renderer.seekDirect(time); renderer.setCamera(0, 0, 8);
          await presentFrame();
        }, times[index]);
        assert.deepEqual(pngjs.PNG.sync.read(await canvas.screenshot()).data, direct[index], `retained seek ${times[index]}`);
        run.retainedSeekChecks += 1;
      }
      await page.evaluate(() => { window.signalTrainsProof.renderer.free(); });

      // The optional Python frontend invokes the same Rust engine via the normal
      // authoring/execution clients. No mock handles or scene-state transport is invented here.
      await bounded(page.evaluate(async (source) => {
        const { PythonAuthoringClient } = await import("./authoring-client.js");
        const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
        const canvas = document.createElement("canvas");
        canvas.id = "signal-trains"; canvas.width = 640; canvas.height = 360;
        document.body.replaceChildren(canvas);
        const client = new PythonAuthoringClient();
        let admit, reject;
        const attached = new Promise((resolve, fail) => { admit = resolve; reject = fail; });
        const state = { canvas, client, error: null, result: null };
        const execution = new AuthoringExecutionClient(canvas, { onError(error, owner) {
          state.error = `${owner}: ${error}`; reject(new Error(state.error));
        } });
        state.execution = execution;
        state.finished = client.run(source, {}, { async onSemanticContinuation(registration) {
          await execution.startSemanticExecution(registration.semanticExecution, {
            authoringClient: client, loopDurationSeconds: registration.duration,
            transportMode: "transferable", pacing: "external_samples",
          });
          admit();
        } }).then((result) => { state.result = result; }, (error) => {
          state.error = String(error); reject(error);
        });
        window.signalTrainsProof = state;
        await attached;
      }, source), `${backend} Python initialization`);
      for (let i = 0; i < times.length; i += 1) {
        const observed = await bounded(page.evaluate(async (time) => {
          const state = window.signalTrainsProof;
          if (state.error) throw new Error(state.error);
          // This acknowledgement already awaits the required presentation.
          // A clean static hold may reuse the earlier rendered publication.
          const sample = await state.execution.sampleToAuthoredTime(time);
          const metrics = await state.execution.metrics();
          if (state.error) throw new Error(state.error);
          return { sample, snapshot: {
            executionMode: metrics.executionMode,
            metrics: { ready: metrics.metrics.ready, retained: metrics.metrics.retained,
              backend: metrics.metrics.backend, time: metrics.metrics.time,
              presentedFrames: metrics.metrics.presentedFrames },
          } };
        }, times[i]), `${backend} Python sample ${times[i]}`);
        run.lastPythonObservation = { requestedTime: times[i], ...observed };
        const timing = inspectSignalSample(observed.sample, observed.snapshot, i, backend);
        const image = pngjs.PNG.sync.read(await canvas.screenshot({ path: path.join(artifacts, `${backend}-python-${i}.png`) }));
        inspect(image, i);
        // Rail paint is 0.25 in Rust and #404040 in Python: allow only that
        // one 8-bit quantization level, never geometric or temporal displacement.
        let maxDifference = 0;
        for (let j = 0; j < image.data.length; j += 1) maxDifference = Math.max(maxDifference, Math.abs(image.data[j] - direct[i][j]));
        assert.ok(maxDifference <= 1, `Rust/Python pixels differ by ${maxDifference} at ${times[i]}`);
        run.pythonSamples.push({ ...timing, maxDifference });
      }
      const duration = await bounded(page.evaluate(async () => {
        const state = window.signalTrainsProof;
        await state.finished;
        if (state.error) throw new Error(state.error);
        state.execution.terminate();
        return state.result.duration;
      }), `${backend} Python completion`);
      assert.ok(Math.abs(duration - 4.2) < 1e-9);
      assert.deepEqual(errors, []);
      Object.assign(run, { complete: true, pythonDuration: duration, observations });
    } finally { await browser.close(); }
  }
} catch (error) {
  report.error = String(error?.stack ?? error);
  throw error;
} finally {
  server.kill("SIGTERM");
  await writeFile(path.join(artifacts, "report.json"), JSON.stringify(report, null, 2));
  await writeFile(path.join(artifacts, "server.log"), serverLog);
}
console.log(JSON.stringify(report, null, 2));

// Product qualification only: neither scene construction nor animation runs in JS.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import pngjs from "pngjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = path.resolve(root, "browser-smoke-artifacts/signal-trains");
const port = Number(process.env.NOON_SIGNAL_TRAINS_PORT ?? 4189);
const base = `http://127.0.0.1:${port}`;
const times = [0, 0.65, 0.9, 1.5, 2, 3.55, 4.15, 4.2];
const expected = [0, 1, 2, 5, 8, 1, 0, 0];
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

// Independent image assertions: count separated bright pulses on each rail.
function inspect(image, index) {
  assert.equal(image.width, 640);
  assert.equal(image.height, 360);
  const columns = [new Set(), new Set()];
  for (let i = 0; i < image.data.length; i += 4) {
    const [r, g, b, a] = image.data.subarray(i, i + 4);
    assert.equal(a, 255);
    const x = (i / 4) % image.width;
    const y = Math.floor(i / 4 / image.width);
    if (g > 128 && b > 128 && r < 64) {
      assert.ok(y < image.height / 2, "cyan must stay on the upper rail");
      columns[0].add(x);
    }
    if (r > 128 && g > 100 && b < 64) {
      assert.ok(y > image.height / 2, "yellow must stay on the lower rail");
      columns[1].add(x);
    }
  }
  return columns.map((occupied, rail) => {
    const xs = [...occupied].sort((a, b) => a - b);
    const count = xs.filter((x, i) => i === 0 || x > xs[i - 1] + 1).length;
    assert.equal(count, expected[index], `rail ${rail} signal count at t=${times[index]}`);
    return { count, min: xs.at(0) ?? null, max: xs.at(-1) ?? null };
  });
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
        window.signalTrainsProof = { renderer, canvas };
      }), `${backend} direct initialization`);
      const canvas = page.locator("#signal-trains");
      const direct = [];
      const observations = [];
      for (let i = 0; i < times.length; i += 1) {
        const observation = await page.evaluate(async (time) => {
          const { renderer } = window.signalTrainsProof;
          renderer.seekDirect(time);
          renderer.setCamera(0, 0, 8);
          let presented = false;
          for (let attempt = 0; attempt < 60 && !presented; attempt += 1) {
            presented = renderer.render();
            if (!presented) await new Promise((resolve) => setTimeout(resolve, 10));
          }
          if (!presented) throw new Error("direct renderer did not present");
          return { backend: renderer.rendererBackend(), time: renderer.time() };
        }, times[i]);
        assert.equal(observation.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(observation.time, times[i]);
        const image = pngjs.PNG.sync.read(await canvas.screenshot({ path: path.join(artifacts, `${backend}-rust-${i}.png`) }));
        observations.push(inspect(image, i));
        direct.push(image.data);
      }
      assert.ok(observations[2][0].max > observations[1][0].max, "cyan moves right");
      assert.ok(observations[2][1].min < observations[1][1].min, "yellow moves left");
      assert.deepEqual(direct[0], direct[7], "both rails survive completion");
      for (const index of [7, 2, 0, 4, 1, 5, 3, 7, 0, 2]) {
        await page.evaluate((time) => {
          const { renderer } = window.signalTrainsProof;
          renderer.seekDirect(time); renderer.setCamera(0, 0, 8);
          if (!renderer.render()) throw new Error("seek frame was not presented");
        }, times[index]);
        assert.deepEqual(pngjs.PNG.sync.read(await canvas.screenshot()).data, direct[index], `retained seek ${times[index]}`);
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
          await state.execution.sampleToAuthoredTime(time);
          for (let attempt = 0; attempt < 100; attempt += 1) {
            if (state.error) throw new Error(state.error);
            const m = await state.execution.metrics();
            if (Math.abs(Number(m.metrics.time) - time) < 1e-6 && m.metrics.ready && m.metrics.retained && m.metrics.presentedFrames > 0) {
              return { backend: m.metrics.backend, time: Number(m.metrics.time), mode: m.executionMode };
            }
            await new Promise((resolve) => setTimeout(resolve, 20));
          }
          throw new Error(`Python renderer did not reach ${time}`);
        }, times[i]), `${backend} Python sample ${times[i]}`);
        assert.equal(observed.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(observed.mode, "semantic");
        const image = pngjs.PNG.sync.read(await canvas.screenshot({ path: path.join(artifacts, `${backend}-python-${i}.png`) }));
        inspect(image, i);
        // Rail paint is 0.25 in Rust and #404040 in Python: allow only that
        // one 8-bit quantization level, never geometric or temporal displacement.
        let maxDifference = 0;
        for (let j = 0; j < image.data.length; j += 1) maxDifference = Math.max(maxDifference, Math.abs(image.data[j] - direct[i][j]));
        assert.ok(maxDifference <= 1, `Rust/Python pixels differ by ${maxDifference} at ${times[i]}`);
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
      report.runs.push({ backend, samples: times, retainedSeekChecks: 10, pythonDuration: duration, observations });
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

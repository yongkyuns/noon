// Canonical shader seconds, using ordinary Rust and Python authoring hosts.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const identity = JSON.parse(await readFile(root + "/web/runtime-build-identity.json", "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "exact clean source/package required");
assert.ok(identity.sourceRevision);
const times = [0, 0.25, 0.75, 1.5, 2];
function reference(image, time) {
  // Full source coverage at the center; straight encoded RGB, intrinsic alpha
  // 0.5 and the declared coverage multiplier are composited once on black.
  const expected = [0.5, 0.75, 1].map(c => Math.round(255 * c * 0.5 * (0.2 + 0.25 * time)));
  const at = 4 * (Math.floor(image.height / 2) * image.width + Math.floor(image.width / 2));
  const actual = [...image.data.subarray(at, at + 4)];
  const maximumError = Math.max(...expected.map((v, i) => Math.abs(v - actual[i])));
  assert.ok(maximumError <= 2, `authored shader time ${time}: ${actual} expected ${expected}`);
  assert.equal(actual[3], 255);
  return { expected, actual, maximumError, runtimeBuild: identity.buildId };
}
await qualifyPairedAuthoring({
  artifactDirectory: "ci-artifacts/shader-time",
  prepareContext: disableAuthoringJspi,
  cases: times.map(sampleTime => ({ id: `shader-time-${sampleTime}`, file: "shader_time.py",
    factory: "createDirectShaderTimeSmokeRenderer", duration: 2, sampleTime,
    playback: "live", objectCount: 1, stopAtSourceCompletion: sampleTime === 2 })),
  qualifyPixels({ fixture, rust, python }) {
    return { rust: reference(rust, fixture.sampleTime), python: reference(python, fixture.sampleTime) };
  },
  async qualifyLifecycle(context, baseUrl, backend) {
    const images = new Map(), trials = [];
    for (const schedule of [times, [0, 0.125, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2]]) {
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      try {
        await page.goto(baseUrl + "/web/manim-raster-host.html");
        const actualBackend = await page.evaluate(async () => {
          const wasm = await import("./pkg/noon_web.js"); await wasm.default();
          const canvas = document.querySelector("#scene");
          globalThis.clockRenderer = await wasm.createDirectShaderTimeSmokeRenderer(canvas.transferControlToOffscreen());
          if (!clockRenderer.render()) throw new Error("initial shader publication did not render");
          clockRenderer.advanceDirectRealtime(0);
          return clockRenderer.rendererBackend();
        });
        assert.equal(actualBackend, backend);
        const trial = { samples: [], replay: [] }; trials.push(trial);
        for (const time of schedule) {
          const state = await page.evaluate(async time => {
            const { sampleDirectProgramThrough } = await import("../scripts/direct-program-sample.mjs");
            return sampleDirectProgramThrough(clockRenderer, time);
          }, time);
          assert.equal(state.time, time);
          if (time < 2) assert.equal(state.cadence, "animation-frame", "declared shader time keeps the wait awake");
          const png = PNG.sync.read(await page.locator("#scene").screenshot());
          const check = reference(png, time);
          if (images.has(time)) assert.deepEqual(png.data, images.get(time), "shader output cannot depend on frame count");
          else images.set(time, Buffer.from(png.data));
          trial.samples.push({ time, state, check });
        }
        for (const time of [0.75, 0, 1.5, 2, 0]) {
          await page.evaluate(time => { clockRenderer.seekDirect(time); clockRenderer.render(); }, time);
          assert.equal(await page.evaluate(() => clockRenderer.time()), time);
          const png = PNG.sync.read(await page.locator("#scene").screenshot());
          assert.deepEqual(png.data, images.get(time), "seek must reproduce the original same-time frame");
          trial.replay.push(time);
        }
        const programs = await page.evaluate(() => JSON.parse(clockRenderer.debugEffectProgramsJson()));
        assert.equal(programs.pipelineCompiles, 1, "cadence and seek reuse the immutable shader program");
        trial.programs = programs;
        assert.deepEqual(errors, []);
      } finally { await page.close(); }
    }
    return { backend, trials };
  },
});

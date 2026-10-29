// Canonical shader seconds, using ordinary Rust and Python authoring hosts.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const sourceMode = process.env.NOON_SHADER_SOURCE_MODE ?? "portable";
assert.ok(["portable", "jspi"].includes(sourceMode));
const identity = JSON.parse(await readFile(root + "/web/runtime-build-identity.json", "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "exact clean source/package required");
assert.ok(identity.sourceRevision);
const times = [0, 0.25, 0.75, 1.5, 2];
function sourceForMode(source) {
  if (sourceMode !== "jspi") return source;
  return source.replace("    def construct(self):\n",
    "    def construct(self):\n        self.shader_lesson()\n\n    def shader_lesson(self):\n")
    .replace("        self.wait(0.5)\n", "        self.wait(0.5)\n        from _manim_scene import _synchronous_continuation_active\n        assert _synchronous_continuation_active(self), 'synchronous source required'\n");
}
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
// The CPU-authored program stays valid. Corrupt only the real device descriptor
// to prove that compilation failure cannot publish the staged semantic batch.
async function qualifyLatePreparation(context, baseUrl, backend) {
  const page = await context.newPage();
  try {
    if (backend === "WebGPU") await page.addInitScript(() => {
      globalThis.rejectLateShader = false;
      globalThis.rejectedLateShaders = 0;
      const original = GPUDevice.prototype.createShaderModule;
      GPUDevice.prototype.createShaderModule = function (descriptor) {
        if (globalThis.rejectLateShader && descriptor.code.includes("let stripes")) {
          globalThis.rejectedLateShaders++;
          descriptor = { ...descriptor, code: descriptor.code + "\ninvalid generated WGSL" };
        }
        return original.call(this, descriptor);
      };
    });
    await page.goto(baseUrl + "/web/manim-raster-host.html");
    assert.equal(await page.evaluate(async () => {
      const wasm = await import("./pkg/noon_web.js"); await wasm.default();
      globalThis.lateRenderer = await wasm.createDirectShaderSweepLateSmokeRenderer(
        document.querySelector("#scene").transferControlToOffscreen());
      if (!lateRenderer.render()) throw new Error("initial scene did not render");
      lateRenderer.advanceDirectRealtime(0);
      return lateRenderer.rendererBackend();
    }), backend);
    const previousPixels = PNG.sync.read(await page.locator("#scene").screenshot()).data;
    const before = await page.evaluate(() => {
      globalThis.rejectLateShader = true;
      lateRenderer.advanceDirectRealtime(500);
      return JSON.parse(lateRenderer.debugSelectionFrameJson());
    });
    let failure;
    if (backend === "WebGPU") {
      failure = await page.evaluate(async () => {
        for (let attempt = 0; attempt < 100; attempt++) {
          try { lateRenderer.render(); }
          catch (error) { return String(error); }
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error("real shader compilation was not rejected");
      });
      assert.match(failure, /shader|WGSL|compil|valid/i);
      assert.equal(await page.evaluate(() => rejectedLateShaders), 1);
      assert.deepEqual(await page.evaluate(() => JSON.parse(lateRenderer.debugSelectionFrameJson())), before,
        "GPU failure must leave the current runtime publication intact");
      assert.deepEqual(PNG.sync.read(await page.locator("#scene").screenshot()).data, previousPixels,
        "the old displayed frame remains live through failed preparation");
    }
    const after = await page.evaluate(async () => {
      globalThis.rejectLateShader = false;
      const { sampleDirectProgramThrough } = await import("../scripts/direct-program-sample.mjs");
      await sampleDirectProgramThrough(lateRenderer, 0.5);
      return { frame: JSON.parse(lateRenderer.debugSelectionFrameJson()),
        programs: JSON.parse(lateRenderer.debugEffectProgramsJson()) };
    });
    assert.equal(before.objects.flatMap(object => object.effects).length, 0);
    assert.equal(after.frame.objects.flatMap(object => object.effects).length, 2);
    assert.equal(after.programs.rows, 2);
    assert.equal(after.programs.pipelineCompiles, 1);
    return { backend, failure, before, after };
  } finally { await page.close(); }
}

async function qualifyProfileTransition(context, baseUrl, backend) {
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  try {
    await page.goto(baseUrl + "/web/manim-raster-host.html");
    await page.waitForFunction(() => globalThis.noonHostRaster);
    const lesson = await readFile(root + "/web/python/examples/showcase_shader_material.py", "utf8");
    const source = sourceForMode(lesson
      .replace("class ShaderMaterial(Scene):", "class ShaderMaterial(MovingCameraScene):")
      .replace("    def construct(self):\n", "    def construct(self):\n        self.camera.frame.set_view_color_profile('linear_hdr')\n")
      .replace('programs = [material("local"), material("scene")]', 'programs = [material("scene")] * 2')
      .replace("        for panel, program in zip(panels, programs):", "        self.camera.frame.set_view_color_profile('compatibility')\n        for panel, program in zip(panels, programs):"));
    const initial = await page.evaluate(source => noonHostRaster.load(source, 6), source);
    assert.equal(initial.rendererBackend, backend);
    const worker = page.workers().find(worker => worker.url().includes("execution-render-worker") && !worker.url().includes("capability-probe"));
    assert.ok(worker);
    if (backend === "WebGPU") await worker.evaluate(() => {
      globalThis.profileShaderModules = 0;
      const original = GPUDevice.prototype.createShaderModule;
      GPUDevice.prototype.createShaderModule = function (descriptor) {
        if (descriptor.code.includes("let stripes")) profileShaderModules++;
        return original.call(this, descriptor);
      };
    });
    await page.evaluate(() => noonHostRaster.renderThrough(2, [0, 0.5, 0.75]));
    const frame = await page.evaluate(() => noonHostRaster.debugFrame());
    assert.equal(frame.objects.flatMap(object => object.effects).length, 2);
    const modules = backend === "WebGPU" ? await worker.evaluate(() => profileShaderModules) : null;
    if (modules !== null) assert.equal(modules, 1, "working-target changes preserve prepared programs");
    const pixels = PNG.sync.read(await page.locator("#scene").screenshot()).data;
    let treatedPixels = 0;
    for (let at = 0; at < pixels.length; at += 4) {
      if (pixels[at + 1] > pixels[at] + 40 && pixels[at + 2] > pixels[at] + 30 && pixels[at + 1] > 100) treatedPixels++;
    }
    assert.ok(treatedPixels > 500, "the retained picture contains the shader treatment");
    const compatibleSource = source.replace("self.camera.frame.set_view_color_profile('linear_hdr')",
      "self.camera.frame.set_view_color_profile('compatibility')");
    await page.close();
    const referencePage = await context.newPage();
    referencePage.on("pageerror", error => errors.push(String(error)));
    try {
      await referencePage.goto(baseUrl + "/web/manim-raster-host.html");
      await referencePage.waitForFunction(() => globalThis.noonHostRaster);
      await referencePage.evaluate(source => noonHostRaster.load(source, 6), compatibleSource);
      await referencePage.evaluate(() => noonHostRaster.renderThrough(2, [0, 0.5, 0.75]));
      assert.deepEqual(pixels, PNG.sync.read(await referencePage.locator("#scene").screenshot()).data,
        "transition output matches the same program authored entirely in compatibility color");
    } finally { await referencePage.close(); }
    assert.deepEqual(errors, []);
    return { backend, sourceMode, modules, treatedPixels, frame };
  } finally { await page.close(); }
}

async function qualifyPythonPreparation(context, baseUrl, backend) {
  if (backend !== "WebGPU") return null;
  const page = await context.newPage();
  try {
    await page.goto(baseUrl + "/web/manim-raster-host.html");
    await page.waitForFunction(() => globalThis.noonHostRaster);
    const lesson = await readFile(root + "/web/python/examples/showcase_shader_material.py", "utf8");
    const original = `        for panel, program in zip(panels, programs):
            panel.add_effect(ShaderEffect(program), name="stripes")`;
    assert.ok(lesson.includes(original));
    let source = lesson.replace("class ShaderMaterial(Scene):", "class ShaderMaterial(MovingCameraScene):")
      .replace('programs = [material("local"), material("scene")]',
      'programs = [material("scene")] * 2').replace(original, `        self.camera.frame.set_view_color_profile("linear_hdr")
        try:
            panels[0].add_effect(ShaderEffect(programs[0]), name="stripes")
        except Exception:
            pass
        else:
            raise AssertionError("HDR shader attachment must reject")
        self.camera.frame.set_view_color_profile("compatibility")
        try:
            panels[0].add_effect(ShaderEffect(programs[0]), name="stripes")
        except Exception:
            try:
                panels[0].get_effect("stripes")
            except Exception:
                pass
            else:
                raise AssertionError("failed GPU preparation published a semantic attachment")
            self.wait(0.25)
            panels[0].add_effect(ShaderEffect(programs[0]), name="stripes")
        panels[1].add_effect(ShaderEffect(programs[1]), name="stripes")
        try:
            self.camera.frame.set_view_color_profile("linear_hdr")
        except Exception:
            pass
        else:
            raise AssertionError("HDR profile edit must reject while shaders are attached")`);
    source = sourceForMode(source);
    const initial = await page.evaluate(source => noonHostRaster.load(source, 6.25), source);
    assert.equal(initial.rendererBackend, backend);
    const previous = PNG.sync.read(await page.locator("#scene").screenshot()).data;
    const author = page.workers().find(worker => worker.url().includes("python-worker"));
    assert.ok(author, "actual Python authoring worker required");
    await author.evaluate(() => {
      globalThis.shaderPreparationBytes = [];
      const original = MessagePort.prototype.postMessage;
      MessagePort.prototype.postMessage = function (message, ...args) {
        if (message?.type === "prepare_effect_programs") shaderPreparationBytes.push(message.bytes.byteLength);
        return original.call(this, message, ...args);
      };
    });
    const worker = page.workers().find(worker => worker.url().includes("execution-render-worker.js") && !worker.url().includes("capability-probe"));
    assert.ok(worker, "actual Python render worker required");
    await worker.evaluate(() => {
      globalThis.rejectShader = true;
      globalThis.rejectedShaders = 0;
      const original = GPUDevice.prototype.createShaderModule;
      GPUDevice.prototype.createShaderModule = function (descriptor) {
        if (rejectShader && descriptor.code.includes("let stripes")) {
          rejectedShaders++;
          descriptor = { ...descriptor, code: descriptor.code + "\ninvalid generated WGSL" };
        }
        return original.call(this, descriptor);
      };
    });
    await page.evaluate(() => noonHostRaster.renderThrough(1, [0, 0.5, 0.75]));
    const rejected = await page.evaluate(() => noonHostRaster.debugFrame());
    assert.equal(await worker.evaluate(() => rejectedShaders), 1);
    assert.equal(rejected.time, 0.5);
    assert.equal(rejected.objects.flatMap(object => object.effects).length, 0);
    assert.deepEqual(PNG.sync.read(await page.locator("#scene").screenshot()).data, previous,
      "failed Python attachment preserves the displayed frame");
    await worker.evaluate(() => { rejectShader = false; });
    await page.evaluate(() => noonHostRaster.renderThrough(2, [0, 0.5, 0.75]));
    const recovered = await page.evaluate(() => noonHostRaster.debugFrame());
    assert.equal(recovered.time, 0.75);
    assert.equal(recovered.objects.flatMap(object => object.effects).length, 2);
    const preparationBytes = await author.evaluate(() => shaderPreparationBytes);
    assert.equal(preparationBytes.length, 3, "failed source, retry and shared-program attachment");
    assert.ok(preparationBytes[2] < preparationBytes[1] / 2, "shared program uses its installed resource handle");
    return { backend, sourceMode, rejected, recovered, rejectedModules: 1, preparationBytes };
  } finally { await page.close(); }
}

await qualifyPairedAuthoring({
  artifactDirectory: "ci-artifacts/shader-time",
  prepareContext: sourceMode === "portable" ? disableAuthoringJspi : undefined,
  cases: (sourceMode === "jspi" ? [0] : times).map(sampleTime => ({ id: `shader-time-${sampleTime}`, file: "shader_time.py",
    factory: "createDirectShaderTimeSmokeRenderer", duration: 2, sampleTime,
    playback: "live", objectCount: 1, stopAtSourceCompletion: sampleTime === 2 })),
  qualifyPixels({ fixture, rust, python }) {
    return { rust: reference(rust, fixture.sampleTime), python: reference(python, fixture.sampleTime) };
  },
  async qualifyLifecycle(context, baseUrl, backend) {
    if (sourceMode === "jspi") return {
      backend, pythonPreparation: await qualifyPythonPreparation(context, baseUrl, backend),
      profileTransition: await qualifyProfileTransition(context, baseUrl, backend),
    };
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
    return { backend, trials,
      latePreparation: await qualifyLatePreparation(context, baseUrl, backend),
      pythonPreparation: await qualifyPythonPreparation(context, baseUrl, backend),
      profileTransition: await qualifyProfileTransition(context, baseUrl, backend) };
  },
});

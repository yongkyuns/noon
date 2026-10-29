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
assert.ok(["portable", "jspi", "async"].includes(sourceMode));
const identity = JSON.parse(await readFile(root + "/web/runtime-build-identity.json", "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "exact clean source/package required");
assert.ok(identity.sourceRevision);
const times = [0, 0.25, 0.75, 1.5, 2];
function sourceForMode(source) {
  if (sourceMode === "async") return source
    .replace("    def construct(self):", "    async def construct(self):")
    .replaceAll("self.wait(", "await self.wait(")
    .replaceAll("self.play(", "await self.play(");
  if (sourceMode !== "jspi") return source;
  return source.replace("    def construct(self):\n",
    "    def construct(self):\n        self.shader_lesson()\n\n    def shader_lesson(self):\n")
    .replace("        self.wait(0.5)\n", "        self.wait(0.5)\n        from _manim_scene import _synchronous_continuation_active\n        assert _synchronous_continuation_active(self), 'synchronous source required'\n");
}
const materialLesson = await readFile(root + "/web/python/examples/showcase_shader_material.py", "utf8");
const materialAttachment = `        for panel, program in zip(panels, programs):
            panel.add_effect(ShaderEffect(program), name="stripes")`;
assert.ok(materialLesson.includes(materialAttachment));
const fadeSource = sourceForMode(materialLesson
  .replace(materialAttachment, `        self.play(*(FadeIn(panel) for panel in panels), run_time=1, rate_func=linear)`)
  .replace('        programs = [material("local"), material("scene")]\n', '')
  .replace("        self.add(*panels)", `        programs = [material("local"), material("scene")]
${materialAttachment}`));

function fadeReference(image, time) {
  if (![0.5, 1, 7].includes(time)) return null;
  const at = 4 * (Math.floor(image.height / 2) * image.width + Math.floor(image.width / 2 + 3 * image.height / 8));
  const sceneY = (image.height / 2 - Math.floor(image.height / 2) - 0.5) * 8 / image.height;
  const stripes = 0.5 + 0.5 * Math.cos(2 * Math.PI * sceneY * 2);
  const expected = [[0.09, 0.48, 0.035], [0.2, 0.94, 0.045], [0.42, 0.83, 0.09]].map(([low, high, background]) =>
    Math.round(255 * (time === 0.5 ? background : time === 7 ? 1 : 0.5 * (low + (high - low) * stripes) + 0.5 * background)));
  const actual = [...image.data.subarray(at, at + 4)];
  const maximumError = Math.max(...expected.map((value, channel) => Math.abs(value - actual[channel])));
  assert.ok(maximumError <= 2, `literal FadeIn at ${time}: ${actual} expected ${expected}`);
  assert.equal(actual[3], 255);
  return { expected, actual, maximumError, runtimeBuild: identity.buildId };
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

async function qualifyPythonPreparation(context, baseUrl, backend, kind = "attachments") {
  if (backend !== "WebGPU" && kind === "attachments") return null;
  const page = await context.newPage();
  try {
    await page.goto(baseUrl + "/web/manim-raster-host.html");
    await page.waitForFunction(() => globalThis.noonHostRaster);
    const lesson = await readFile(root + "/web/python/examples/showcase_shader_material.py", "utf8");
    const original = `        for panel, program in zip(panels, programs):
            panel.add_effect(ShaderEffect(program), name="stripes")`;
    assert.ok(lesson.includes(original));
    const familyMembership = kind === "family" || kind === "object";
    const fade = kind === "fade";
    const addition = index => {
      const call = kind === "scene" ? `self.add(panels[${index}], key="panel-${index}")`
      : familyMembership ? `family.add(panels[${index}])`
      : fade ? `self.play(FadeIn(panels[${index}])${index === 0 ? ", UpdateFromAlphaFunc(anchor, observe_intro)" : ""}, run_time=0.25, rate_func=linear)`
        : `panels[${index}].add_effect(ShaderEffect(programs[${index}]), name="stripes")`;
      return sourceMode === "async" && !fade ? `(await ${call})` : call;
    };
    const rejectedCheck = kind === "scene" || fade ? `            assert panels[0]._scene is None
            assert "panel-0" not in self._object_key_ids`
      : familyMembership ? `            assert len(family.submobjects) == 0`
        : `            try:
                panels[0].get_effect("stripes")
            except Exception:
                pass
            else:
                raise AssertionError("failed GPU preparation published a semantic attachment")`;
    let source = lesson.replace("class ShaderMaterial(Scene):", "class ShaderMaterial(MovingCameraScene):")
      .replace('programs = [material("local"), material("scene")]',
      'programs = [material("scene")] * 2').replace(original, `        self.camera.frame.set_view_color_profile("linear_hdr")
        try:
            ${addition(0)}
        except Exception:
            pass
        else:
            raise AssertionError("HDR shader attachment must reject")
        self.camera.frame.set_view_color_profile("compatibility")
        try:
            ${addition(0)}
        except Exception:
${rejectedCheck}
            self.wait(0.25)
            ${addition(0)}
        bound = ${addition(1)}${fade ? "" : ".set_opacity(1)"}
        assert bound is ${fade ? "self" : familyMembership ? "family" : "panels[1]"}${fade ? '\n        assert observed, "the introducing play must service its ordinary callback"' : ''}
        try:
            self.camera.frame.set_view_color_profile("linear_hdr")
        except Exception:
            pass
        else:
            raise AssertionError("HDR profile edit must reject while shaders are attached")`);
    if (kind !== "attachments") source = source
      .replace("        self.add(*panels)", `        shared = material("scene")
        for panel in panels:
            panel.add_effect(ShaderEffect(shared), name="stripes")${familyMembership ? `\n        family = ${kind === "family" ? "VGroup()" : "Rectangle(width=0.01, height=0.01, fill_opacity=0, stroke=None)"}\n        self.add(family)` : ''}`)
      .replace('programs = [material("scene")] * 2', 'programs = [shared] * 2');
    if (fade) source = source.replace("        self.wait(0.5)", `        anchor = Dot(radius=0.01).set_opacity(0)
        self.add(anchor)
        observed = []
        def observe_intro(_, alpha):
            assert panels[0]._scene is self
            assert abs(panels[0].get_center().x + 3) < 1e-6
            observed.append(alpha)
        self.wait(0.5)`);
    source = sourceForMode(source);
    const checkpoints = fade ? [0, 0.5, 0.75, 1, 1.25, 1.5] : [0, 0.5, 0.75];
    const initial = await page.evaluate(({source, duration}) => noonHostRaster.load(source, duration), {source, duration: fade ? 6.75 : 6.25});
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
    if (backend === "WebGPU") await worker.evaluate(() => {
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
    let rejected = null;
    if (backend === "WebGPU") {
      await page.evaluate(times => noonHostRaster.renderThrough(1, times), checkpoints);
      rejected = await page.evaluate(() => noonHostRaster.debugFrame());
      assert.equal(await worker.evaluate(() => rejectedShaders), 1);
      assert.equal(rejected.time, 0.5);
      assert.equal(rejected.objects.flatMap(object => object.effects).length, 0);
      assert.deepEqual(PNG.sync.read(await page.locator("#scene").screenshot()).data, previous,
        "failed Python publication preserves the displayed frame");
      await worker.evaluate(() => { rejectShader = false; });
    }
    await page.evaluate(times => noonHostRaster.renderThrough(times.length - 1, times), checkpoints);
    const recovered = await page.evaluate(() => noonHostRaster.debugFrame());
    assert.equal(recovered.time, checkpoints.at(-1));
    assert.equal(recovered.objects.flatMap(object => object.effects).length, 2);
    const preparationBytes = await author.evaluate(() => shaderPreparationBytes);
    assert.equal(preparationBytes.length, backend === "WebGPU" ? 3 : 2, "first use, optional failed attempt and shared-program reuse");
    assert.ok(preparationBytes.at(-1) < preparationBytes.at(-2) / 2, "shared program uses its installed resource handle");
    const pixels = PNG.sync.read(await page.locator("#scene").screenshot()).data;
    let treatedPixels = 0;
    for (let at = 0; at < pixels.length; at += 4) {
      if (pixels[at + 1] > pixels[at] + 40 && pixels[at + 2] > pixels[at] + 30 && pixels[at + 1] > 100) treatedPixels++;
    }
    assert.ok(treatedPixels > 500, "published source contains the shader treatment");
    return { backend, sourceMode, kind, rejected, recovered, rejectedModules: backend === "WebGPU" ? 1 : null, preparationBytes, treatedPixels };
  } finally { await page.close(); }
}

await qualifyPairedAuthoring({
  artifactDirectory: "ci-artifacts/shader-time",
  prepareContext: sourceMode !== "jspi" ? disableAuthoringJspi : undefined,
  cases: [...(sourceMode !== "portable" ? [0] : times).map(sampleTime => ({ id: `shader-time-${sampleTime}`, file: "shader_time.py",
    factory: "createDirectShaderTimeSmokeRenderer", duration: 2, sampleTime,
    playback: "live", objectCount: 1, stopAtSourceCompletion: sampleTime === 2 })),
    ...(sourceMode === "portable" ? [0.5, 1, 1.75, 7] : [1, 1.75]).map(sampleTime => ({
      id: `shader-fade-${sampleTime}`, file: "shader_material_fade.py", source: fadeSource,
      factory: "createDirectShaderFadeSmokeRenderer", duration: 7, sampleTime, playback: "live",
      boundaries: [0.5, 1.5, 3.5, 4, 6, 6.5], stopAtSourceCompletion: sampleTime === 7,
    }))],
  qualifyPixels({ fixture, rust, python }) {
    const check = fixture.id.startsWith("shader-fade-") ? fadeReference : reference;
    return { rust: check(rust, fixture.sampleTime), python: check(python, fixture.sampleTime) };
  },
  async qualifyLifecycle(context, baseUrl, backend) {
    if (sourceMode !== "portable") return {
      backend, pythonPreparation: await qualifyPythonPreparation(context, baseUrl, backend),
      pythonMembership: [await qualifyPythonPreparation(context, baseUrl, backend, "scene"),
        await qualifyPythonPreparation(context, baseUrl, backend, "family"),
        await qualifyPythonPreparation(context, baseUrl, backend, "object")],
      pythonFade: await qualifyPythonPreparation(context, baseUrl, backend, "fade"),
      profileTransition: sourceMode === "jspi" ? await qualifyProfileTransition(context, baseUrl, backend) : null,
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
      pythonMembership: [await qualifyPythonPreparation(context, baseUrl, backend, "scene"),
        await qualifyPythonPreparation(context, baseUrl, backend, "family"),
        await qualifyPythonPreparation(context, baseUrl, backend, "object")],
      pythonFade: await qualifyPythonPreparation(context, baseUrl, backend, "fade"),
      profileTransition: await qualifyProfileTransition(context, baseUrl, backend) };
  },
});

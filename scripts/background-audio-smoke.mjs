// Real browser-device qualification for the finite direct Rust audio profile.
// Instrumentation observes native Web Audio nodes; it does not synthesize audio,
// replace the runtime clock, or supply an alternative scene/player implementation.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import pngjs from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = path.resolve(process.env.NOON_AUDIO_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/background-audio"));
const backend = process.env.NOON_AUDIO_BACKEND ?? "webgpu";
assert.ok(["webgpu", "webgl"].includes(backend), "unknown audio qualification renderer backend");
const report = {
  scope: "finite-direct-rust-wasm-web-audio", status: "running", backend,
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  cases: [], browserErrors: [], consoleErrors: [],
};
await mkdir(artifacts, { recursive: true });
for (const source of ["web/pkg/noon_web_bg.wasm", "web/pkg/noon_web.js", "web/background-audio.js",
  "web/background-audio.html", "web/background-audio.css", "scripts/background-audio-smoke.mjs"]) {
  report[source] = createHash("sha256").update(await readFile(path.join(root, source))).digest("hex");
}

// Installed only by this harness, before the unmodified product page loads.
function observeAudio() {
  const NativeAudioContext = window.AudioContext;
  const contexts = [], starts = [], stops = [], buffers = [], parameters = [];
  const contextIds = new WeakMap(), sourceIds = new WeakMap(), bufferIds = new WeakMap();
  const parameterIds = new WeakMap(), sourceGains = new WeakMap();
  const pendingResumes = [];
  const connect = AudioNode.prototype.connect;
  const start = AudioBufferSourceNode.prototype.start;
  const stop = AudioBufferSourceNode.prototype.stop;
  const parameterValue = Object.getOwnPropertyDescriptor(AudioParam.prototype, "value");
  const contextId = context => contextIds.get(context);
  const parameterId = parameter => {
    if (!parameterIds.has(parameter)) {
      parameterIds.set(parameter, parameters.length);
      parameters.push({ events: [] });
    }
    return parameterIds.get(parameter);
  };
  const parameterEvent = (parameter, method, args) =>
    parameters[parameterId(parameter)].events.push({ method, args: Array.from(args) });
  for (const method of ["setValueAtTime", "linearRampToValueAtTime"]) {
    const native = AudioParam.prototype[method];
    AudioParam.prototype[method] = function (...args) {
      const value = native.apply(this, args);
      parameterEvent(this, method, args);
      return value;
    };
  }
  Object.defineProperty(AudioParam.prototype, "value", {
    ...parameterValue,
    set(value) { parameterValue.set.call(this, value); parameterEvent(this, "value", [value]); },
  });
  AudioNode.prototype.connect = function (destination, ...args) {
    if (this instanceof AudioBufferSourceNode && destination instanceof GainNode) {
      sourceGains.set(this, parameterId(destination.gain));
    }
    const record = contexts[contextId(this.context)];
    if (record && destination === this.context.destination && this !== record.analyser) {
      // Insert one unity-gain analyser after the actual master node. The native
      // destination remains connected and real device processing still runs.
      connect.call(this, record.analyser, ...args);
      if (!record.connected) {
        connect.call(record.analyser, destination);
        record.connected = true;
      }
      return destination;
    }
    return connect.call(this, destination, ...args);
  };
  AudioBufferSourceNode.prototype.start = function (...args) {
    const value = start.apply(this, args);
    starts.push({ context: contextId(this.context), source: sourceIds.get(this),
      buffer: bufferIds.get(this.buffer), gainParameter: sourceGains.get(this),
      loop: this.loop, loopStart: this.loopStart, loopEnd: this.loopEnd,
      args, observedAt: this.context.currentTime });
    return value;
  };
  AudioBufferSourceNode.prototype.stop = function (...args) {
    const value = stop.apply(this, args);
    stops.push({ context: contextId(this.context), source: sourceIds.get(this), args });
    return value;
  };
  class ObservedAudioContext extends NativeAudioContext {
    constructor(...args) {
      super(...args);
      const id = contexts.length;
      contextIds.set(this, id);
      const analyser = this.createAnalyser();
      analyser.fftSize = 2048;
      analyser.smoothingTimeConstant = 0;
      const record = { context: this, analyser, connected: false, states: [this.state] };
      contexts.push(record);
      this.addEventListener("statechange", () => record.states.push(this.state));
    }
    createBuffer(...args) {
      const buffer = super.createBuffer(...args);
      bufferIds.set(buffer, buffers.length);
      buffers.push({ context: contextId(this), channels: buffer.numberOfChannels,
        frames: buffer.length, sampleRate: buffer.sampleRate, duration: buffer.duration });
      return buffer;
    }
    createBufferSource(...args) {
      const source = super.createBufferSource(...args);
      sourceIds.set(source, this.nextObservedSource = (this.nextObservedSource ?? 0) + 1);
      return source;
    }
    resume() {
      const native = super.resume();
      if (!window.audioQualification.holdNextResume) return native;
      window.audioQualification.holdNextResume = false;
      return new Promise((resolve, reject) => {
        native.then(value => pendingResumes.push(() => resolve(value)), reject);
      });
    }
  }
  window.AudioContext = ObservedAudioContext;
  window.audioQualification = {
    contexts, starts, stops, buffers, parameters, pendingResumes,
    holdNextResume: false, renderer: null,
    sample() {
      return contexts.map(({ context, analyser, states }) => {
        const samples = new Float32Array(analyser.fftSize);
        analyser.getFloatTimeDomainData(samples);
        let sum = 0, peak = 0;
        for (const value of samples) { sum += value * value; peak = Math.max(peak, Math.abs(value)); }
        return { state: context.state, deviceTime: context.currentTime, states,
          rms: Math.sqrt(sum / samples.length), peak };
      });
    },
    snapshot() {
      const renderer = this.renderer;
      return { output: this.sample(), starts, stops, buffers, parameters,
        transport: renderer ? { time: renderer.time(), playing: renderer.isDirectPlaying(),
          enabled: renderer.audioEnabled(), voices: renderer.audioVoiceCount(),
          volume: renderer.audioVolume(), muted: renderer.audioMuted(),
          sceneRevision: String(renderer.directSceneRevision()) } : null };
    },
  };
}

let browser, server, page;
async function record(name, action) {
  const result = { name, status: "running" };
  report.cases.push(result);
  try {
    Object.assign(result, await action(), { status: "passed" });
    console.log(`[PASS] background audio: ${name}`);
  } catch (error) {
    Object.assign(result, { status: "failed", error: String(error.stack ?? error) });
    throw error;
  }
}
const state = () => page.evaluate(() => audioQualification.snapshot());
const near = (actual, expected, label, tolerance = 1e-6) =>
  assert.ok(Math.abs(actual - expected) <= tolerance, `${label}: ${actual} != ${expected}`);
async function playing() {
  await page.waitForFunction(() => document.querySelector("#stage-state").textContent === "PLAYING");
  await page.waitForFunction(() => audioQualification.sample().some(output =>
    output.state === "running" && output.rms > 1e-5 && output.peak > 1e-4), null, { timeout: 8_000 });
  return state();
}
async function quiet() {
  // Allow more than an analyser window to flush the pre-pause samples, then
  // require sustained silence while the real AudioContext keeps processing.
  await page.waitForTimeout(180);
  const samples = await page.evaluate(async () => {
    const results = [];
    for (let index = 0; index < 6; index++) {
      results.push(audioQualification.sample());
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    return results;
  });
  assert.ok(samples.every(window => window.length > 0 && window.every(output =>
    output.state === "running" && output.peak <= 1e-7)), "native output must remain silent");
  return samples;
}
async function pause() {
  await page.locator("#play").click();
  await page.waitForFunction(() => document.querySelector("#stage-state").textContent === "PAUSED");
  return state();
}
async function volume(value) {
  await page.locator("#volume").evaluate((input, target) => {
    input.value = String(target);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, value);
}
async function seek(time) {
  await page.locator("#scrubber").evaluate((input, target) => {
    input.value = String(target);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, time);
  await page.waitForFunction(target =>
    Math.abs(Number(document.querySelector("#scrubber").value) - target) < 0.001, time);
}
async function centroid(name) {
  const bytes = await page.locator("#scene").screenshot({ path: path.join(artifacts, `${name}.png`) });
  const image = pngjs.PNG.sync.read(bytes);
  let count = 0, x = 0, y = 0;
  // The stage's overlaid PAUSED label is also gold. Exclude its caption band
  // so this measurement follows the rendered point rather than UI text.
  for (let row = 0; row < Math.floor(image.height * 0.9); row++) for (let column = 0; column < image.width; column++) {
    const index = (row * image.width + column) * 4;
    const [red, green, blue] = image.data.subarray(index, index + 3);
    if (red > 140 && green > 100 && red > green * 1.04 && green > blue * 1.18) {
      count++; x += column; y += row;
    }
  }
  assert.ok(count > 30, "rendered scene must contain the moving gold point");
  return { x: x / count, y: y / count, pixels: count, width: image.width, height: image.height };
}
async function newProductPage() {
  const result = await browser.newPage({ viewport: { width: 1280, height: 1100 }, deviceScaleFactor: 1 });
  result.on("pageerror", error => report.browserErrors.push(String(error)));
  result.on("console", message => { if (message.type() === "error") report.consoleErrors.push(message.text()); });
  await result.route("**/favicon.ico", route => route.fulfill({ status: 204, body: "" }));
  await result.addInitScript(observeAudio);
  if (backend === "webgl") {
    await result.addInitScript(() => Object.defineProperty(navigator, "gpu", { value: undefined }));
  }
  await result.goto(`${server.baseUrl}/web/background-audio.html`, { waitUntil: "load" });
  await result.waitForFunction(() => !document.querySelector("#play").disabled, null, { timeout: 120_000 });
  // A transparent observation of a public host method captures the actual
  // production renderer; no production test global or alternate driver is used.
  await result.evaluate(async () => {
    const { ExecutionCanvasRenderer } = await import("/web/pkg/noon_web.js");
    const render = ExecutionCanvasRenderer.prototype.render;
    ExecutionCanvasRenderer.prototype.render = function (...args) {
      audioQualification.renderer = this;
      return render.apply(this, args);
    };
    window.dispatchEvent(new Event("resize"));
  });
  return result;
}

try {
  server = await serveRepository(root, 0);
  browser = await playwright.chromium.launch({ headless: true,
    ...(process.env.NOON_AUDIO_CHROMIUM ? { executablePath: process.env.NOON_AUDIO_CHROMIUM } : { channel: "chromium" }),
    args: [...browserArgs(backend), "--autoplay-policy=user-gesture-required"],
  });
  report.browser = browser.version();
  page = await newProductPage();
  const initial = await state();
  let initialPoint, pausedTime;
  await record("silent preparation and initial picture", async () => {
    assert.equal(initial.output.length, 0, "preparation must not construct an AudioContext");
    assert.equal(initial.starts.length, 0);
    assert.equal(initial.transport.time, 0);
    assert.equal(initial.transport.playing, false);
    initialPoint = await centroid("begin");
    await page.screenshot({ path: path.join(artifacts, "page.png"), fullPage: true });
    return { initial, point: initialPoint };
  });
  await record("trusted Play produces real finite PCM output", async () => {
    await page.locator("#play").click();
    const running = await playing();
    assert.equal(running.output.length, 1);
    assert.equal(running.buffers.length, 1);
    assert.deepEqual(running.buffers[0], { context: 0, channels: 2, frames: 1_920_000,
      sampleRate: 24_000, duration: 80 });
    assert.equal(running.starts.length, 1);
    const first = running.starts[0];
    near(first.args[1], 0, "initial source offset");
    near(first.args[2], 30, "finite source duration");
    assert.equal(first.loop, false);
    const automation = running.parameters[first.gainParameter].events;
    assert.deepEqual(automation.map(event => event.args[0]), [0, 0.5, 0.5, 0]);
    for (const [index, offset] of [0, 1.5, 27.5, 30].entries()) {
      near(automation[index].args[1] - first.args[0], offset, "authored fade time");
    }
    return { running };
  });
  await record("pause silences the device and freezes authored time", async () => {
    const paused = await pause();
    pausedTime = paused.transport.time;
    assert.ok(pausedTime > 0 && pausedTime < 5);
    assert.equal(paused.transport.voices, 0);
    const silence = await quiet();
    near((await state()).transport.time, pausedTime, "paused time", 0);
    return { paused, silence };
  });
  await record("resume preserves the PCM phase and immutable buffer", async () => {
    await page.locator("#play").click();
    const running = await playing();
    near(running.starts.at(-1).args[1], pausedTime, "resume offset");
    near(running.starts.at(-1).args[2], 30 - pausedTime, "remaining duration");
    assert.equal(running.buffers.length, 1);
    assert.equal(running.starts.at(-1).buffer, running.starts[0].buffer);
    await pause();
    return { running };
  });
  await record("key points move picture and audio through shared Rust navigation", async () => {
    await page.locator("#sections button").nth(1).click();
    near((await state()).transport.time, 10, "Discover time", 0);
    const discover = await centroid("discover");
    await page.locator("#next").click();
    near((await state()).transport.time, 20, "Next time", 0);
    const resolve = await centroid("resolve");
    await page.locator("#previous").click();
    near((await state()).transport.time, 10, "Previous time", 0);
    assert.ok(initialPoint.x < discover.x && discover.x < resolve.x, "point visits left, top, then right vertex");
    assert.ok(discover.y < initialPoint.y - 50 && discover.y < resolve.y - 50,
      "Discover is the upper triangle vertex");
    await page.locator("#play").click();
    const running = await playing();
    const source = running.starts.at(-1);
    near(source.args[1], 10, "section source offset", 0);
    near(source.args[2], 20, "section remaining duration", 0);
    const automation = running.parameters[source.gainParameter].events;
    near(automation[0].args[0], 0.5, "seek does not repeat the entrance fade", 0);
    return { discover, resolve, running };
  });
  await record("viewer mute and volume affect output without semantic edits", async () => {
    const before = await state();
    await page.locator("#mute").click();
    const mutedSilence = await quiet();
    const muted = await state();
    assert.ok(muted.transport.time > before.transport.time);
    assert.equal(muted.transport.muted, true);
    assert.equal(muted.transport.sceneRevision, before.transport.sceneRevision);
    await volume(0.25);
    await page.locator("#mute").click();
    const reduced = await playing();
    near(reduced.transport.volume, 0.25, "viewer volume", 0);
    assert.equal(reduced.transport.sceneRevision, before.transport.sceneRevision);
    assert.equal(reduced.starts.length, before.starts.length, "viewer gain must not restart the clip");
    await volume(0);
    const zeroVolumeSilence = await quiet();
    await volume(0.7);
    const restored = await playing();
    assert.equal(restored.transport.sceneRevision, before.transport.sceneRevision);
    return { before, muted, reduced, restored, mutedSilence, zeroVolumeSilence };
  });
  await record("scrub and restart replace device voices at the chosen scene time", async () => {
    await seek(12.5);
    const scrubbed = await playing();
    near(scrubbed.starts.at(-1).args[1], 12.5, "scrub source offset", 0);
    await page.locator("#restart").click();
    const restarted = await playing();
    near(restarted.starts.at(-1).args[1], 0, "restart source offset", 0);
    assert.equal(restarted.buffers.length, 1);
    assert.equal(restarted.transport.voices, 1);
    await pause();
    const rejected = await page.evaluate(() => {
      const renderer = audioQualification.renderer;
      const before = renderer.time();
      let message = null;
      try { renderer.seekDirect(40); } catch (error) { message = String(error); }
      return { before, after: renderer.time(), message };
    });
    assert.ok(rejected.message, "seek beyond the qualified finite audio interval must reject");
    assert.equal(rejected.before, rejected.after);
    return { scrubbed, restarted, rejected };
  });
  await record("finite completion stops at 30 seconds and stays silent", async () => {
    await seek(29.6);
    await page.locator("#play").click();
    await page.waitForFunction(() => document.querySelector("#stage-state").textContent === "COMPLETE",
      null, { timeout: 8_000 });
    const complete = await state();
    near(complete.transport.time, 30, "completed time", 0);
    assert.equal(complete.transport.playing, false);
    assert.equal(complete.transport.voices, 0);
    const silence = await quiet();
    near((await state()).transport.time, 30, "settled completion time", 0);
    return { complete, silence };
  });
  await record("page retirement closes the real device and stops its voice", async () => {
    await page.locator("#restart").click();
    await page.locator("#play").click();
    await playing();
    await page.evaluate(() => {
      audioQualification.renderer = null;
      window.dispatchEvent(new PageTransitionEvent("pagehide"));
    });
    await page.waitForFunction(() => audioQualification.contexts.every(({ context }) => context.state === "closed"));
    const retired = await state();
    assert.equal(retired.stops.at(-1).source, retired.starts.at(-1).source);
    return { retired };
  });
  await page.close();
  page = await newProductPage();
  await record("a hidden tab cancels a pending unlock until a new explicit Play", async () => {
    await page.evaluate(() => { audioQualification.holdNextResume = true; });
    await page.locator("#play").click();
    await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
    await page.evaluate(() => {
      // Drive the actual product visibility listener across a hide/show cycle
      // while the native device resume promise remains pending. Restore the
      // browser's own visibility getter before letting that promise settle.
      const previous = Object.getOwnPropertyDescriptor(document, "hidden");
      try {
        Object.defineProperty(document, "hidden", { configurable: true, value: true });
        document.dispatchEvent(new Event("visibilitychange"));
        Object.defineProperty(document, "hidden", { configurable: true, value: false });
        document.dispatchEvent(new Event("visibilitychange"));
      } finally {
        if (previous) Object.defineProperty(document, "hidden", previous);
        else delete document.hidden;
      }
      for (const resolve of audioQualification.pendingResumes.splice(0)) resolve();
    });
    const silence = await quiet();
    const cancelled = await state();
    assert.equal(cancelled.starts.length, 0, "a stale unlock must not start audio after a visibility pause");
    assert.equal(cancelled.transport.playing, false);
    near(cancelled.transport.time, 0, "cancelled unlock time", 0);
    await page.locator("#play").click();
    const running = await playing();
    assert.equal(running.starts.length, 1, "a new trusted Play starts the paused scene exactly once");
    near(running.starts[0].args[1], 0, "explicit Play after visibility pause", 0);
    return { cancelled, silence, running };
  });
  await page.close();
  page = await newProductPage();
  await record("a delayed native unlock cannot revive a retired run", async () => {
    await page.evaluate(() => { audioQualification.holdNextResume = true; });
    await page.locator("#play").click();
    await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
    await page.evaluate(() => {
      audioQualification.renderer = null;
      window.dispatchEvent(new PageTransitionEvent("pagehide"));
    });
    await page.waitForFunction(() => audioQualification.contexts.every(({ context }) => context.state === "closed"));
    await page.evaluate(() => { for (const resolve of audioQualification.pendingResumes.splice(0)) resolve(); });
    await page.waitForTimeout(200);
    const retired = await state();
    assert.equal(retired.starts.length, 0);
    assert.equal(retired.output[0].state, "closed");
    return { retired };
  });
  await record("free alone closes an active direct factory renderer", async () => {
    await page.evaluate(async () => {
      const wasm = await import("/web/pkg/noon_web.js");
      const renderer = await wasm.createBackgroundAudioRenderer(new OffscreenCanvas(640, 360));
      renderer.resize(640, 360);
      renderer.render();
      renderer.seekDirect(8);
      renderer.render();
      audioQualification.renderer = renderer;
      document.querySelector("#play").addEventListener("click", async () => {
        await renderer.enableAudio();
        renderer.resumeDirect();
        renderer.directWakeDirectiveJson(performance.now());
      }, { once: true });
    });
    await page.locator("#play").click();
    await page.waitForFunction(() => audioQualification.sample().some(output =>
      output.state === "running" && output.rms > 1e-5), null, { timeout: 8_000 });
    const active = await state();
    assert.equal(active.transport.voices, 1);
    await page.evaluate(() => {
      const renderer = audioQualification.renderer;
      audioQualification.renderer = null;
      renderer.free();
    });
    await page.waitForFunction(() => audioQualification.contexts.every(({ context }) => context.state === "closed"));
    const freed = await state();
    assert.equal(freed.stops.at(-1).source, freed.starts.at(-1).source);
    return { active, freed };
  });
  assert.deepEqual(report.browserErrors, [], "no uncaught browser/WASM errors");
  assert.deepEqual(report.consoleErrors, [], "no browser audio/GPU errors");
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error.stack ?? error);
  process.exitCode = 1;
  console.error(report.error);
  await page?.screenshot({ path: path.join(artifacts, "failure.png"), fullPage: true }).catch(() => {});
} finally {
  await browser?.close();
  await server?.close();
  await writeFile(path.join(artifacts, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
}

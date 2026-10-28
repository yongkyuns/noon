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
import { observeAudio } from "./browser-audio-observation.mjs";
import { releaseRetirementWitness } from "./audio-retirement-reference.mjs";

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
  "web/native-inputs.js", "web/browser-pointer-input.js", "web/browser-wheel-units.js",
  "web/background-audio.html", "web/background-audio.css", "scripts/background-audio-smoke.mjs", "scripts/browser-audio-observation.mjs", "scripts/audio-retirement-reference.mjs"]) {
  report[source] = createHash("sha256").update(await readFile(path.join(root, source))).digest("hex");
}


let browser, server, page;
async function record(name, action) {
  const result = { name, status: "running" };
  report.cases.push(result);
  try {
    Object.assign(result, await action(result), { status: "passed" });
    console.log(`[PASS] background audio: ${name}`);
  } catch (error) {
    Object.assign(result, { status: "failed", error: String(error.stack ?? error) });
    throw error;
  }
}
const state = () => page.evaluate(() => audioQualification.snapshot());
const near = (actual, expected, label, tolerance = 1e-6) =>
  assert.ok(Math.abs(actual - expected) <= tolerance, `${label}: ${actual} != ${expected}`);
const attackSeconds = 0.040, releaseSeconds = 0.060, viewerRampSeconds = 0.020;
function rampTo(snapshot, parameter, target, duration, label) {
  assert.ok(Number.isInteger(parameter), `${label}: native graph must expose its gain parameter`);
  const events = snapshot.parameters[parameter].events;
  const index = events.findLastIndex(event => event.method === "linearRampToValueAtTime"
    && Math.abs(event.args[0] - target) < 1e-6);
  assert.ok(index >= 0, `${label}: must schedule a native linear ramp to ${target}`);
  const ramp = events[index];
  const hold = events.slice(0, index).findLast(event =>
    event.method === "setValueAtTime" || event.method === "linearRampToValueAtTime");
  assert.ok(hold, `${label}: ramp must have an explicit starting value/time`);
  assert.equal(hold.method, "linearRampToValueAtTime",
    `${label}: close the prior linear segment at its held value before retargeting`);
  near(ramp.args[1] - hold.args[1], duration, `${label} duration`);
  return { hold, ramp };
}
function valueOnRamp({ hold, ramp }, time) {
  const progress = Math.max(0, Math.min(1, (time - hold.args[1]) / (ramp.args[1] - hold.args[1])));
  return hold.args[0] + (ramp.args[0] - hold.args[0]) * progress;
}
function sourceStoppedAt(snapshot, source, deadline, label) {
  const stop = snapshot.stops.find(event => event.node === source.node
    && event.args.length === 1 && Math.abs(event.args[0] - deadline) < 1e-6);
  assert.ok(stop, `${label}: native source must stop at the exact release deadline`);
  assert.ok(stop.args[0] > stop.observedAt, `${label}: release stop must be scheduled in the future`);
  return stop;
}
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
// Known authored coordinates of this test scene. Production JavaScript neither
// knows these positions nor chooses a control action; real pointer occurrences
// travel through the normal collector, displayed-frame receipt and Rust picker.
const sceneControls = { previous: -4, play: -2, next: 0, restart: 2, mute: 4 };
async function scenePosition(x, y) {
  const canvas = page.locator("#scene");
  await canvas.scrollIntoViewIfNeeded();
  const box = await canvas.boundingBox();
  assert.ok(box, "scene canvas must have a visible surface");
  return { x: box.x + box.width / 2 + x * box.height / 8,
    y: box.y + box.height / 2 - y * box.height / 8 };
}
async function clickSceneControl(name) {
  assert.ok(Object.hasOwn(sceneControls, name), "known authored scene control");
  const position = await scenePosition(sceneControls[name], -2.6);
  await page.mouse.click(position.x, position.y);
}
async function pausedAt(time) {
  await page.waitForFunction(target => {
    const renderer = audioQualification.renderer;
    return renderer && !renderer.isDirectPlaying() && renderer.time() === target;
  }, time);
  return state();
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
  await record("trusted in-scene Play produces real finite PCM output", async () => {
    await clickSceneControl("play");
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
    assert.notEqual(first.gainParameter, first.busParameter, "authored and transport gains are separate native nodes");
    assert.notEqual(first.busParameter, first.masterParameter, "viewer gain is separate from the transport envelope");
    const attack = rampTo(running, first.busParameter, 1, attackSeconds, "initial transport attack");
    near(attack.hold.args[0], 0, "initial attack starts muted", 0);
    assert.ok(Number.isFinite(attack.hold.observedAt));
    assert.ok(attack.hold.args[1] >= first.args[0] - 1e-9
      && attack.hold.args[1] <= attack.hold.observedAt + 1e-9,
    "initial attack begins after source admission and no later than its native scheduling call");
    return { running, attack };
  });
  await record("pause silences the device and freezes authored time", async () => {
    await clickSceneControl("play");
    await page.waitForFunction(() => document.querySelector("#stage-state").textContent === "PAUSED");
    const paused = await state();
    pausedTime = paused.transport.time;
    assert.ok(pausedTime > 0 && pausedTime < 5);
    assert.ok(paused.transport.voices <= 1, "pause may retain only its bounded release voice");
    const source = paused.starts.at(-1);
    const release = rampTo(paused, source.busParameter, 0, releaseSeconds, "pause release");
    const stop = sourceStoppedAt(paused, source, release.ramp.args[1], "pause");
    const silence = await quiet();
    const settled = await state();
    near(settled.transport.time, pausedTime, "paused time", 0);
    assert.equal(settled.transport.voices, 0, "native ended cleanup releases the paused graph");
    const retirement = releaseRetirementWitness(settled, source, release.ramp.args[1]);
    assert.ok(retirement, "release retires after the exact native stop or completed device time");
    return { paused, release, stop, silence, settled, retirement };
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
    const muteRamp = rampTo(muted, before.starts.at(-1).masterParameter, 0,
      viewerRampSeconds, "viewer mute");
    assert.ok(muted.transport.time > before.transport.time);
    assert.equal(muted.transport.muted, true);
    assert.equal(muted.transport.sceneRevision, before.transport.sceneRevision);
    await volume(0.25);
    await page.locator("#mute").click();
    const reduced = await playing();
    const unmuteRamp = rampTo(reduced, before.starts.at(-1).masterParameter, 0.25,
      viewerRampSeconds, "viewer unmute");
    near(reduced.transport.volume, 0.25, "viewer volume", 0);
    assert.equal(reduced.transport.sceneRevision, before.transport.sceneRevision);
    assert.equal(reduced.starts.length, before.starts.length, "viewer gain must not restart the clip");
    await volume(0);
    const zeroVolumeSilence = await quiet();
    await volume(0.7);
    const restored = await playing();
    assert.equal(restored.transport.sceneRevision, before.transport.sceneRevision);
    return { before, muted, reduced, restored, muteRamp, unmuteRamp, mutedSilence, zeroVolumeSilence };
  });
  await record("scene key-point buttons seek picture and sound while preserving transport state", async () => {
    const before = await state();
    await clickSceneControl("play");
    await page.waitForFunction(() => !audioQualification.renderer.isDirectPlaying());
    await clickSceneControl("restart");
    await pausedAt(0);
    await clickSceneControl("previous");
    await pausedAt(0);
    await clickSceneControl("next");
    await pausedAt(10);
    const discover = await centroid("scene-discover");
    await clickSceneControl("next");
    await pausedAt(20);
    const resolve = await centroid("scene-resolve");
    await clickSceneControl("next");
    await pausedAt(20);
    await clickSceneControl("previous");
    const previous = await pausedAt(10);
    assert.equal(previous.transport.sceneRevision, before.transport.sceneRevision);
    assert.ok(initialPoint.x < discover.x && discover.x < resolve.x);
    assert.ok(discover.y < initialPoint.y - 50 && discover.y < resolve.y - 50);
    await clickSceneControl("play");
    const running = await playing();
    near(running.starts.at(-1).args[1], 10, "scene section resume source offset", 0);
    await clickSceneControl("next");
    await page.waitForFunction(() => audioQualification.starts.at(-1)?.args[1] === 20);
    const nextWhilePlaying = await playing();
    near(nextWhilePlaying.starts.at(-1).args[1], 20, "playing scene Next source offset", 0);
    assert.equal(nextWhilePlaying.transport.playing, true);
    await clickSceneControl("restart");
    await page.waitForFunction(() => audioQualification.starts.at(-1)?.args[1] === 0);
    const restarted = await playing();
    near(restarted.starts.at(-1).args[1], 0, "playing scene Restart source offset", 0);
    assert.equal(restarted.transport.playing, true);
    assert.equal(restarted.buffers.length, 1);
    return { discover, resolve, previous, running, nextWhilePlaying, restarted };
  });
  await record("scene Sound controls viewer gain without restarting or rewriting the scene", async () => {
    const before = await state();
    await clickSceneControl("mute");
    const silence = await quiet();
    const muted = await state();
    assert.equal(muted.transport.muted, true);
    assert.equal(muted.transport.playing, true);
    assert.ok(muted.transport.time > before.transport.time);
    assert.equal(muted.transport.sceneRevision, before.transport.sceneRevision);
    assert.equal(muted.starts.length, before.starts.length);
    await clickSceneControl("mute");
    const restored = await playing();
    assert.equal(restored.transport.muted, false);
    assert.equal(restored.starts.length, before.starts.length);
    assert.equal(restored.transport.sceneRevision, before.transport.sceneRevision);
    return { before, muted, restored, silence };
  });
  await record("background, drag and cancelled scene gestures cannot activate playback", async () => {
    await clickSceneControl("play");
    await page.waitForFunction(() => !audioQualification.renderer.isDirectPlaying());
    await clickSceneControl("restart");
    const before = await pausedAt(0);
    const background = await scenePosition(5.7, 2.6);
    await page.mouse.click(background.x, background.y);
    const afterBackground = await state();
    assert.equal(afterBackground.transport.playing, false);
    const play = await scenePosition(sceneControls.play, -2.6);
    await page.mouse.move(play.x, play.y);
    await page.mouse.down();
    await page.mouse.move(play.x + 30, play.y, { steps: 3 });
    await page.mouse.move(play.x, play.y, { steps: 3 });
    await page.mouse.up();
    const afterDrag = await state();
    assert.equal(afterDrag.transport.playing, false, "returning inside after drag must not count as a click");
    await page.mouse.down();
    // The host receives the browser's cancellation occurrence. This deliberately
    // supplies no target or action, and the later physical release must not revive it.
    await page.locator("#scene").dispatchEvent("pointercancel", {
      pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, buttons: 0,
      clientX: play.x, clientY: play.y,
    });
    await page.mouse.up();
    const afterCancel = await pausedAt(0);
    assert.equal(afterCancel.starts.length, before.starts.length);
    assert.equal(afterCancel.transport.sceneRevision, before.transport.sceneRevision);
    await clickSceneControl("play");
    const fresh = await playing();
    assert.equal(fresh.starts.length, before.starts.length + 1,
      "one fresh complete click starts playback after cancellation");
    return { before, afterBackground, afterDrag, afterCancel, fresh };
  });
  await record("scrub and restart replace device voices at the chosen scene time", async () => {
    await seek(12.5);
    const scrubbed = await playing();
    near(scrubbed.starts.at(-1).args[1], 12.5, "scrub source offset", 0);
    await page.locator("#restart").click();
    await page.waitForFunction(() => audioQualification.renderer.audioVoiceCount() === 1,
      null, { timeout: 2_000 });
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
  await record("transport release and attack use separate native ramps without moving source anchors", async () => {
    await seek(8);
    await page.locator("#play").click();
    await playing();
    await page.waitForTimeout(120);
    const transition = await page.evaluate(async () => {
      const qualification = audioQualification;
      const renderer = qualification.renderer;
      const before = qualification.snapshot();
      const oldSource = qualification.starts.at(-1);
      renderer.seekDirect(10);
      renderer.render();
      renderer.directWakeDirectiveJson(performance.now());
      const newSource = qualification.starts.at(-1);
      const admitted = qualification.snapshot();
      const observations = [];
      // These values come from the real native AudioParams while the device
      // renders. Event inspection below separately verifies exact deadlines.
      for (let index = 0; index < 35; index++) {
        observations.push({ deviceTime: qualification.contexts[newSource.context].context.currentTime,
          oldBus: qualification.parameterValue(oldSource.busParameter),
          newBus: qualification.parameterValue(newSource.busParameter),
          output: qualification.sample()[newSource.context] });
        await new Promise(resolve => setTimeout(resolve, 4));
      }
      return { before, admitted, oldSource, newSource, observations, settled: qualification.snapshot() };
    });
    const { oldSource, newSource, admitted, observations, settled } = transition;
    const release = rampTo(admitted, oldSource.busParameter, 0, releaseSeconds, "seek release");
    const attack = rampTo(admitted, newSource.busParameter, 1, attackSeconds, "seek attack");
    sourceStoppedAt(admitted, oldSource, release.ramp.args[1], "seek");
    near(attack.hold.args[0], 0, "replacement graph starts muted", 0);
    near(attack.hold.args[1], release.ramp.args[1], "attack begins at the retained release deadline");
    near(newSource.args[1], 10, "new source keeps its admitted scene offset", 0);
    near(newSource.args[2], 20, "new source keeps its admitted duration", 0);
    near(admitted.transport.time, 10, "output transition does not advance authored time", 0);
    assert.ok(newSource.args[0] < attack.hold.args[1], "new PCM runs behind the muted bus before attack");
    assert.equal(admitted.transport.sceneRevision, transition.before.transport.sceneRevision);
    const waiting = observations.filter(sample => sample.deviceTime < attack.hold.args[1]);
    assert.ok(waiting.length > 0, "observe the replacement's muted waiting interval");
    assert.ok(waiting.every(sample => sample.newBus === 0));
    assert.ok(observations.some(sample => sample.oldBus > 0.01 && sample.oldBus < 0.99),
      "native retiring gain traverses intermediate values");
    assert.ok(observations.some(sample => sample.newBus > 0.01 && sample.newBus < 0.99),
      "native replacement gain traverses intermediate values");
    assert.ok(observations.some(sample => sample.deviceTime >= attack.ramp.args[1]
      && sample.output.rms > 1e-5), "replacement produces real output after attack");
    assert.equal(settled.transport.voices, 1);
    assert.ok(settled.ended.some(event => event.node === oldSource.node));
    assert.ok(settled.disconnects.some(event => event.node === oldSource.node));
    return { ...transition, release, attack };
  });
  await record("rapid viewer-gain changes retarget continuously without changing clip phase", async evidence => {
    const result = await page.evaluate(async () => {
      const qualification = audioQualification;
      const renderer = qualification.renderer;
      const source = qualification.starts.at(-1);
      const parameter = source.masterParameter;
      const before = qualification.snapshot();
      const firstIndex = qualification.parameters[parameter].events.length;
      renderer.setAudioVolume(0.1);
      const first = qualification.snapshot();
      renderer.setAudioVolume(0.4);
      const second = qualification.snapshot();
      await new Promise(resolve => setTimeout(resolve, 8));
      // Device time can stay unchanged across the wall-time delay above. Keep
      // this target above the initial 0.7 and both earlier targets so the third
      // ramp still traverses native values when all retargets share one quantum.
      renderer.setAudioVolume(0.9);
      const third = qualification.snapshot();
      const observations = [];
      for (let index = 0; index < 12; index++) {
        observations.push({ deviceTime: qualification.contexts[source.context].context.currentTime,
          value: qualification.parameterValue(parameter), output: qualification.sample()[source.context] });
        await new Promise(resolve => setTimeout(resolve, 4));
      }
      return { parameter, firstIndex, before, first, second, third, observations,
        settled: qualification.snapshot() };
    });
    // Keep the actual device observations even when a qualification assertion
    // fails; a later failure snapshot cannot reconstruct this short interval.
    Object.assign(evidence, result);
    const first = rampTo(result.first, result.parameter, 0.1, viewerRampSeconds, "first viewer retarget");
    const second = rampTo(result.second, result.parameter, 0.4, viewerRampSeconds, "second viewer retarget");
    const third = rampTo(result.third, result.parameter, 0.9, viewerRampSeconds, "third viewer retarget");
    near(second.hold.args[0], valueOnRamp(first, second.hold.args[1]), "second ramp holds the prior curve");
    near(third.hold.args[0], valueOnRamp(second, third.hold.args[1]), "third ramp holds the prior curve");
    const changed = result.settled.parameters[result.parameter].events.slice(result.firstIndex);
    assert.ok(changed.filter(event => event.method === "cancelScheduledValues").length >= 3,
      "each viewer retarget cancels superseded native automation");
    assert.ok(changed.every(event => event.method !== "value"), "viewer changes must not step the native gain");
    assert.ok(third.hold.args[0] < 0.899, "third viewer target must differ from its held value");
    assert.ok(result.observations.some(sample => sample.value > third.hold.args[0] + 0.001
      && sample.value < 0.899), "native master gain traverses its new ramp");
    near(result.settled.parameters[result.parameter].value, 0.9, "settled native viewer gain");
    assert.equal(result.settled.starts.length, result.before.starts.length);
    assert.equal(result.settled.transport.sceneRevision, result.before.transport.sceneRevision);
    assert.ok(result.settled.transport.time > result.before.transport.time);
    return { ...result, firstRamp: first, secondRamp: second, thirdRamp: third };
  });
  await record("rapid admitted seeks keep only the latest muted replacement and one releasing graph", async () => {
    const burst = await page.evaluate(() => {
      const qualification = audioQualification;
      const renderer = qualification.renderer;
      const before = qualification.snapshot();
      const oldSource = qualification.starts.at(-1);
      const steps = [];
      for (let index = 0; index < 16; index++) {
        const target = 8 + index * 0.25;
        renderer.seekDirect(target);
        renderer.render();
        renderer.directWakeDirectiveJson(performance.now());
        const source = qualification.starts.at(-1);
        steps.push({ target, time: renderer.time(), voices: renderer.audioVoiceCount(),
          revision: String(renderer.directSceneRevision()), source,
          busGain: qualification.parameterValue(source.busParameter),
          bus: structuredClone(qualification.parameters[source.busParameter]) });
      }
      return { before, oldSource, steps, after: qualification.snapshot() };
    });
    const release = rampTo(burst.after, burst.oldSource.busParameter, 0, releaseSeconds, "burst initial release");
    sourceStoppedAt(burst.after, burst.oldSource, release.ramp.args[1], "burst initial graph");
    assert.equal(burst.after.starts.length - burst.before.starts.length, burst.steps.length,
      "every admitted seek starts exactly its own replacement source");
    for (const step of burst.steps) {
      near(step.time, step.target, "burst committed scene time", 0);
      near(step.source.args[1], step.target, "burst PCM source offset", 0);
      near(step.source.args[2], 30 - step.target, "burst PCM remaining duration", 0);
      assert.equal(step.revision, burst.before.transport.sceneRevision);
      assert.ok(step.voices >= 1 && step.voices <= 2, "voice lifetime is bounded during every admitted seek");
    }
    const waiting = burst.steps.filter(step => step.source.args[0] < release.ramp.args[1]);
    assert.ok(waiting.length >= 2, "burst must exercise multiple commands inside one release interval");
    for (const step of waiting) {
      const attack = rampTo({ parameters: { [step.source.busParameter]: step.bus } },
        step.source.busParameter, 1, attackSeconds, "waiting replacement attack");
      near(attack.hold.args[1], release.ramp.args[1], "rapid retarget preserves the original release deadline");
      near(step.busGain, 0, "superseded replacement remains inaudible", 0);
    }
    const originalStops = burst.after.stops.filter(event => event.node === burst.oldSource.node
      && event.args.length === 1 && event.args[0] > event.observedAt);
    assert.equal(originalStops.length, 1, "rapid retargeting does not extend or reschedule the audible release");
    await page.waitForFunction(() => audioQualification.renderer.audioVoiceCount() === 1,
      null, { timeout: 2_000 });
    await page.waitForTimeout(80);
    const settled = await playing();
    assert.equal(settled.starts.length, burst.after.starts.length, "no queued commands or late source starts");
    assert.equal(settled.buffers.length, 1, "transition bursts reuse prepared PCM");
    assert.equal(settled.transport.voices, 1);
    assert.equal(settled.transport.sceneRevision, burst.before.transport.sceneRevision);
    for (const step of burst.steps.slice(0, -1)) {
      assert.ok(settled.disconnects.some(event => event.node === step.source.node),
        "every superseded source is disconnected after the burst settles");
    }
    return { ...burst, release, settled };
  });
  await record("visibility hard-stops both graphs during a pending transport release", async () => {
    const result = await page.evaluate(() => {
      const qualification = audioQualification;
      const renderer = qualification.renderer;
      const oldSource = qualification.starts.at(-1);
      renderer.seekDirect(12.75);
      renderer.render();
      renderer.directWakeDirectiveJson(performance.now());
      const newSource = qualification.starts.at(-1);
      const releasing = qualification.snapshot();
      const previous = Object.getOwnPropertyDescriptor(document, "hidden");
      try {
        Object.defineProperty(document, "hidden", { configurable: true, value: true });
        document.dispatchEvent(new Event("visibilitychange"));
      } finally {
        if (previous) Object.defineProperty(document, "hidden", previous);
        else delete document.hidden;
      }
      document.dispatchEvent(new Event("visibilitychange"));
      return { oldSource, newSource, releasing, stopped: qualification.snapshot() };
    });
    const release = rampTo(result.releasing, result.oldSource.busParameter, 0, releaseSeconds, "pending visibility release");
    assert.equal(result.releasing.transport.voices, 2, "lifecycle event occurs with both graph lifetimes present");
    assert.equal(result.stopped.transport.voices, 0, "visibility must hard-stop instead of retaining a release tail");
    assert.equal(result.stopped.transport.playing, false);
    for (const source of [result.oldSource, result.newSource]) {
      assert.ok(result.stopped.stops.some(event => event.node === source.node
        && (event.args.length === 0 || event.args[0] <= event.observedAt)),
      "lifecycle cancellation immediately stops each native source");
      assert.ok(result.stopped.disconnects.some(event => event.node === source.node
        && event.observedAt < release.ramp.args[1]), "lifecycle cleanup does not wait for the release deadline");
    }
    const silence = await quiet();
    const settled = await state();
    assert.equal(settled.starts.length, result.stopped.starts.length);
    near(settled.transport.time, result.stopped.transport.time, "visibility freezes authored time", 0);
    return { ...result, release, silence, settled };
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
  await record("visibility invalidates a delayed scene Play before its unlock completes", async () => {
    await page.evaluate(() => { audioQualification.holdNextResume = true; });
    await clickSceneControl("play");
    await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
    const sequence = await page.evaluate(() => {
      const renderer = audioQualification.renderer;
      const pending = renderer.pendingSceneControlSequence();
      if (pending === undefined) throw new Error("scene Play must retain its admitted input request");
      const unlock = renderer.sceneControlAudioUnlock();
      if (renderer.enableAudio() !== unlock || renderer.enableAudio() !== unlock) {
        throw new Error("repeated preparation must reuse the one outstanding native device operation");
      }
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
      return String(pending);
    });
    const silence = await quiet();
    const cancelled = await pausedAt(0);
    assert.equal(cancelled.starts.length, 0);
    const stale = await page.evaluate(value => {
      const renderer = audioQualification.renderer;
      return { pending: renderer.pendingSceneControlSequence() === undefined,
        applied: renderer.completeSceneControl(BigInt(value)) };
    }, sequence);
    assert.equal(stale.pending, true, "visibility pause retires the pending scene command");
    assert.equal(stale.applied, false, "the old input sequence cannot revive playback");
    await clickSceneControl("play");
    const running = await playing();
    assert.equal(running.starts.length, 1);
    near(running.starts[0].args[1], 0, "fresh scene Play after visibility cancellation", 0);
    return { sequence, cancelled, stale, silence, running };
  });
  await page.close();
  page = await newProductPage();
  await record("retirement closes audio during a delayed scene Play without later revival", async () => {
    await page.evaluate(() => { audioQualification.holdNextResume = true; });
    await clickSceneControl("play");
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
    await page.waitForTimeout(80);
    const active = await state();
    assert.equal(active.transport.voices, 1);
    const pending = await page.evaluate(() => {
      const renderer = audioQualification.renderer;
      const oldSource = audioQualification.starts.at(-1);
      renderer.seekDirect(8.5);
      renderer.render();
      renderer.directWakeDirectiveJson(performance.now());
      const newSource = audioQualification.starts.at(-1);
      const releasing = audioQualification.snapshot();
      audioQualification.renderer = null;
      renderer.free();
      return { oldSource, newSource, releasing };
    });
    await page.waitForFunction(() => audioQualification.contexts.every(({ context }) => context.state === "closed"));
    const freed = await state();
    const release = rampTo(pending.releasing, pending.oldSource.busParameter, 0,
      releaseSeconds, "release pending at free");
    assert.equal(pending.releasing.transport.voices, 2);
    for (const source of [pending.oldSource, pending.newSource]) {
      assert.ok(freed.stops.some(event => event.node === source.node
        && (event.args.length === 0 || event.args[0] <= event.observedAt)),
      "free immediately stops both current and retiring sources");
      assert.ok(freed.disconnects.some(event => event.node === source.node
        && event.observedAt < release.ramp.args[1]));
    }
    return { active, pending, release, freed };
  });
  assert.deepEqual(report.browserErrors, [], "no uncaught browser/WASM errors");
  assert.deepEqual(report.consoleErrors, [], "no browser audio/GPU errors");
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error.stack ?? error);
  process.exitCode = 1;
  console.error(report.error);
  report.failureState = await page?.evaluate(() => ({
    stage: document.querySelector("#stage-state")?.textContent,
    status: document.querySelector("#status")?.textContent,
    audio: window.audioQualification?.snapshot(),
  })).catch(error => ({ unavailable: String(error) }));
  await page?.screenshot({ path: path.join(artifacts, "failure.png"), fullPage: true }).catch(() => {});
} finally {
  await browser?.close();
  await server?.close();
  await writeFile(path.join(artifacts, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
}

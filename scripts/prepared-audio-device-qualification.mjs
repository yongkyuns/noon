// Exercise open-prefix stop bounds with real Window Web Audio nodes. Rust owns
// clip phase and authorization; this harness only supplies ordered anchors.
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { observeAudio } from "./browser-audio-observation.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const server = await serveRepository(root, 0);
let browser;
try {
  browser = await playwright.chromium.launch({ headless: true, channel: "chromium",
    args: [...browserArgs("webgl"), "--autoplay-policy=user-gesture-required"] });
  const context = await browser.newContext();
  await context.addInitScript(observeAudio);
  const page = await context.newPage();
  await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  await page.evaluate(async () => {
    const { default: init, WasmWorkerAudioOutput } = await import("./pkg/noon_web.js");
    await init();
    const sampleRate = 8_000, frames = sampleRate;
    const metadata = new TextEncoder().encode(JSON.stringify({ version: 1, session: 23,
      resources: [{ handle: [1, 1, 1], sample_rate: sampleRate, channels: 1, samples: frames }],
      clips: [
        { identity: [1, 1], resource: 0, start_time: 0, source_start_frame: 0,
          source_end_frame: frames, duration: 1, looped: false, gain: 0.5, fade_in: 0, fade_out: 0 },
        { identity: [2, 1], resource: 0, start_time: 0.6, source_start_frame: 0,
          source_end_frame: frames, duration: 1, looped: false, gain: 0.5, fade_in: 0, fade_out: 0 },
      ] }));
    const bytes = new Uint8Array(4 + metadata.length + frames * 4);
    new DataView(bytes.buffer).setUint32(0, metadata.length, true);
    bytes.set(metadata, 4);
    const pcm = new DataView(bytes.buffer, 4 + metadata.length);
    for (let frame = 0; frame < frames; frame++) {
      pcm.setFloat32(frame * 4, 0.2 * Math.sin(2 * Math.PI * 440 * frame / sampleRate), true);
    }
    globalThis.audioTest = { output: new WasmWorkerAudioOutput(bytes), bytes, sampleRate };
    document.body.innerHTML = '<main><p id="picture">Prepared scene at time zero</p><button id="play">Play</button><button id="restart">Restart</button></main>';
    document.querySelector("#play").addEventListener("click", async () => {
      const sampled = performance.timeOrigin + performance.now();
      await audioTest.output.unlock();
      const received = performance.timeOrigin + performance.now();
      audioTest.output.startUntil(23, 0, 0.25, sampled, received);
      audioTest.firstAnchor = { sampled, received };
    });
    document.querySelector("#restart").addEventListener("click", async () => {
      audioTest.output.stop();
      audioTest.output.free();
      audioTest.output = new WasmWorkerAudioOutput(audioTest.bytes);
      await audioTest.output.unlock();
      const sampled = performance.timeOrigin + performance.now();
      audioTest.output.startUntil(23, 0, 0.25, sampled, sampled);
    });
  });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const prepared = await page.evaluate(() => audioQualification.snapshot());
  assert.equal(prepared.starts.length, 0, "preparation must not start an audio source");
  assert.equal(prepared.output.length, 1);
  assert.ok(prepared.output.every(output => output.peak === 0), "preparation must stay silent");

  await page.getByRole("button", { name: "Play" }).click();
  await page.waitForFunction(() => audioQualification.starts.length === 1);
  const first = await page.evaluate(() => audioQualification.snapshot());
  assert.equal(first.starts[0].args.length, 2, "no-duration source start is required for extensible stops");
  const firstStop = first.stops.find(stop => stop.node === first.starts[0].node);
  assert.ok(firstStop && firstStop.args[0] > firstStop.observedAt,
    "first source must stop at the current Rust authorization boundary");
  await page.waitForFunction(() => audioQualification.sample().some(output => output.rms > 1e-5));

  await page.evaluate(() => {
    const sampled = performance.timeOrigin + performance.now();
    audioTest.output.extendUntil(23, 0.05, 0.5, sampled, sampled);
  });
  const extended = await page.evaluate(() => audioQualification.snapshot());
  assert.equal(extended.starts.length, 1, "an active voice keeps its node when its stop is extended");
  const replacementStop = extended.stops.filter(stop => stop.node === extended.starts[0].node).at(-1);
  assert.ok(replacementStop.args[0] > firstStop.args[0], "the same active source receives a later stop");
  assert.equal(extended.starts.some(start => start.args[0] >= firstStop.args[0]), false,
    "future clips stay unstarted until their Rust start time is authorized");

  await page.evaluate(() => {
    const sampled = performance.timeOrigin + performance.now();
    audioTest.output.extendUntil(23, 0.2, 0.8, sampled, sampled);
  });
  await page.waitForFunction(() => audioQualification.starts.length === 2);
  const future = await page.evaluate(() => audioQualification.snapshot());
  assert.equal(future.starts[1].args.length, 2);
  assert.ok(Math.abs(future.starts[1].args[0] - future.starts[1].observedAt - 0.4) < 0.03,
    "the later clip starts at its Rust-authored time after authorization expands");
  const secondStop = future.stops.filter(stop => stop.node === future.starts[1].node).at(-1);
  assert.ok(secondStop && secondStop.args[0] > future.starts[1].observedAt);

  await page.waitForFunction(() => audioQualification.ended.length >= 1);
  await page.getByRole("button", { name: "Restart" }).click();
  await page.waitForFunction(() => audioQualification.starts.length === 3);
  const endedBeforeBoundary = await page.evaluate(() => audioQualification.ended.length);
  await page.waitForFunction(count => audioQualification.ended.length > count, endedBeforeBoundary);
  await page.evaluate(() => {
    const sampled = performance.timeOrigin + performance.now();
    audioTest.output.extendUntil(23, 0.3, 0.7, sampled, sampled);
  });
  await page.waitForFunction(() => audioQualification.starts.length === 5);
  const restarted = await page.evaluate(() => audioQualification.snapshot());
  assert.ok(Math.abs(restarted.starts[3].args[1] - 0.3) < 0.04,
    "after a real buffer boundary, a new graph resumes from the fresh Rust clip offset");
  assert.equal(restarted.starts[4].args.length, 2,
    "a later clip is scheduled when its start enters the expanded prefix");

  const roundoff = await page.evaluate(() => {
    audioTest.output.stop();
    const sampled = performance.timeOrigin + performance.now();
    const before = audioQualification.starts.length;
    audioTest.output.startUntil(23, 0, 0.15, sampled, sampled);
    audioTest.output.extendUntil(23, 0, 0.3, sampled, sampled + 0.00001);
    const node = audioQualification.starts.at(-1).node;
    return {
      startsAdded: audioQualification.starts.length - before,
      node,
      stops: audioQualification.stops.filter(stop => stop.node === node && stop.args.length > 0),
    };
  });
  assert.equal(roundoff.startsAdded, 1,
    "fractional Rust clock aging extends the same active clip instead of starting a duplicate");
  assert.equal(roundoff.stops.filter(stop => stop.args.length > 0).length, 2,
    "the fractional extension reschedules the same voice's pending stop");

  await page.evaluate(() => audioTest.output.stop());
  const initialCycleCount = await page.evaluate(() => {
    const sampled = performance.timeOrigin + performance.now();
    const before = audioQualification.starts.length;
    audioTest.output.startUntil(23, 0, 0.1, sampled, sampled);
    return before;
  });
  await page.waitForFunction(count => audioQualification.starts.length > count, initialCycleCount);
  for (let boundary = 1; boundary <= 5; boundary++) {
    const node = await page.evaluate(() => audioQualification.starts.at(-1).node);
    await page.waitForFunction(sourceNode => audioQualification.ended.some(event => event.node === sourceNode), node);
    const nextCount = await page.evaluate(() => audioQualification.starts.length);
    await page.evaluate(boundaryIndex => {
      const sampled = performance.timeOrigin + performance.now();
      const sceneTime = boundaryIndex / 10;
      audioTest.output.extendUntil(23, sceneTime, sceneTime + 0.1, sampled, sampled);
    }, boundary);
    await page.waitForFunction(count => audioQualification.starts.length > count, nextCount);
    const bounded = await page.evaluate(() => ({
      disconnects: audioQualification.disconnects.map(disconnect => disconnect.node),
      ended: audioQualification.ended.map(event => event.node),
      starts: audioQualification.starts.map(start => start.node),
      node: audioQualification.starts.at(-1).node,
    }));
    assert.ok(bounded.disconnects.includes(node), "each resumed frontier disconnects its ended source");
    assert.equal(bounded.starts.filter(startNode => !bounded.ended.includes(startNode)).length, 1,
      "each restarted frontier has one live source while prior voices have ended");
  }
  await page.evaluate(() => audioTest.output.stop());
  console.log(JSON.stringify({ status: "passed", scope: "real-webaudio-open-prefix-bounds",
    preparation: { contexts: prepared.output.length, starts: prepared.starts.length },
    firstStop: firstStop.args[0], replacementStop: replacementStop.args[0],
    futureSourceStart: future.starts[1].args[0], futureSourceStop: secondStop.args[0],
    restartOffset: restarted.starts[3].args[1], roundoff: { startsAdded: roundoff.startsAdded },
    repeatedFrontiers: 5 }, null, 2));
} finally {
  await browser?.close();
  await server.close();
}

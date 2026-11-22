import assert from "node:assert/strict";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const [asset] = process.argv.slice(2);
const server = await serveRepository(resolve(fileURLToPath(new URL("..", import.meta.url))), 0);
let browser;
try {
  browser = await chromium.launch({
    channel: process.env.NOON_VIDEO_CHROMIUM_CHANNEL ?? "chromium",
    args: browserArgs("webgpu"),
  });
  const page = await browser.newPage({ viewport: { width: 960, height: 720 } });
  await page.addInitScript(() => {
    window.audioOffsets = [];
    const start = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function (...args) {
      if (this.buffer?.duration > 1) window.audioOffsets.push(args[1]);
      return start.apply(this, args);
    };
  });
  const offsets = () => page.evaluate(() => window.audioOffsets);
  const seek = (...times) => page.locator(".playback-scrubber").evaluate((slider, values) => {
    for (const value of values) {
      slider.value = String(value);
      slider.dispatchEvent(new Event("input", { bubbles: true }));
    }
    slider.dispatchEvent(new Event("change", { bubbles: true }));
  }, times);
  const waitForSeek = time => page.waitForFunction(target => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.busy === "false" && Number(controls.dataset.elapsedSeconds) === target;
  }, time);
  async function run(source) {
    const patch = page.locator("#patch-status");
    const previous = await patch.getAttribute("data-run-generation");
    if (source !== undefined) {
      await page.locator("#python-scene-source").evaluate((editor, code) => {
        editor.value = code;
        editor.dispatchEvent(new Event("input", { bubbles: true }));
      }, source);
      await page.locator("#replace-scene").click();
    }
    await page.waitForFunction(generation => {
      const patch = document.querySelector("#patch-status");
      const { runGeneration, state } = patch.dataset;
      return (generation === null || generation !== runGeneration) && ["applied", "error"].includes(state);
    }, source === undefined ? null : previous);
    assert.equal(await patch.getAttribute("data-state"), "applied", await patch.textContent());
  }

  await page.goto(server.baseUrl + "/web/index.html");
  await run();
  await page.locator("#video-file").setInputFiles(asset);
  await run(`from noon import Circle, RIGHT, Scene, UP, VideoSource
result = Scene()
media = await VideoSource.from_upload()
result.video(media, start_time=0.1, source_start=0.125, source_end=1.95, height=8)
marker = Circle(radius=0.15, fill_opacity=1).shift(UP * 3)
result.add(marker)
result.play(marker.animate.shift(RIGHT), run_time=0.4)
result.wait(1.7)
`);
  assert.equal(await page.locator(".playback-scrubber").getAttribute("max"), "2.1");
  assert.ok(await page.evaluate(() => window.__noonExampleGallery?.executionDiagnostics?.events.some(
    event => event.stage === "terminal-adopted" && event.profile === "incremental"
  )), "ordinary video playback must adopt the prepared history");

  // Delay real decoder pixels in the render worker, keeping transport, audio
  // output and the Rust clock on their normal production paths.
  const renderer = page.workers().find(worker => worker.url().endsWith("/execution-render-worker.js"));
  assert.ok(renderer);
  await renderer.evaluate(() => {
    const bitmap = globalThis.createImageBitmap;
    globalThis.videoGate = { promise: null, blocked: false };
    globalThis.createImageBitmap = async (...args) => {
      if (videoGate.promise && args[0] instanceof VideoFrame) {
        videoGate.blocked = true;
        await videoGate.promise;
      }
      return bitmap(...args);
    };
  });
  async function blockSeek() {
    await renderer.evaluate(() => {
      videoGate.blocked = false;
      videoGate.promise = new Promise(resolve => { videoGate.release = resolve; });
    });
    await seek(1.75);
    const deadline = Date.now() + 5000;
    while (!await renderer.evaluate(() => videoGate.blocked)) {
      assert.ok(Date.now() < deadline, "seek did not request an uncached video frame");
      await delay(20);
    }
  }
  const release = () => renderer.evaluate(() => {
    videoGate.promise = null;
    videoGate.release();
  });
  await blockSeek();
  await seek(1.1, 0.8, 0.6);
  assert.equal(await page.locator(".playback-controls").getAttribute("data-busy"), "true");
  assert.deepEqual(await offsets(), []);
  await release();
  await waitForSeek(0.6);
  const canvas = page.locator("#scene");
  const selected = await canvas.screenshot();
  await seek(0.25);
  await waitForSeek(0.25);
  assert.notDeepEqual(await canvas.screenshot(), selected);
  await seek(0.6);
  await waitForSeek(0.6);
  assert.deepEqual(await canvas.screenshot(), selected);
  assert.deepEqual(await offsets(), []);

  const toggle = page.locator(".playback-toggle");
  await toggle.click();
  await page.waitForFunction(() => window.audioOffsets.length > 0);
  // The fixture's AAC track starts one packet before the click at source 0.25.
  const pcmStart = 0.1 + 0.25 - 0.125 - 1024 / 48_000;
  assert.ok(Math.abs((await offsets())[0] + pcmStart - 0.6) < 0.05);
  await toggle.click();
  await blockSeek();
  await run(`from noon import Scene, Square, BLUE, RIGHT
result = Scene()
square = Square(color=BLUE, fill_opacity=1)
result.add(square)
result.play(square.animate.shift(RIGHT), run_time=2.5)
`);
  assert.equal(await page.locator(".playback-scrubber").getAttribute("max"), "2.5");
  const beforeRelease = await offsets();
  await release();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  if (await page.locator(".playback-controls").getAttribute("data-playing") === "false") await toggle.click();
  await page.waitForFunction(() => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) > 0.35);
  assert.deepEqual(await offsets(), beforeRelease, "retired soundtrack resumed after replacing the scene");
} finally {
  await browser?.close();
  await server.close();
}

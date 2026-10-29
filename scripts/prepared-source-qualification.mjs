// Actual autonomous source tasks interleave with the existing canvas/input driver.
// All source control flow, scene state and history delivery remain inside Rust.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_PREPARED_SOURCE_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/prepared-canvas/source");
await mkdir(output, { recursive: true });
const report = { status: "running", cases: [] };
let server, browser;
function equalPixels(a, b) { return a.width === b.width && a.height === b.height && a.data.equals(b.data); }
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    for (const scenario of ["complete", "failure", "cancel", "bounded", "starved"]) {
      const result = { backend, scenario, status: "running", checks: [] }; report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 } });
      page.setDefaultTimeout(60_000);
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      const state = () => page.evaluate(() => ({ playback: JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()), calls: autoCanvas.source.calls(), dropped: autoCanvas.source.producerDropped(), pump: autoCanvas.pump?.stats(), errors: autoCanvas.errors }));
      const capture = async name => {
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(output, `${backend}-${scenario}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async scenario => {
          globalThis.Worker = class { constructor() { throw new Error("source preparation introduced worker transport"); } };
          const { default: init, CooperativeCanvasFixture } = await import("./pkg/noon_web.js"); await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const { createCooperativePreparationDriver } = await import("./cooperative-preparation-driver.js");
          const source = new CooperativeCanvasFixture(scenario === "bounded" ? 12 : scenario === "starved" ? 4 : 128, scenario === "failure" ? 5 : 0xffffffff, scenario === "bounded");
          const canvas = document.querySelector("#scene");
          const renderer = await source.createCanvas(canvas.transferControlToOffscreen());
          renderer.setPreparedPlaying(false);
          const errors = [];
          let pump = null;
          const samples = [];
          let bufferedWakes = 0;
          const direct = createDirectExecutionWakeDriver(renderer, { onDemand() {
            if (JSON.parse(renderer.debugPreparedPlaybackJson()).buffering) bufferedWakes++;
            pump?.wake();
          } });
          const detach = attachNativeInputs(renderer, canvas, { inspectionZoom: true, onInput: () => direct.wake(), onError: error => errors.push(String(error)) });
          // Registering a logical view can require a fresh receipt.
          direct.wake();
          globalThis.autoCanvas = { source, renderer, errors, direct, detach, pump: null, samples,
            bufferedWakes: () => bufferedWakes,
            start() {
              const ahead = scenario === "bounded" ? 1 : scenario === "starved" ? 0.125 : 60;
              pump = this.pump = createCooperativePreparationDriver(() => source.prepareNext(renderer, ahead), {
                onProgress() {
                  const state = JSON.parse(renderer.debugPreparedPlaybackJson());
                  samples.push({ time: state.time, frontier: state.frontier, calls: source.calls() });
                  direct.wake();
                },
                onError: error => errors.push(String(error)),
              });
            } };
        }, scenario);
        assert.equal(await page.evaluate(() => autoCanvas.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        const baseline = await capture("baseline");
        assert.equal((await state()).calls, 1);
        await page.evaluate(() => autoCanvas.start());
        if (scenario === "bounded" || scenario === "starved") {
          await page.waitForFunction(() => autoCanvas.pump.stats().waiting && autoCanvas.direct.stats().idle);
          const capped = await state();
          assert.equal(capped.calls, scenario === "bounded" ? 3 : 1);
          assert.equal(capped.playback.frontier, scenario === "bounded" ? 1.5 : 1);
          assert.equal(capped.playback.time, 0.5); assert.equal(capped.playback.end, null);
          assert.equal(capped.dropped, false); assert.equal(capped.pump.active, true);
          const turns = capped.pump.turns;
          const frames = await page.evaluate(() => autoCanvas.direct.stats().presentedFrames);
          await page.waitForTimeout(160);
          assert.equal((await state()).pump.turns, turns, "look-ahead waiting must not poll");
          assert.equal((await state()).calls, capped.calls);
          assert.equal(await page.evaluate(() => autoCanvas.direct.stats().presentedFrames), frames);
          result.checks.push("bounded-ahead-while-paused", "no-preparation-or-frame-polling", "not-falsely-finished");
          if (scenario === "bounded") {
            const rect = await page.locator("#scene").boundingBox();
            await page.mouse.click(rect.x + 320, rect.y + 180);
            await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions);
            await page.waitForTimeout(100);
            assert.ok(!equalPixels(baseline, await capture("capped-highlight")));
            assert.equal((await state()).calls, capped.calls);
            await page.waitForFunction(() => !JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions && autoCanvas.direct.stats().idle && autoCanvas.pump.stats().waiting);
            assert.ok(equalPixels(baseline, await capture("capped-restored")));
            await page.evaluate(() => { autoCanvas.renderer.seekDirect(0.75); autoCanvas.direct.wake(); });
            await page.waitForFunction(() => autoCanvas.source.calls() === 4 && autoCanvas.pump.stats().waiting);
            assert.equal((await state()).playback.time, 0.75);
            assert.equal((await state()).playback.frontier, 1.75);
            await page.evaluate(() => { autoCanvas.renderer.seekDirect(0.5); autoCanvas.direct.wake(); });
            await page.waitForFunction(() => autoCanvas.pump.stats().waiting);
            assert.equal((await state()).calls, 4, "rewind must not rewind or re-execute source");
            result.checks.push("paused-highlight-while-producer-sleeps", "seek-refills-demand", "rewind-does-not-reexecute-source");
          }
          await page.evaluate(() => { autoCanvas.renderer.setPreparedPlaying(true); autoCanvas.direct.wake(); });
          if (scenario === "bounded") {
            await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).time > 1.05 && autoCanvas.source.calls() < 14);
            assert.ok(!equalPixels(baseline, await capture("playing-during-preparation")), "continuous marker animation must be visible");
            result.checks.push("continuous-playback-with-ongoing-preparation");
          }
          const endTime = scenario === "bounded" ? 4 : 2;
          await page.waitForFunction(endTime => !autoCanvas.pump.stats().active && JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).time === endTime && autoCanvas.direct.stats().idle, endTime);
          const end = await state();
          assert.equal(end.playback.end, endTime); assert.equal(end.dropped, true);
          assert.equal(end.calls, scenario === "bounded" ? 14 : 6);
          assert.equal(end.pump.failed, false); assert.deepEqual(end.errors, []);
          const samples = await page.evaluate(() => autoCanvas.samples);
          assert.ok(samples.some(sample => sample.time > 1 && sample.calls < end.calls));
          if (scenario === "bounded") {
            result.maximumPreparedLead = Math.max(...samples.map(sample => sample.frontier - sample.time));
            assert.ok(result.maximumPreparedLead <= 1.25 + 1e-9, "ahead limit plus one unknown segment");
          } else {
            assert.ok(await page.evaluate(() => autoCanvas.bufferedWakes() > 0));
            result.checks.push("long-static-wait-frontier-refill-does-not-deadlock");
          }
          await page.evaluate(() => { autoCanvas.renderer.setPreparedPlaying(false); autoCanvas.renderer.seekDirect(0.5); autoCanvas.direct.wake(); });
          assert.ok(equalPixels(baseline, await capture("bounded-rewind")));
          assert.equal((await state()).calls, end.calls);
          result.checks.push("terminal-clamp-and-release", "pixel-exact-rewind-after-completion");
        } else if (scenario === "complete") {
          await page.waitForFunction(() => autoCanvas.source.calls() >= 3 && autoCanvas.pump.stats().active);
          const rect = await page.locator("#scene").boundingBox();
          await page.mouse.move(rect.x + 320, rect.y + 180); await page.mouse.down();
          const pressed = await state();
          assert.ok(pressed.pump.active && pressed.calls < 130);
          result.callsAtPress = pressed.calls;
          const receipt = await page.evaluate(() => autoCanvas.renderer.debugPointerPresentationJson());
          await page.waitForFunction(calls => autoCanvas.source.calls() > calls + 2, pressed.calls);
          assert.equal(await page.evaluate(() => autoCanvas.renderer.debugPointerPresentationJson()), receipt);
          await page.mouse.up();
          await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions);
          await page.waitForTimeout(100);
          assert.equal((await state()).playback.time, 0.5);
          assert.ok(!equalPixels(baseline, await capture("highlight")));
          result.checks.push("automatic-typed-preparation", "press-yields-to-source-tasks", "receipt-preserved-during-delivery", "paused-visible-highlight");
          await page.waitForFunction(() => !autoCanvas.pump.stats().active && !JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions && autoCanvas.direct.stats().idle);
          const end = await state();
          assert.equal(end.calls, 130); assert.equal(end.playback.end, 33); assert.equal(end.dropped, true);
          assert.equal(end.playback.time, 0.5); assert.equal(end.pump.failed, false); assert.deepEqual(end.errors, []);
          assert.ok(equalPixels(baseline, await capture("restored")));
          const frames = await page.evaluate(() => autoCanvas.direct.stats().presentedFrames);
          await page.waitForTimeout(120);
          assert.equal(await page.evaluate(() => autoCanvas.direct.stats().presentedFrames), frames);
          await page.evaluate(() => { autoCanvas.renderer.seekDirect(33); autoCanvas.direct.wake(); });
          assert.equal((await state()).playback.time, 33);
          await page.evaluate(() => { autoCanvas.renderer.seekDirect(0.5); autoCanvas.direct.wake(); });
          assert.ok(equalPixels(baseline, await capture("rewind")));
          result.checks.push("source-completed-once", "producer-released", "terminal-after-adoption", "exact-restoration-and-rewind", "settled-without-polling");
        } else {
          if (scenario === "failure") {
            await page.waitForFunction(() => autoCanvas.pump.stats().failed);
            assert.equal((await state()).errors.length, 1); assert.equal((await state()).calls, 6);
          } else {
            await page.waitForFunction(() => autoCanvas.source.calls() >= 3 && autoCanvas.pump.stats().active);
            await page.evaluate(() => { autoCanvas.pump.stop(); autoCanvas.source.cancel(); });
          }
          const stopped = await state();
          assert.equal(stopped.dropped, true); assert.equal(stopped.playback.end, null); assert.equal(stopped.playback.time, 0.5);
          await page.waitForTimeout(120); assert.equal((await state()).calls, stopped.calls);
          const rect = await page.locator("#scene").boundingBox(); await page.mouse.click(rect.x + 320, rect.y + 180);
          await page.waitForFunction(() => JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions);
          await page.waitForFunction(() => !JSON.parse(autoCanvas.renderer.debugPreparedPlaybackJson()).interactions && autoCanvas.direct.stats().idle);
          assert.ok(equalPixels(baseline, await capture("still-interactive")));
          result.checks.push("no-source-retry", "no-false-completion", "producer-released", "retained-history-still-interactive");
        }
        assert.deepEqual(errors, []);
        result.status = "passed";
        console.log(`[PASS] cooperative ${backend}/${scenario}: ${result.checks.join(", ")}`);
      } catch (error) { result.status = "failed"; result.error = String(error); result.pageErrors = errors; throw error; }
      finally { await page.close(); }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) { report.status = "failed"; report.error = String(error); throw error; }
finally {
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
  if (browser) await browser.close(); if (server) await server.close();
}

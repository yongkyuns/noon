// Actual same-context DOM -> Rust -> GPU. No worker/scene codec on this path.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { VIEW, SHAPES, shapeSurfaceCenter, assertExactPixels, assertSelectionPixels }
  from "./pointer-selection-raster-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.env.NOON_POINTER_DIRECT_ARTIFACTS
  ? path.resolve(process.env.NOON_POINTER_DIRECT_ARTIFACTS)
  : path.join(root, "browser-smoke-artifacts/pointer-selection-direct");
const report = { scope: "direct-rust-wasm", status: "running", cases: [] };
await mkdir(output, { recursive: true });
let server, browser;
try {
  const declarations = await readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8");
  assert.match(declarations, /createDirectPointerSelectionRenderer\(/,
    "build with NOON_RENDERER_SMOKE=1; no skipped direct qualification");
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend), channel: "chromium" });
    for (const liveProgram of [false, true]) {
     for (const deviceScaleFactor of [1, 2]) {
      const mode = liveProgram ? "program" : "session", prefix = `${backend}-${mode}-dpr${deviceScaleFactor}`;
      const result = { backend, mode, deviceScaleFactor, status: "running", steps: [] };
      report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor });
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      page.setDefaultTimeout(60_000);
      const image = async name => {
        const bytes = await page.locator("#scene").screenshot({ scale: "css" });
        await writeFile(path.join(output, `${prefix}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const state = () => page.evaluate(() => ({ frame: JSON.parse(direct.renderer.debugSelectionFrameJson()),
        time: direct.renderer.time(), objects: direct.renderer.objectCount(), stats: direct.driver.stats() }));
      const settled = async () => {
        await page.waitForFunction(() => direct.driver.stats().idle);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(errors, []);
        assert.deepEqual(await page.evaluate(() => direct.errors), []);
      };
      const changed = async before => {
        // Only the production input notification may wake presentation. Do not
        // call render/advance/wake here to make a missing input wake appear to pass.
        await page.waitForFunction(count => direct.driver.stats().presentedFrames > count, before.stats.presentedFrames);
        await settled();
      };
      const click = async shape => {
        const bounds = await page.locator("#scene").boundingBox(), p = shapeSurfaceCenter(shape);
        await page.mouse.click(bounds.x + p.x, bounds.y + p.y);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async liveProgram => {
          globalThis.Worker = class { constructor() { throw new Error("direct path attempted a Worker"); } };
          const { default: init, createDirectPointerSelectionRenderer } = await import("./pkg/noon_web.js");
          await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const canvas = document.querySelector("#scene"), errors = [];
          // Keep the CSS view identical while allocating a DPR-sized backing
          // store. The collector must map in CSS coordinates; Rust still owns
          // scene-space conversion and the renderer owns physical pixels.
          canvas.width = Math.round(640 * devicePixelRatio);
          canvas.height = Math.round(360 * devicePixelRatio);
          const renderer = await createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), liveProgram, false);
          // Observe only calls that actually return from the Rust ABI. This is
          // test instrumentation, not an alternate collector or input consumer.
          const admittedInputs = [], nativePointerInput = renderer.nativePointerInput.bind(renderer);
          renderer.nativePointerInput = (...args) => {
            const result = nativePointerInput(...args);
            admittedInputs.push(args);
            return result;
          };
          const driver = createDirectExecutionWakeDriver(renderer);
          const attach = (onInput = () => driver.wake()) => attachNativeInputs(renderer, canvas, {
            onInput, onError: error => errors.push(String(error)),
          });
          globalThis.direct = { renderer, driver, attach, detach: attach(), errors, admittedInputs };
        }, liveProgram);
        await settled();
        const before = await state(), baseline = await image("baseline");
        const dimensions = await page.evaluate(() => {
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          return { dpr: devicePixelRatio, css: [rect.width, rect.height], backing: [canvas.width, canvas.height] };
        });
        result.dimensions = dimensions;
        assert.deepEqual(dimensions, { dpr: deviceScaleFactor, css: [640, 360],
          backing: [640 * deviceScaleFactor, 360 * deviceScaleFactor] });
        assert.equal(before.time, 0); assert.equal(before.objects, 3);
        assert.equal(await page.evaluate(() => direct.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        // This public fixture is authored in Python in the worker qualification
        // and through the shared Rust builder here. Compare the whole image.
        let workerBaseline = null, workerCircle = null;
        if (deviceScaleFactor === 1) {
          workerBaseline = PNG.sync.read(await readFile(path.join(root,
            `browser-smoke-artifacts/pointer-selection/${backend}-transferable-baseline.png`)));
          assertExactPixels(baseline, workerBaseline, "Rust/Python fixture baseline");
          result.steps.push({ name: "python-baseline-pixels", status: "passed" });
        }
        await page.evaluate(() => { direct.renderer.setPointerFillSelection(4); direct.driver.wake(); });
        await settled();
        let prior = await state(); await click(SHAPES[0]); await changed(prior);
        const selected = await image("circle");
        result.steps.push({ name: "circle", ...assertSelectionPixels(baseline, selected, SHAPES[0]) });
        assert.deepEqual((await state()).frame, before.frame);
        if (deviceScaleFactor === 1) {
          workerCircle = PNG.sync.read(await readFile(path.join(root,
            `browser-smoke-artifacts/pointer-selection/${backend}-transferable-circle.png`)));
          assertExactPixels(selected, workerCircle, "Rust/Python selected image");
          result.steps.push({ name: "python-selection-pixels", status: "passed" });
        }
        // Replay the same normalized trace at each DPR and inspect the actual
        // successful Rust ABI admissions, including occurrence-local CSS view.
        const tracePoint = shapeSurfaceCenter(SHAPES[0]);
        await page.evaluate(() => { direct.detach(); direct.detach = direct.attach(); direct.admittedInputs.length = 0; });
        await settled();
        await page.evaluate(({ x, y }) => {
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          direct.admittedInputs.length = 0;
          const event = (type, x, buttons) => new PointerEvent(type, { pointerId: 81,
            pointerType: "mouse", isPrimary: true, clientX: rect.left + x,
            clientY: rect.top + y, button: type === "pointermove" ? -1 : 0, buttons });
          canvas.dispatchEvent(event("pointerdown", x, 1));
          canvas.dispatchEvent(event("pointermove", x + 8, 1));
          canvas.dispatchEvent(event("pointerup", x + 8, 0));
        }, tracePoint);
        await settled();
        const trace = await page.evaluate(() => direct.admittedInputs.map(args => args.slice(0, 9)));
        assert.deepEqual(trace.map(args => args[0]), ["press", "move", "release"]);
        for (const args of trace) {
          assert.equal(args[1], trace[0][1], "one normalized contact keeps its source");
          assert.equal(args[2], 81, "pointer identity is occurrence-local");
          assert.equal(args[3], trace[0][3], "one normalized contact keeps its view revision");
          assert.equal(args[6], 640); assert.equal(args[7], 360);
        }
        assert.deepEqual(trace.map(args => args.slice(4, 6)), [
          [tracePoint.x, tracePoint.y], [tracePoint.x + 8, tracePoint.y], [tracePoint.x + 8, tracePoint.y],
        ]);
        assert.deepEqual((await state()).frame, before.frame);
        assert.equal((await state()).time, 0, "paused pointer trace must not advance authored time");
        assertExactPixels(await image("normalized-trace"), selected, "normalized selection trace");
        await page.evaluate(() => { direct.detach(); direct.detach = direct.attach(); direct.admittedInputs.length = 0; });
        await settled();
        await page.evaluate(({ x, y }) => {
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          direct.admittedInputs.length = 0;
          canvas.dispatchEvent(new PointerEvent("pointerdown", { pointerId: 82, pointerType: "mouse",
            isPrimary: true, clientX: rect.left + x, clientY: rect.top + y, button: 0, buttons: 1 }));
          canvas.dispatchEvent(new PointerEvent("pointercancel", { pointerId: 82, pointerType: "mouse", isPrimary: true }));
        }, tracePoint);
        await settled();
        assert.deepEqual(await page.evaluate(() => direct.admittedInputs.map(args => args[0])), ["press", "cancel"]);
        assert.deepEqual((await state()).frame, before.frame);
        assert.equal((await state()).time, 0, "cancelled pointer trace must not advance authored time");
        assertExactPixels(await image("normalized-cancel"), selected, "normalized cancellation trace");
        prior = await state(); await click(SHAPES[0]); await settled();
        assert.equal((await state()).stats.presentedFrames, prior.stats.presentedFrames);
        assertExactPixels(await image("repeat"), selected, "repeat selection");
        await page.evaluate(() => {
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          const event = (type, x, buttons) => new PointerEvent(type, { pointerId: 77, pointerType: "mouse", isPrimary: true,
            clientX: rect.left + x, clientY: rect.top + 320, button: type === "pointermove" ? -1 : 0, buttons });
          canvas.dispatchEvent(event("pointerdown", 20, 1));
          const samples = [event("pointermove", 60, 1), event("pointermove", 20, 1)];
          const parent = event("pointermove", 20, 1);
          Object.defineProperty(parent, "getCoalescedEvents", { value: () => samples });
          canvas.dispatchEvent(parent); canvas.dispatchEvent(event("pointerup", 20, 0));
        });
        await settled(); assertExactPixels(await image("excursion"), selected, "coalesced excursion");
        prior = await state(); await click(SHAPES[1]); await changed(prior);
        result.steps.push({ name: "rectangle", ...assertSelectionPixels(baseline, await image("rectangle"), SHAPES[1]) });
        const bounds = await page.locator("#scene").boundingBox();
        prior = await state(); await page.mouse.click(bounds.x + 20, bounds.y + 320); await changed(prior);
        assertExactPixels(await image("clear"), baseline, "background clear");
        const center = shapeSurfaceCenter(SHAPES[0]);
        await page.mouse.move(bounds.x + center.x, bounds.y + center.y); await page.mouse.down();
        await page.mouse.move(850, 550); await page.mouse.move(bounds.x + center.x, bounds.y + center.y); await page.mouse.up();
        await settled(); assertExactPixels(await image("cancel"), baseline, "cancelled re-entry");
        // No excursion: movement-threshold rejection must not mask a missing
        // cancellation. Down/up are trusted mouse events; loss events here are
        // deliberately injected (no OS capture acquisition is claimed).
        for (const [eventType, kind] of [["blur", "focus_lost"],
          ["lostpointercapture", "capture_lost"], ["pointercancel", "cancel"]]) {
          prior = await state();
          await page.evaluate(() => { direct.admittedInputs.length = 0; });
          await page.mouse.move(bounds.x + center.x, bounds.y + center.y);
          await page.mouse.down();
          const press = await page.evaluate(() => direct.admittedInputs.findLast(args => args[0] === "press"));
          assert.ok(press, "stationary press must reach Rust");
          await page.evaluate(({ eventType, pointerId }) => {
            if (eventType === "blur") window.dispatchEvent(new Event("blur"));
            else document.querySelector("#scene").dispatchEvent(new PointerEvent(eventType, {
              pointerId, pointerType: "mouse", isPrimary: true,
            }));
          }, { eventType, pointerId: press[2] });
          await page.mouse.up(); await settled();
          const admitted = await page.evaluate(() => direct.admittedInputs.map(args => args[0]));
          assert.equal(admitted.at(-1), kind, `Rust must admit stationary ${kind}`);
          assert.ok(!admitted.includes("release"), "cancelled DOM release must not become an edge");
          // A deliberate stale platform-ABI call must also fail in Rust. The
          // collector ignoring the ordinary release alone is not enough proof.
          const rejected = await page.evaluate(press => {
            const release = [...press]; release[0] = "release";
            try { direct.renderer.nativePointerInput(...release); return null; }
            catch (error) { return String(error); }
          }, press);
          assert.match(rejected ?? "", /identity\/view does not match its live source/,
            "Rust must reject a release for the cancelled source");
          assert.deepEqual((await state()).frame, before.frame);
          assert.equal((await state()).stats.presentedFrames, prior.stats.presentedFrames);
          assertExactPixels(await image(`stationary-${kind}`), baseline, `stationary ${kind}`);
          // A fresh contact must still select normally after each cancellation.
          prior = await state(); await click(SHAPES[0]); await changed(prior);
          assertExactPixels(await image(`after-${kind}`), selected, `new contact after ${kind}`);
          prior = await state(); await page.mouse.click(bounds.x + 20, bounds.y + 320); await changed(prior);
          assertExactPixels(await image(`clear-after-${kind}`), baseline, `clear after ${kind}`);
          result.steps.push({ name: `stationary-${kind}`, retiredReleaseRejected: true });
        }
        // Fatal host notification/collector errors must retire the Rust contact,
        // not just its JavaScript listeners. The scene still owns the admitted
        // press until an explicit cancellation succeeds.
        for (const fault of ["notification", "malformed-motion"]) {
          await page.evaluate(fault => {
            direct.detach();
            direct.admittedInputs.length = 0;
            direct.detach = direct.attach(fault === "notification" ? () => {
              if (direct.admittedInputs.at(-1)?.[0] === "press") {
                throw new Error("qualification notification fault");
              }
              direct.driver.wake();
            } : undefined);
          }, fault);
          await settled(); prior = await state();
          await page.mouse.move(bounds.x + center.x, bounds.y + center.y);
          await page.mouse.down();
          const press = await page.evaluate(() => direct.admittedInputs.findLast(args => args[0] === "press"));
          assert.ok(press, "failure-path press must reach Rust");
          if (fault === "malformed-motion") {
            await page.evaluate(pointerId => {
              const event = new PointerEvent("pointermove", { pointerId, pointerType: "mouse",
                isPrimary: true, button: -1, buttons: 1 });
              Object.defineProperty(event, "clientX", { value: NaN });
              document.querySelector("#scene").dispatchEvent(event);
            }, press[2]);
          }
          await page.mouse.up();
          const faults = await page.evaluate(() => direct.errors.splice(0));
          assert.equal(faults.length, 1, "report exactly the originating terminal fault");
          assert.match(faults[0], fault === "notification" ? /qualification notification fault/ : /finite/);
          const admitted = await page.evaluate(() => direct.admittedInputs.map(args => args[0]));
          assert.equal(admitted.at(-1), "cancel", "fatal input retirement must reach Rust as cancellation");
          assert.ok(!admitted.includes("release"), "retired listener must not deliver release");
          const rejected = await page.evaluate(press => {
            const release = [...press]; release[0] = "release";
            try { direct.renderer.nativePointerInput(...release); return null; }
            catch (error) { return String(error); }
          }, press);
          assert.match(rejected ?? "", /identity\/view does not match its live source/,
            "failure cleanup must retire the Rust source");
          await settled();
          assert.deepEqual((await state()).frame, before.frame);
          assert.equal((await state()).stats.presentedFrames, prior.stats.presentedFrames);
          assertExactPixels(await image(`fatal-${fault}`), baseline, `fatal ${fault}`);
          await page.evaluate(() => { direct.detach(); direct.detach = direct.attach(); });
          prior = await state(); await click(SHAPES[0]); await changed(prior);
          assertExactPixels(await image(`after-fatal-${fault}`), selected, `fresh contact after ${fault}`);
          prior = await state(); await page.mouse.click(bounds.x + 20, bounds.y + 320); await changed(prior);
          assertExactPixels(await image(`clear-after-fatal-${fault}`), baseline, `clear after ${fault}`);
          result.steps.push({ name: `fatal-${fault}`, retiredReleaseRejected: true });
        }
        await page.evaluate(() => { direct.detach(); direct.detach = direct.attach(); });
        prior = await state(); await click(SHAPES[0]); await changed(prior);
        prior = await state();
        await page.evaluate(() => { direct.renderer.setPointerFillSelection(undefined); direct.driver.wake(); });
        await changed(prior); assertExactPixels(await image("disable"), baseline, "disable");
        await click(SHAPES[1]); await settled();
        const after = await state(); assert.deepEqual(after.frame, before.frame); assert.equal(after.time, 0);
        assertExactPixels(await image("disabled-click"), baseline, "disabled click");
        // Seek is a separate explicit timeline operation. Keep the no-input-
        // mutation assertion above intact, then qualify its overlay-only clear.
        if (!liveProgram) {
          await page.evaluate(() => { direct.renderer.setPointerFillSelection(4); direct.driver.wake(); });
          await settled(); prior = await state(); await click(SHAPES[0]); await changed(prior);
          assert.deepEqual((await state()).frame, before.frame);
          prior = await state();
          const needsPresent = await page.evaluate(() => {
            const changed = direct.renderer.seekDirect(0);
            if (changed) direct.driver.wake();
            return changed;
          });
          assert.equal(needsPresent, true, "same-time seek must report its selection clear");
          await changed(prior);
          assert.equal((await state()).time, 0);
          assertExactPixels(await image("seek-clear"), baseline, "same-time seek clear");
        }
        result.status = "passed";
        console.log(`[PASS] ${prefix}: direct typed selection, DPR ${deviceScaleFactor}, normalized trace, and cancellation${deviceScaleFactor === 1 && workerBaseline && workerCircle ? ", with exact Python pixels" : ""}`);
      } catch (error) { result.status = "failed"; result.error = String(error.stack ?? error); throw error; }
      finally {
        await page.evaluate(() => { direct.detach(); direct.driver.stop(); direct.renderer.free(); }).catch(() => {});
        await page.close();
      }
     }
    }
    {
      const deviceScaleFactor = backend === "webgpu" ? 1 : 2;
      const prefix = `${backend}-rust-input-trace-dpr${deviceScaleFactor}`;
      const result = { backend, mode: "shared-rust-input-trace", deviceScaleFactor,
        status: "running", steps: [] };
      report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 800, height: 500 }, deviceScaleFactor });
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      page.setDefaultTimeout(60_000);
      const image = async name => {
        const bytes = await page.locator("#scene").screenshot({ scale: "css" });
        await writeFile(path.join(output, `${prefix}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const targetCenter = image => Array.from(image.data.subarray((180 * image.width + 320) * 4,
        (180 * image.width + 321) * 4));
      const state = () => page.evaluate(() => ({ frame: JSON.parse(direct.renderer.debugSelectionFrameJson()),
        time: direct.renderer.time(), stats: direct.driver.stats() }));
      const settled = async () => {
        await page.waitForFunction(() => direct.driver.stats().idle);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(errors, []);
        assert.deepEqual(await page.evaluate(() => direct.errors), []);
      };
      const changed = async before => {
        await page.waitForFunction(count => direct.driver.stats().presentedFrames > count,
          before.stats.presentedFrames);
        await settled();
      };
      const click = async (x, y) => {
        const bounds = await page.locator("#scene").boundingBox();
        await page.mouse.click(bounds.x + x, bounds.y + y);
      };
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async () => {
          globalThis.Worker = class { constructor() { throw new Error("direct path attempted a Worker"); } };
          const { default: init, createDirectPointerInputTraceRenderer } = await import("./pkg/noon_web.js");
          await init();
          const { attachNativeInputs } = await import("./native-inputs.js");
          const { createDirectExecutionWakeDriver } = await import("./direct-execution-wake-driver.js");
          const canvas = document.querySelector("#scene"), errors = [];
          canvas.width = Math.round(640 * devicePixelRatio);
          canvas.height = Math.round(360 * devicePixelRatio);
          const renderer = await createDirectPointerInputTraceRenderer(canvas.transferControlToOffscreen());
          const admittedInputs = [], nativePointerInput = renderer.nativePointerInput.bind(renderer);
          renderer.nativePointerInput = (...args) => {
            const result = nativePointerInput(...args);
            admittedInputs.push(args);
            return result;
          };
          const driver = createDirectExecutionWakeDriver(renderer);
          const attach = () => attachNativeInputs(renderer, canvas, {
            onInput: () => driver.wake(), onError: error => errors.push(String(error)),
          });
          globalThis.direct = { renderer, driver, attach, detach: attach(), errors, admittedInputs };
        });
        await settled();
        const before = await state(), baseline = await image("baseline");
        assert.equal(before.frame.objects.length, 2, "use the shared Rust-authored pointer fixture");
        assert.equal(await page.evaluate(() => direct.renderer.rendererBackend()),
          backend === "webgpu" ? "WebGPU" : "WebGL2");
        assert.equal(before.time, 0);

        // Target click drives both native event subscriptions and C5 selection.
        let prior = await state(); await click(320, 180); await changed(prior);
        const selected = await image("selected");
        const selectedFrame = await state();
        assert.notDeepEqual(targetCenter(selected), targetCenter(baseline),
          "Rust selection overlay should change the target center pixel");
        assert.notDeepEqual(selectedFrame.frame.objects[0].transform, before.frame.objects[0].transform,
          "the subscribed down event should update the target rotation");
        assert.notDeepEqual(selectedFrame.frame.objects[1].transform, before.frame.objects[1].transform,
          "the subscribed up event should update the unrelated rotation");
        assert.equal(selectedFrame.time, 0, "paused native input must not advance authored time");

        // Retire a held contact through actual DOM cancellation; it must not
        // publish another up edge or disturb the prior selection.
        const bounds = await page.locator("#scene").boundingBox();
        await page.mouse.move(bounds.x + 320, bounds.y + 180);
        await page.mouse.down();
        const press = await page.evaluate(() => direct.admittedInputs.findLast(args => args[0] === "press"));
        assert.ok(press, "target press must reach the shared Rust session");
        await page.evaluate(pointerId => document.querySelector("#scene").dispatchEvent(
          new PointerEvent("pointercancel", { pointerId, pointerType: "mouse", isPrimary: true })), press[2]);
        await page.mouse.up(); await settled();
        const afterCancel = await page.evaluate(() => direct.admittedInputs.map(args => args[0]));
        assert.equal(afterCancel.at(-1), "cancel");
        assert.equal(afterCancel.filter(kind => kind === "release").length, 1,
          "cancellation must not fabricate a second up event");
        const cancelledFrame = await state();
        assert.deepEqual(cancelledFrame.frame.objects[1].transform, selectedFrame.frame.objects[1].transform,
          "cancellation must not publish an up event");
        assert.equal(cancelledFrame.time, 0);
        const cancelled = await image("cancelled-contact");
        assert.deepEqual(targetCenter(cancelled), targetCenter(selected),
          "cancellation must retain the selected target");

        prior = await state(); await click(20, 20); await changed(prior);
        const cleared = await image("background-clear");
        assert.deepEqual(targetCenter(cleared), targetCenter(baseline),
          "background click should clear selection");
        const final = await state();
        assert.notDeepEqual(final.frame.objects[0].transform, selectedFrame.frame.objects[0].transform,
          "background click should publish its subscribed down event");
        assert.notDeepEqual(final.frame.objects[1].transform, selectedFrame.frame.objects[1].transform,
          "background click should publish its subscribed up event");
        assert.equal(final.time, 0);

        // The shared Rust-authored Space state trace is admitted by the browser
        // keyboard collector and native winit test. Ordered edge counts are
        // asserted by the paired native test; this renderer path proves the
        // key state drives the same paused scene signal.
        const beforeKey = await state();
        await page.keyboard.down("Space"); await changed(beforeKey);
        const afterKeyPress = await state();
        assert.equal(afterKeyPress.frame.objects[1].present, true,
          "Space keydown publishes the shared key-state binding");
        await page.keyboard.up("Space"); await changed(afterKeyPress);
        const afterKeyRelease = await state();
        assert.equal(afterKeyRelease.frame.objects[1].present, false,
          "Space keyup restores the shared key-state binding");
        assert.equal(afterKeyRelease.time, 0,
          "paused keyboard input must not advance authored time");
        result.steps.push({ name: "reactive-events-and-selection", status: "passed" },
          { name: "cancel-without-release", status: "passed" },
          { name: "background-clear", status: "passed" },
          { name: "shared-keyboard-state-parity", status: "passed" });
        result.status = "passed";
        console.log(`[PASS] ${prefix}: shared Rust fixture, reactive edges, selection, cancellation, and keyboard state`);
      } catch (error) {
        result.status = "failed"; result.error = String(error.stack ?? error); throw error;
      } finally {
        await page.evaluate(() => { direct.detach(); direct.driver.stop(); direct.renderer.free(); }).catch(() => {});
        await page.close();
      }
    }
    await browser.close(); browser = null;
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error);
  console.error(report.error); process.exitCode = 1;
} finally {
  await browser?.close(); await server?.close();
  await writeFile(path.join(output, "report.json"), JSON.stringify(report, null, 2));
}

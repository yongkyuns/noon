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
      const mode = liveProgram ? "program" : "session", prefix = `${backend}-${mode}`;
      const result = { backend, mode, status: "running", steps: [] };
      report.cases.push(result);
      const page = await browser.newPage({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      page.setDefaultTimeout(60_000);
      const image = async name => {
        const bytes = await page.locator("#scene").screenshot();
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
          const renderer = await createDirectPointerSelectionRenderer(canvas.transferControlToOffscreen(), liveProgram);
          // Observe only calls that actually return from the Rust ABI. This is
          // test instrumentation, not an alternate collector or input consumer.
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
        }, liveProgram);
        await settled();
        const before = await state(), baseline = await image("baseline");
        assert.equal(before.time, 0); assert.equal(before.objects, 3);
        assert.equal(await page.evaluate(() => direct.renderer.rendererBackend()), backend === "webgpu" ? "WebGPU" : "WebGL2");
        // This public fixture is authored in Python in the worker qualification
        // and through the shared Rust builder here. Compare the whole image.
        const workerBaseline = PNG.sync.read(await readFile(path.join(root,
          `browser-smoke-artifacts/pointer-selection/${backend}-transferable-baseline.png`)));
        assertExactPixels(baseline, workerBaseline, "Rust/Python fixture baseline");
        await page.evaluate(() => { direct.renderer.setPointerFillSelection(4); direct.driver.wake(); });
        await settled();
        let prior = await state(); await click(SHAPES[0]); await changed(prior);
        const selected = await image("circle");
        result.steps.push({ name: "circle", ...assertSelectionPixels(baseline, selected, SHAPES[0]) });
        assert.deepEqual((await state()).frame, before.frame);
        const workerCircle = PNG.sync.read(await readFile(path.join(root,
          `browser-smoke-artifacts/pointer-selection/${backend}-transferable-circle.png`)));
        assertExactPixels(selected, workerCircle, "Rust/Python selected image");
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
        console.log(`[PASS] ${prefix}: direct typed selection, autonomous wake, cancellation, clear, and exact Python pixels`);
      } catch (error) { result.status = "failed"; result.error = String(error.stack ?? error); throw error; }
      finally {
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

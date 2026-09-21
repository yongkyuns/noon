// Network-free browser DOM qualification of the production collector, not Rust
// selection or raster proof. Trusted mouse events and injected coalesced packets
// are recorded separately so synthetic histories cannot masquerade as hardware.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_POINTER_DOM_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/pointer-dom"));
const source = await readFile(path.join(root, "web/browser-pointer-input.js"), "utf8");
const report = { scope: "dom-collector-only", status: "running",
  collectorSha256: createHash("sha256").update(source).digest("hex"), cases: [] };
await mkdir(output, { recursive: true });
let browser;
try {
  browser = await playwright.chromium.launch({ headless: true,
    ...(process.env.NOON_CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.NOON_CHROMIUM_EXECUTABLE } : { channel: "chromium" }),
  });
  report.browser = browser.version();
  for (const deviceScaleFactor of [1, 2]) {
    const context = await browser.newContext({ viewport: { width: 900, height: 600 }, deviceScaleFactor });
    const run = async (name, stimulus, act, check) => {
      const result = { name, deviceScaleFactor, stimulus, status: "running" };
      report.cases.push(result);
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      try {
        await page.setContent('<style>body{margin:0}#scene{position:absolute;left:20px;top:30px;width:640px;height:360px;touch-action:none}</style><canvas id="scene" width="640" height="360"></canvas>');
        await page.evaluate(async source => {
          const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
          const { attachBrowserPointerInput } = await import(url);
          URL.revokeObjectURL(url);
          const canvas = document.querySelector("#scene");
          const controller = new AbortController();
          let sourceId = 0, view = 0, current = true;
          const events = [], faults = [], raw = [];
          const collector = attachBrowserPointerInput(canvas, {
            signal: controller.signal, isCurrent: () => current,
            send: input => events.push(input), allocateSource: () => ++sourceId,
            viewRevision: () => view, advanceView: () => ++view,
            onError: error => { faults.push(String(error)); controller.abort(); }, maxSamples: 64,
          });
          for (const type of ["pointerdown", "pointermove", "pointerup", "pointerleave"]) {
            canvas.addEventListener(type, event => raw.push({ type, trusted: event.isTrusted,
              button: event.button, buttons: event.buttons, x: event.clientX, y: event.clientY }));
          }
          const event = (type, values = {}) => new PointerEvent(type, {
            pointerId: 7, pointerType: "mouse", isPrimary: true,
            clientX: 200, clientY: 150, button: type === "pointermove" ? -1 : 0,
            buttons: type === "pointerup" ? 0 : 1, ...values,
          });
          window.pointerProbe = {
            events, faults, raw, retire: () => { current = false; },
            invalidate: () => collector.invalidateView(),
            emit: (type, values) => canvas.dispatchEvent(event(type, values)),
            batch: (samples, parentValues = {}) => {
              const rawSamples = samples.map(values => event("pointermove", values));
              const parent = event("pointermove", parentValues);
              Object.defineProperty(parent, "getCoalescedEvents", { value: () => rawSamples });
              Object.defineProperty(parent, "getPredictedEvents", { value: () => {
                throw new Error("predicted input must not be consumed");
              } });
              canvas.dispatchEvent(parent);
            },
          };
        }, source);
        await act(page);
        const state = await page.evaluate(() => ({ events: pointerProbe.events,
          faults: pointerProbe.faults, raw: pointerProbe.raw }));
        assert.deepEqual(errors, [], "no uncaught page errors");
        check(state);
        Object.assign(result, state, { status: "passed" });
        console.log(`[PASS] DOM dpr=${deviceScaleFactor} ${name}`);
      } catch (error) {
        result.status = "failed";
        result.error = String(error.stack ?? error);
        throw error;
      } finally { await page.close(); }
    };
    try {
      await run("trusted out-and-back motion", "trusted-browser-mouse", async page => {
        await page.mouse.move(200, 150); await page.mouse.down();
        await page.mouse.move(245, 150, { steps: 4 });
        await page.mouse.move(200, 150, { steps: 4 }); await page.mouse.up();
      }, ({ events, faults, raw }) => {
        assert.deepEqual(faults, []);
        assert.ok(raw.length > 0 && raw.every(event => event.trusted));
        const press = events.findIndex(event => event.kind === "press");
        const release = events.findIndex(event => event.kind === "release");
        assert.ok(press >= 0 && release > press);
        const motion = events.slice(press + 1, release);
        assert.equal(motion.length, 8);
        assert.equal(Math.max(...motion.map(event => event.surface_x)), 225);
        assert.equal(motion.at(-1).surface_x, 180);
        assert.ok(events.every(event => event.source_id === events[0].source_id));
        assert.equal(events[press].surface_y, 120, "logical CSS pixels, not device pixels");
      });
      await run("trusted button chords", "trusted-browser-mouse", async page => {
        await page.mouse.move(200, 150); await page.mouse.down();
        await page.mouse.down({ button: "right" }); await page.mouse.up({ button: "right" });
        await page.mouse.up();
      }, ({ events, faults, raw }) => {
        assert.deepEqual(faults, []);
        assert.ok(raw.every(event => event.trusted));
        assert.deepEqual(events.filter(event => event.kind !== "move").map(event => [event.kind, event.button]),
          [["press", 0], ["press", 2], ["release", 2], ["release", 0]]);
      });
      await run("trusted viewport exit", "trusted-browser-mouse", async page => {
        await page.mouse.move(200, 150); await page.mouse.down();
        await page.mouse.move(800, 500); await page.mouse.up();
      }, ({ events, faults, raw }) => {
        assert.deepEqual(faults, []);
        assert.ok(raw.every(event => event.trusted));
        assert.ok(events.some(event => event.kind === "cancel"));
        assert.equal(events.filter(event => event.kind === "release").length, 0);
      });
      await run("trusted held re-entry remains cancelled", "trusted-browser-mouse", async page => {
        await page.mouse.move(200, 150); await page.mouse.down();
        await page.mouse.move(800, 500); await page.mouse.move(200, 150); await page.mouse.up();
        await page.mouse.click(200, 150);
      }, ({ events, faults, raw }) => {
        assert.deepEqual(faults, []);
        assert.ok(raw.every(event => event.trusted));
        const edges = events.filter(event => event.kind !== "move");
        assert.deepEqual(edges.map(event => event.kind), ["press", "cancel", "press", "release"]);
        assert.ok(edges[2].source_id > edges[0].source_id);
        assert.equal(edges[2].source_id, edges[3].source_id);
      });
      await run("coalesced excursion replaces summary", "injected-coalesced-pointer-events", page =>
        page.evaluate(() => {
          pointerProbe.emit("pointerdown");
          pointerProbe.batch([{ clientX: 245, shiftKey: true }, { clientX: 200, ctrlKey: true }], { clientX: 201 });
          pointerProbe.emit("pointerup");
        }), ({ events, faults, raw }) => {
          assert.deepEqual(faults, []);
          assert.ok(raw.every(event => !event.trusted));
          assert.deepEqual(events.map(event => [event.kind, event.surface_x]),
            [["press", 180], ["move", 225], ["move", 180], ["release", 180]]);
          assert.equal(events[1].shift, true); assert.equal(events[2].control, true);
        });
      await run("malformed packet has no delivered prefix", "injected-coalesced-pointer-events", page =>
        page.evaluate(() => {
          pointerProbe.emit("pointerdown");
          pointerProbe.batch([{ clientX: 245 }, { pointerId: 9 }]);
        }), ({ events, faults }) => {
          assert.equal(events.length, 1); assert.equal(events[0].kind, "press");
          assert.equal(faults.length, 1); assert.match(faults[0], /foreign.*sample/);
        });
      await run("oversized packet fails before expansion", "injected-coalesced-pointer-events", page =>
        page.evaluate(() => {
          pointerProbe.emit("pointerdown"); pointerProbe.batch(Array.from({ length: 65 }, () => ({})));
        }), ({ events, faults }) => {
          assert.equal(events.length, 1); assert.equal(faults.length, 1);
          assert.match(faults[0], /capacity/);
        });
      await run("view change cancels rather than releasing", "injected-pointer-events-real-layout", page =>
        page.evaluate(() => {
          pointerProbe.emit("pointerdown");
          document.querySelector("#scene").style.width = "600px";
          pointerProbe.emit("pointerup");
        }), ({ events, faults }) => {
          assert.deepEqual(faults, []);
          assert.deepEqual(events.map(event => event.kind), ["press", "cancel"]);
        });
      await run("retired attachment does not inspect packets", "injected-coalesced-pointer-events", page =>
        page.evaluate(() => {
          pointerProbe.emit("pointerdown"); pointerProbe.retire();
          pointerProbe.batch(Array.from({ length: 65 }, () => ({}))); pointerProbe.emit("pointerup");
        }), ({ events, faults }) => {
          assert.equal(events.length, 1); assert.deepEqual(faults, []);
        });
    } finally { await context.close(); }
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error.stack ?? error);
  process.exitCode = 1;
  console.error(report.error);
} finally {
  await browser?.close();
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
}

import assert from "node:assert/strict";
import { mkdir, writeFile, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { referenceGlow, maxChannelError } from "./glow-worker-reference.mjs";
import { encodeGlowWorkerReport } from "./glow-worker-report.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.resolve(root, process.env.NOON_GLOW_WORKER_ARTIFACTS ?? "browser-smoke-artifacts/glow-worker");
await mkdir(out, { recursive: true });
const report = { status: "failed", scope: "real retained render-worker transport/pixels; public Scene/Python activation not qualified",
  toleranceBytes: 2, modes: [], frames: 0, maxError: 0 };
const server = await serveRepository(root, Number(process.env.NOON_GLOW_WORKER_PORT ?? 4198), { crossOriginIsolated: true });
let browser;
try {
  const executablePath = process.env.NOON_BROWSER_EXECUTABLE;
  browser = await playwright.chromium.launch({ headless: true,
    ...(executablePath ? { executablePath } : { channel: "chromium" }), args: browserArgs("webgpu") });
  report.browser = browser.version();
  report.wasmSha256 = createHash("sha256").update(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm"))).digest("hex");
  for (const mode of ["transferable", "shared"]) {
    const page = await browser.newPage({ viewport: { width: 800, height: 500 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      const description = await page.evaluate(async mode => {
        const { default: init, glowWorkerSmokeFixture } = await import("./pkg/noon_web.js");
        await init();
        const fixture = JSON.parse(glowWorkerSmokeFixture());
        const { TransferableExecutionDeltaSender, SharedExecutionDeltaWriter, createSharedExecutionMailbox } = await import("./execution-transport.js");
        const endpoints = new Map();
        async function create(id, width, height) {
          document.getElementById(id)?.remove();
          const canvas = document.createElement("canvas");
          canvas.id = id; canvas.width = width; canvas.height = height;
          canvas.style.cssText = `display:block;width:${width}px;height:${height}px`;
          document.body.append(canvas);
          const offscreen = canvas.transferControlToOffscreen();
          const worker = new Worker("./authoring-render-worker.js", { type: "module" });
          const ports = new MessageChannel();
          const state = { worker, port: ports.port1, errors: [], pending: new Map(), requests: new Map(), id: 1, ready: null };
          const fail = error => {
            const message = String(error.message ?? error);
            state.errors.push(message);
            for (const pending of state.pending.values()) pending.reject(new Error(message));
            state.pending.clear();
          };
          worker.addEventListener("error", event => { event.preventDefault(); fail(event); });
          worker.addEventListener("message", ({ data }) => {
            if (data.type === "error") { fail(data); return; }
            if (data.type === "ready") state.ready = data;
            if (state.requests.has(data.requestId)) {
              const p = state.requests.get(data.requestId); state.requests.delete(data.requestId); p.resolve(data);
            }
          });
          state.port.addEventListener("message", ({ data }) => {
            if (data.type === "render_error") { fail(data); return; }
            if (data.type === "execution_presented") {
              const key = `${data.session}:${data.sequence}`;
              const p = state.pending.get(key);
              if (p) { state.pending.delete(key); p.resolve(data); }
            }
          });
          state.port.start();
          worker.postMessage({ channel: "noon.render", protocolVersion: 1, type: "init",
            mode: "retained", transportMode: mode, canvas: offscreen, width, height, port: ports.port2 }, [offscreen, ports.port2]);
          if (mode === "shared") {
            const mailbox = createSharedExecutionMailbox(1024 * 1024);
            state.sender = new SharedExecutionDeltaWriter(mailbox);
            state.port.postMessage({ type: "transport_setup", mode, mailbox });
          } else { state.sender = new TransferableExecutionDeltaSender(state.port); }
          const resources = new Uint8Array(fixture.resources);
          state.port.postMessage({ type: "retained_resources", bytes: resources }, [resources.buffer]);
          endpoints.set(id, state);
        }
        async function send(id, delta) {
          const state = endpoints.get(id), key = `${delta.session}:${delta.sequence}`;
          if (state.errors.length) throw new Error(state.errors.join("\n"));
          const promise = new Promise((resolve, reject) => {
            const timer = setTimeout(() => { state.pending.delete(key); reject(new Error(`worker presentation timeout: ${id} ${key}`)); }, 30000);
            state.pending.set(key, { resolve: data => { clearTimeout(timer); resolve(data); }, reject: error => { clearTimeout(timer); reject(error); } });
          });
          if (!state.sender.send(JSON.stringify(delta))) throw new Error("unexpected fixture backpressure");
          if (mode === "shared") state.port.postMessage({ type: "shared_delta" });
          return promise;
        }
        async function metrics(id) {
          const state = endpoints.get(id), requestId = state.id++;
          const promise = new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("metrics timeout")), 10000);
            state.requests.set(requestId, { resolve: data => { clearTimeout(timer); resolve(data); } });
          });
          state.worker.postMessage({ channel: "noon.render", protocolVersion: 1, type: "metrics", requestId, includeGpuIdentity: true });
          return promise;
        }
        function dispose(id) { const state = endpoints.get(id); state.worker.terminate(); state.port.close(); endpoints.delete(id); }
        window.glowTest = { fixture, endpoints, create, send, metrics, dispose };
        await create("scene", fixture.width, fixture.height);
        await create("reference", fixture.width + 2 * fixture.padding, fixture.height + 2 * fixture.padding);
        await create("ordinary", fixture.width, fixture.height);
        return { width: fixture.width, height: fixture.height, padding: fixture.padding,
          cases: fixture.cases.map(c => ({ name: c.name, count: c.frames.length })), isolated: crossOriginIsolated };
      }, mode);
      assert.equal(description.isolated, true);
      assert.equal(description.cases.length, 5);
      const image = async (id, filename) => {
        const bytes = await page.locator(`#${id}`).screenshot({ timeout: 15000 });
        if (filename) await writeFile(path.join(out, filename), bytes);
        return PNG.sync.read(bytes);
      };
      const modeReport = { mode, cases: [], maxError: 0 };
      for (let ci = 0; ci < description.cases.length; ci++) {
        const { name, count } = description.cases[ci];
        assert.equal(count, 5);
        let original, lastReference;
        const caseReport = { name, errors: [], rewindExact: false };
        for (let fi = 0; fi < count; fi++) {
          const sample = await page.evaluate(async ({ ci, fi }) => {
            const sample = glowTest.fixture.cases[ci].frames[fi];
            await glowTest.send("scene", sample.delta);
            const { references, delta, ...parameters } = sample;
            return { ...parameters, time: delta.time, referenceCount: references.length };
          }, { ci, fi });
          const actual = await image("scene", `${mode}-${name}-${fi}.png`);
          if (fi === 0) original = actual.data;
          if (fi === 4) { assert.deepEqual(actual.data, original, `${name}: exact worker rewind`); caseReport.rewindExact = true; }
          const references = [];
          for (let ri = 0; ri < sample.referenceCount; ri++) {
            await page.evaluate(async ({ ci, fi, ri }) => glowTest.send("reference", glowTest.fixture.cases[ci].frames[fi].references[ri]), { ci, fi, ri });
            references.push(await image("reference"));
          }
          const expected = referenceGlow(references, sample, description);
          const error = maxChannelError(actual.data, expected.data);
          await writeFile(path.join(out, `${mode}-${name}-${fi}-expected.png`), PNG.sync.write(expected));
          caseReport.errors.push(error); report.frames++; report.maxError = Math.max(report.maxError, error);
          modeReport.maxError = Math.max(modeReport.maxError, error);
          assert.ok(error <= report.toleranceBytes, `${mode} ${name} t=${sample.time}: full-image error ${error} > 2`);
          if (fi === 0) assert.ok(expected.haloSignal > 2, `${name}: no visible halo / ineffective missing-glow negative control`);
          lastReference = expected;
        }
        await page.evaluate(async ci => {
          const fixture = glowTest.fixture.cases[ci];
          await glowTest.send("scene", fixture.neutral);
          await glowTest.send("ordinary", fixture.ordinary);
        }, ci);
        const neutral = await image("scene", `${mode}-${name}-neutral.png`);
        const ordinary = await image("ordinary");
        assert.deepEqual(neutral.data, ordinary.data, `${name}: neutral must match actual ordinary worker pixels exactly`);
        caseReport.neutralExact = true;
        await page.evaluate(async ci => glowTest.send("scene", glowTest.fixture.cases[ci].removed), ci);
        const removed = await image("scene", `${mode}-${name}-removed.png`);
        caseReport.removalError = maxChannelError(removed.data, lastReference.removed);
        assert.ok(caseReport.removalError <= 2, `${name}: stale halo after removal`);
        modeReport.cases.push(caseReport);
        console.log(`PASS ${mode} ${name}: five actual worker frames, exact rewind and source removal; max error ${Math.max(...caseReport.errors)}`);
      }
      // A new worker and new surface must reproduce the original snapshot, not reuse stale GPU caches.
      await page.evaluate(async () => {
        glowTest.dispose("scene");
        await glowTest.create("scene", glowTest.fixture.width, glowTest.fixture.height);
        await glowTest.send("scene", glowTest.fixture.cases[0].frames[0].delta);
      });
      const recovered = await image("scene", `${mode}-recreated.png`);
      const first = PNG.sync.read(await readFile(path.join(out, `${mode}-circle-motion-0.png`)));
      assert.deepEqual(recovered.data, first.data, "worker recreation must reproduce initial pixels");
      modeReport.recreationExact = true;
      const beforeInvalid = await page.evaluate(() => glowTest.metrics("scene"));
      const rejected = await page.evaluate(async mode => {
        const state = glowTest.endpoints.get("scene");
        const invalid = structuredClone(glowTest.fixture.cases[0].frames[1].delta);
        const row = invalid.objects.find(row => row.glow);
        if (!row) throw new Error("negative control lacks a glow row");
        row.glow.intensity = 99;
        const count = state.errors.length;
        if (!state.sender.send(JSON.stringify(invalid))) throw new Error("negative control unexpectedly backpressured");
        if (mode === "shared") state.port.postMessage({ type: "shared_delta" });
        const deadline = performance.now() + 10000;
        while (state.errors.length === count) {
          if (performance.now() > deadline) throw new Error("invalid glow was not rejected");
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        return state.errors.splice(count);
      }, mode);
      assert.ok(rejected.length > 0 && rejected.every(message => /glow/i.test(message)), JSON.stringify(rejected));
      const afterInvalid = await page.evaluate(() => glowTest.metrics("scene"));
      assert.ok(Number.isSafeInteger(beforeInvalid.metrics.presentedFrames) && beforeInvalid.metrics.presentedFrames > 0);
      assert.ok(Number.isFinite(beforeInvalid.metrics.time));
      assert.equal(afterInvalid.metrics.presentedFrames, beforeInvalid.metrics.presentedFrames);
      assert.equal(afterInvalid.metrics.time, beforeInvalid.metrics.time);
      assert.deepEqual((await image("scene", `${mode}-invalid-preserved.png`)).data, recovered.data,
        "invalid glow payload must leave the displayed frame intact");
      modeReport.invalidPreserved = { errors: rejected, unchangedPresentations: true, pixelsExact: true };
      modeReport.metrics = afterInvalid;
      assert.ok(modeReport.metrics.metrics.presentedFrames > 0);
      assert.deepEqual(await page.evaluate(() => [...glowTest.endpoints.values()].flatMap(x => x.errors)), []);
      assert.deepEqual(errors, []);
      report.modes.push(modeReport);
    } finally { await page.close(); }
  }
  assert.equal(report.frames, 50);
  assert.equal(report.modes.length, 2);
  report.status = "passed";
} catch (error) {
  report.error = String(error.stack ?? error);
  throw error;
} finally {
  await writeFile(path.join(out, "report.json"), encodeGlowWorkerReport(report));
  await browser?.close(); await server.close();
}

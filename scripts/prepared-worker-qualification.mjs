// Actual prepared-reader publication to the existing production render worker.
// Source/history remain typed in the producing WASM context. Only the existing
// retained renderer wire protocol crosses the MessagePort/shared mailbox.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceKind = process.env.NOON_PREPARED_WORKER_SOURCE ?? "rust";
assert.ok(["rust", "python"].includes(sourceKind), "unknown prepared-worker source");
const workerSource = sourceKind === "python" ? await readFile(path.join(root, "web/python-worker.source.js"), "utf8") : null;
const pyodideUrl = workerSource?.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
if (sourceKind === "python") assert.ok(pyodideUrl, "use the product's pinned interpreter");
const modules = sourceKind === "python" ? await Promise.all(PYTHON_COMPAT_MODULES.map(async ({sourcePath, runtimePath}) => ({
  runtimePath, source: await readFile(path.join(root, "web", sourcePath), "utf8"),
}))) : [];
const cache = workerSource ? createPyodideResourceCache(workerSource) : null;
const out = process.env.NOON_PREPARED_WORKER_ARTIFACTS ?? path.join(root, `browser-smoke-artifacts/prepared-worker${sourceKind === "python" ? "/python" : ""}`);
await mkdir(out, { recursive: true });
const report = { status: "running", sourceKind, scope: "prepared reader to production render worker, explicit source/tick fixture, not automatic Python worker adoption", cases: [] };
let server, browser;
function pixels(a, b) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let count = 0;
  for (let i = 0; i < a.data.length; i += 4) if (!a.data.subarray(i, i + 4).equals(b.data.subarray(i, i + 4))) count++;
  return count;
}
try {
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of ["webgpu", "webgl"]) {
    browser = await playwright.chromium.launch({ headless: true,
      ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : { channel: "chromium" }),
      args: browserArgs(backend) });
    for (const mode of ["transferable", "shared"]) {
      const result = { backend, mode, status: "running", checks: [] }; report.cases.push(result);
      const page = await browser.newPage({ viewport: { width:900, height:600 } });
      page.setDefaultTimeout(sourceKind === "python" ? 180_000 : 60_000);
      if (cache) await cache.install(page.context());
      const errors = []; page.on("pageerror", error => errors.push(String(error)));
      const capture = async name => {
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const bytes = await page.locator("#scene").screenshot();
        await writeFile(path.join(out, `${backend}-${mode}-${name}.png`), bytes);
        return PNG.sync.read(bytes);
      };
      const state = () => page.evaluate(() => ({ ...JSON.parse(preparedWorker.player.debugPreparedPlaybackJson()),
        sent:preparedWorker.sent, presented:preparedWorker.presented, receipt:preparedWorker.receipt,
        errors:preparedWorker.errors, outcomes:preparedWorker.outcomes }));
      const click = () => page.mouse.click(320, 180);
      const step = timestamp => page.evaluate(timestamp => preparedWorker.send(preparedWorker.player.tickDeltaJson(timestamp)), timestamp);
      try {
        await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
        const ready = await page.evaluate(async ({mode, sourceKind, modules, pyodideUrl}) => {
          const wasm = await import("./pkg/noon_web.js");
          await wasm.default();
          const { attachBrowserPointerInput } = await import("./browser-pointer-input.js");
          const { replaceExecutionCanvas } = await import("./execution-canvas.js");
          const { SharedExecutionDeltaWriter, TransferableExecutionDeltaSender, createSharedExecutionMailbox,
            executionDeltaMetadata } = await import("./execution-transport.js");
          // Python mode uses the public canonical context factory, not a smoke
          // fixture, a second scene representation or a serialized prefix.
          const source = sourceKind === "python"
            ? await (await import("../scripts/prepared-worker-python-source.mjs")).createPythonSource(wasm, modules, pyodideUrl)
            : new wasm.PreparedCanvasFixture();
          const player = source.createWorkerPlayer(710);
          player.setPreparedPlaying(false);
          let receiverSession = 710, sourceId = 0, view = 1;
          const attachReceiver = async (replacement = false) => {
          const previous = replacement ? globalThis.preparedWorker : null;
          if (previous) {
            previous.abort.abort(); previous.worker.terminate(); previous.port.close();
            replaceExecutionCanvas(document.querySelector("#scene"));
            player.resetReceiver(++receiverSession);
          }
          const canvas = document.querySelector("#scene"), rect = canvas.getBoundingClientRect();
          player.setBrowserPointerViewJson(JSON.stringify({ revision:view, width:rect.width, height:rect.height }));
          const ports = new MessageChannel(), worker = new Worker("./authoring-render-worker.js", { type:"module" });
          const host = { source:previous ? previous.source : source, player, worker, port:ports.port1, receipt:null, authorize:previous?.authorize ?? false,
            errors:[], outcomes:[], sent:0, presented:0, pending:new Map(), requests:new Map(), nextRequest:1, ready:null };
          globalThis.preparedWorker = host;
          const fail = message => {
            host.errors.push(String(message));
            for (const pending of host.pending.values()) pending.reject(new Error(String(message)));
            host.pending.clear();
          };
          worker.addEventListener("error", event => { event.preventDefault(); fail(event.message); });
          worker.addEventListener("message", ({data}) => {
            if (data.type === "error") { fail(data.message); return; }
            if (data.type === "ready") host.ready = data;
            const pending = host.requests.get(data.requestId);
            if (pending) { host.requests.delete(data.requestId); pending.resolve(data); }
          });
          host.port.addEventListener("message", ({data}) => {
            if (data.type === "render_error") { fail(data.message); return; }
            if (data.type === "execution_presented") {
              host.presented++;
              host.receipt = data.pointerReceipt ?? null;
              if (host.authorize && host.receipt) player.notePointerPresentationJson(JSON.stringify(host.receipt));
              const pending = host.pending.get(data.sequence);
              if (pending) { host.pending.delete(data.sequence); pending.resolve(data); }
            }
            if (data.type === "pointer_presentation_invalidated") {
              player.invalidatePointerPresentationJson(JSON.stringify(data.receipt)); host.receipt = null;
            }
          });
          host.port.start();
          const offscreen = canvas.transferControlToOffscreen();
          worker.postMessage({ channel:"noon.render", protocolVersion:1, type:"init", mode:"retained", transportMode:mode,
            canvas:offscreen, width:canvas.width, height:canvas.height, port:ports.port2 }, [offscreen,ports.port2]);
          const resources = new Uint8Array(player.resourceBundleBytes());
          host.port.postMessage({ type:"retained_resources", bytes:resources }, [resources.buffer]);
          const mailbox = mode === "shared" ? createSharedExecutionMailbox(1024 * 1024) : null;
          host.sender = mailbox ? new SharedExecutionDeltaWriter(mailbox) : new TransferableExecutionDeltaSender(host.port);
          if (mailbox) host.port.postMessage({ type:"transport_setup",mode,mailbox });
          host.port.postMessage({ type:"execution_wake", cadence:"idle",timerAfterMilliseconds:null });
          host.send = async json => {
            if (json == null) return null;
            if (host.errors.length) throw new Error(host.errors.join("\n"));
            const metadata = executionDeltaMetadata(json);
            const promise = new Promise((resolve,reject) => {
              const timer = setTimeout(() => { host.pending.delete(metadata.sequence); reject(new Error("prepared worker presentation timeout")); }, 30_000);
              host.pending.set(metadata.sequence, { resolve:value => { clearTimeout(timer); resolve(value); }, reject:error => { clearTimeout(timer); reject(error); } });
            });
            if (!host.sender.send(json)) throw new Error("fixture attempted drain while transport was backpressured");
            host.sent++;
            if (mode === "shared") host.port.postMessage({type:"shared_delta"});
            return promise;
          };
          host.metrics = () => {
            const requestId = host.nextRequest++;
            const promise = new Promise((resolve,reject) => {
              const timer = setTimeout(() => { host.requests.delete(requestId); reject(new Error("prepared worker metrics timeout")); }, 10_000);
              host.requests.set(requestId,{resolve:response => { clearTimeout(timer); resolve(response.metrics); }});
            });
            worker.postMessage({channel:"noon.render",protocolVersion:1,type:"metrics",requestId,includeGpuIdentity:true});
            return promise;
          };
          const abort = new AbortController();
          host.abort = abort;
          host.collector = attachBrowserPointerInput(canvas, { signal:abort.signal, isCurrent:() => true,
            maxSamples:64, allocateSource:() => ++sourceId, viewRevision:() => view, advanceView:() => { view++; },
            registeredView:{ ...rect.toJSON(),devicePixelRatio },
            onView:(revision,width,height) => player.setBrowserPointerViewJson(JSON.stringify({revision,width,height})),
            send:input => {
              const admitted = player.submitBrowserPointerInputJson(JSON.stringify({input,presentation:host.receipt}));
              host.outcomes.push({kind:input.kind,admitted}); return admitted;
            }, onError:fail,
          });
          // Bootstrap resources first; the first selected snapshot may be a seek.
          await host.send(sourceKind === "python" && !replacement ? player.seekDeltaJson(0.5) : player.initialDeltaJson());
          return { isolated:crossOriginIsolated, metrics:await host.metrics(), receipt:host.receipt };
          };
          globalThis.recoverPreparedReceiver = () => attachReceiver(true);
          return attachReceiver();
        }, { mode, sourceKind, modules, pyodideUrl });
        assert.equal(ready.isolated, true);
        assert.ok(ready.receipt, "actual renderer must return a presented-frame receipt");
        const metrics = ready.metrics;
        result.initialMetrics = metrics;
        assert.equal(metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
        await click();
        let current = await state();
        assert.equal(current.interactions, false);
        assert.ok(current.outcomes.some(item => item.kind === "press" && !item.admitted));
        await page.evaluate(() => { preparedWorker.authorize = true; preparedWorker.player.notePointerPresentationJson(JSON.stringify(preparedWorker.receipt)); });
        const baseline = await capture("baseline");
        assert.ok(baseline.data.some((byte,i) => i % 4 === 2 && byte > 200), "real scene must contain blue pixels");
        result.checks.push("actual-worker-backend", "consumption-is-not-presentation-authorization");
        await click(); await step(1_000); await step(1_250);
        result.midpointPixels = pixels(baseline, await capture("highlight"));
        assert.ok(result.midpointPixels > 500);
        assert.equal((await state()).time, 0.5);
        await step(1_500);
        assert.equal(pixels(baseline, await capture("restored")), 0);
        result.checks.push("paused-click-crosses-worker", "pixel-exact-restoration");

        await page.mouse.move(320,180); await page.mouse.down();
        const before = await state();
        await page.evaluate(() => preparedWorker.source.publishNextToWorker(preparedWorker.player,false));
        assert.equal(await page.evaluate(() => preparedWorker.player.drainDeltaJson()), undefined);
        const delivered = await state();
        assert.equal(delivered.frontier,2); assert.equal(delivered.time,0.5);
        assert.deepEqual(delivered.receipt,before.receipt); assert.equal(delivered.sent,before.sent);
        assert.equal(pixels(baseline,await capture("future-appended")),0);
        await page.mouse.up(); await step(2_000); await step(2_250);
        assert.ok(pixels(baseline,await capture("historical-release")) > 500);
        await step(2_500);
        assert.equal(pixels(baseline,await capture("historical-release-restored")),0);
        result.checks.push("future-history-sends-no-render-update", "pending-press-survives-delivery");

        await page.mouse.move(320,180); await page.mouse.down();
        const oldReceiver = await state();
        const recovered = await page.evaluate(() => recoverPreparedReceiver());
        await page.mouse.up();
        assert.equal(recovered.receipt.session, oldReceiver.receipt.session + 1);
        assert.equal(recovered.receipt.sequence, 0);
        assert.equal(recovered.metrics.backend, metrics.backend);
        assert.equal((await state()).time, oldReceiver.time);
        assert.equal((await state()).frontier, oldReceiver.frontier);
        assert.equal((await state()).interactions, false);
        assert.equal(await page.evaluate(receipt => preparedWorker.player.notePointerPresentationJson(JSON.stringify(receipt)), oldReceiver.receipt), false);
        assert.equal(pixels(baseline, await capture("receiver-recovered-open")), 0);
        assert.equal(await page.evaluate(() => preparedWorker.source.producerAlive()), true);
        result.checks.push("terminated-renderer-replaced-around-original-reader", "open-history-source-delivery-preserved", "old-contact-and-receipt-retired");

        const beforeTerminal = await state();
        await page.evaluate(() => preparedWorker.source.publishNextToWorker(preparedWorker.player,true));
        assert.equal(await page.evaluate(() => preparedWorker.source.producerAlive()),false);
        assert.equal(await page.evaluate(() => preparedWorker.player.drainDeltaJson()),undefined);
        assert.equal((await state()).end,3); assert.deepEqual((await state()).receipt,beforeTerminal.receipt);
        await page.evaluate(() => { preparedWorker.source.free(); preparedWorker.source=null; });
        await click(); await step(3_000); await step(3_250);
        assert.ok(pixels(baseline,await capture("source-destroyed-highlight")) > 500);
        await step(3_500);
        assert.equal(pixels(baseline,await capture("source-destroyed-restored")),0);
        await page.evaluate(() => preparedWorker.send(preparedWorker.player.seekDeltaJson(3)));
        assert.equal((await state()).time,3);
        const terminalPixels = await capture("terminal");
        assert.ok(pixels(baseline,terminalPixels) > 500);
        await page.evaluate(() => preparedWorker.send(preparedWorker.player.seekDeltaJson(0.5)));
        assert.equal(pixels(baseline,await capture("rewind")),0);
        result.checks.push("terminal-delivery-preserves-display", "source-destruction-independent", "exact-final-frame-and-rewind");
        const completed = await state();
        const recoveredCompleted = await page.evaluate(() => recoverPreparedReceiver());
        assert.equal(recoveredCompleted.receipt.session, completed.receipt.session + 1);
        assert.equal((await state()).time, 0.5); assert.equal((await state()).end, 3);
        assert.equal(pixels(baseline,await capture("receiver-recovered-completed")),0);
        await page.evaluate(() => preparedWorker.send(preparedWorker.player.seekDeltaJson(3)));
        assert.equal(pixels(terminalPixels,await capture("receiver-recovered-terminal")),0);
        await page.evaluate(() => preparedWorker.send(preparedWorker.player.seekDeltaJson(0.5)));
        assert.equal(pixels(baseline,await capture("receiver-recovered-rewind")),0);
        result.checks.push("recovery-after-source-destruction", "recovered-endpoint-and-rewind-exact-pixels");
        const idle = await state(); await page.waitForTimeout(160);
        const settled = await state(); assert.equal(settled.sent,idle.sent); assert.equal(settled.presented,idle.presented);
        assert.deepEqual(settled.errors,[]); assert.deepEqual(errors,[]);
        result.checks.push("settled-channel-idle"); result.status="passed";
      } catch(error) {
        result.error=String(error.stack ?? error);
        result.state = await state().catch(() => null);
        await page.screenshot({path:path.join(out,`${backend}-${mode}-failure.png`)}).catch(() => {});
        throw error;
      }
      finally { await page.close(); }
    }
    await browser.close(); browser=null;
  }
  report.status="passed";
} finally {
  await writeFile(path.join(out,"report.json"),JSON.stringify(report, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));
  await browser?.close(); await server?.close();
}
console.log(JSON.stringify(report, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));

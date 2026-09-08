import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const server = await serveRepository(root, Number(process.env.NOON_RETAINED_EXECUTION_WORKER_PORT ?? 4181), { crossOriginIsolated: true });
const source = `from noon import *
scene = Scene()
scene.add(
    Circle(0.65).shift(2 * LEFT + 0.6 * UP),
    Typst("*Hello* from _Typst!_", font_size=64).shift(1.1 * UP),
    Rectangle(width=1.5, height=0.9).shift(2 * RIGHT + 0.6 * UP),
    Line(1.2 * LEFT, 1.2 * RIGHT).shift(1.5 * LEFT + 1.4 * DOWN),
    MathTypst("frac(x, 2)", font_size=72).set_color(YELLOW).set_opacity(0.9).shift(DOWN),
    Square(0.8).shift(1.5 * RIGHT + 1.4 * DOWN),
)
result = scene
`;
let browser;
const reports = [];
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  for (const transportMode of ["transferable", "shared"]) {
    const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    let timer;
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      const result = await Promise.race([
        page.evaluate(async ({ source, transportMode }) => {
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
          const authoring = new PythonAuthoringClient();
          const clientErrors = [];
          const client = new ExecutionWorkerClient(document.querySelector("#scene"), {
            onError: error => clientErrors.push(String(error)),
          });
          async function context(code) { return (await authoring.run(code, {})).semanticExecution.contextId; }
          async function snapshot() {
            await client.advanceTo(0.75);
            return { ...(await client.metrics()), state: await client.state() };
          }
          try {
            // Prepare the real renderer before Python authoring finishes. No
            // execution snapshot may be published until the shared context attaches.
            const prepared = client.prepare({ transportMode, sharedSlotCapacity: 1024 * 1024 });
            const initialContext = await context(source);
            await prepared;
            const ready = await client.startSemanticExecution(initialContext, authoring, { initiallyPaused: true });
            const initial = await snapshot();
            const canvas = client.canvas;
            const engineReady = await client.restart({ failedOwner: "engine" });
            const reconnected = await snapshot();
            const engineKeptCanvas = client.canvas === canvas;
            // A source replacement uses the existing retained renderer and shared
            // semantic context preflight; no geometry/text engine-mode switch.
            await client.switchToSemanticExecution(await context("from noon import *\nscene = Scene()\nscene.add(Circle(0.5))\nresult = scene"), authoring);
            const geometry = await snapshot();
            await client.switchToSemanticExecution(await context(source), authoring);
            const mixed = await snapshot();
            const switchingKeptCanvas = client.canvas === canvas;
            const recoveryReady = await client.restart({ failedOwner: "render" });
            const recovered = await snapshot();
            return { ready, initial, engineReady, reconnected, geometry, mixed, recoveryReady, recovered,
              engineKeptCanvas, switchingKeptCanvas, renderReplacedCanvas: client.canvas !== canvas,
              mode: client.mode, crossOriginIsolated, clientErrors };
          } finally { client.terminate(); authoring.terminate(); }
        }, { source, transportMode }),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${transportMode} shared recovery timed out`)), 90000); }),
      ]);
      assert.equal(result.crossOriginIsolated, true);
      assert.equal(result.mode, "semantic");
      assert.equal(result.ready.transportMode, transportMode);
      assert.equal(result.engineReady.transportMode, transportMode);
      assert.equal(result.recoveryReady.transportMode, transportMode);
      assert.ok(result.engineKeptCanvas && result.switchingKeptCanvas && result.renderReplacedCanvas);
      for (const [stage, count] of [["initial", 6], ["reconnected", 6], ["geometry", 1], ["mixed", 6], ["recovered", 6]]) {
        const { metrics, state } = result[stage];
        assert.equal(metrics.objectCount, count, stage);
        assert.ok(metrics.presentedFrames > 0 && metrics.drawCalls > 0 && metrics.instancesDrawn > 0, stage);
        assert.equal(metrics.resourceBundlePending, false, stage);
        assert.equal(state.playing, false, `${stage} preserves paused state`);
        assert.equal(state.time, 0.75, `${stage} presents the requested authored time`);
        assert.ok(state.sceneJson == null);
        assert.equal(state.sceneSpecJson, undefined);
      }
      assert.ok(result.reconnected.metrics.presentedFrames >= result.initial.metrics.presentedFrames);
      assert.ok(result.mixed.metrics.presentedFrames >= result.reconnected.metrics.presentedFrames);
      assert.deepEqual(result.clientErrors, []);
      assert.deepEqual(errors, []);
      reports.push({ transportMode, ...result });
      console.log(`PASS ${transportMode}: shared mixed scene, prepared startup, 6→1→6 replacement, engine reconnect and renderer recovery`);
    } finally { clearTimeout(timer); await page.close(); }
  }
  if (process.env.NOON_RETAINED_EXECUTION_WORKER_REPORT) {
    const artifact = path.resolve(process.env.NOON_RETAINED_EXECUTION_WORKER_REPORT);
    await mkdir(path.dirname(artifact), { recursive: true });
    await writeFile(artifact, JSON.stringify(reports, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));
  }
} finally { await browser?.close(); await server.close(); }

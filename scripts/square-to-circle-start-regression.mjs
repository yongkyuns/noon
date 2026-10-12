import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { browserArgs } from "./manim-raster-support.mjs";
import { serveRepository } from "./browser-test-server.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_SQUARE_TO_CIRCLE_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;
const server = await serveRepository(repoRoot, port, { crossOriginIsolated: true });

function visibleObject(frame, label) {
  const visible = frame.objects.filter((object) => object.present && object.bounds !== null);
  assert.equal(visible.length, 1, `${label}: expected exactly one visible geometry object`);
  return visible[0];
}

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgpu"),
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });

  const source = await readFile(
    path.join(repoRoot, "web/python/examples/manim_parity_square_to_circle.py"),
    "utf8",
  );
  const observation = await page.evaluate(async (pythonSource) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new AuthoringExecutionClient(canvas);
    let resolveAttached;
    let rejectAttached;
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });

    try {
      const authored = authoring.run(pythonSource, {}, {
        async onSemanticContinuation(registration) {
          await execution.startSemanticExecution(registration.semanticExecution, {
            authoringClient: authoring,
            loopDurationSeconds: registration.duration,
            transportMode: "transferable",
            pacing: "external_samples",
          });
          resolveAttached();
        },
      });
      authored.then(
        () => rejectAttached(new Error("SquareToCircle did not register a semantic continuation")),
        rejectAttached,
      );

      await attached;
      await execution.sampleToAuthoredTime(0.5);
      const partialCreateFrame = await execution.debugFrame();
      await execution.sampleToAuthoredTime(1.0);
      const fullCreateFrame = await execution.debugFrame();
      await execution.sampleToAuthoredTime(2.0);
      const circleFrame = await execution.debugFrame();
      const [, result] = await Promise.all([
        execution.sampleToAuthoredTime(3.0),
        authored,
      ]);
      const terminalFrame = await execution.debugFrame();
      return {
        duration: result.duration,
        terminalFrame,
        partialCreateFrame,
        fullCreateFrame,
        circleFrame,
      };
    } finally {
      execution.terminate();
      authoring.terminate();
      canvas.remove();
    }
  }, source);

  assert.equal(observation.duration, 3, "SquareToCircle duration drifted");
  for (const [time, frame, label] of [
    [0.5, observation.partialCreateFrame, "partial create phase"],
    [1.0, observation.fullCreateFrame, "full create endpoint"],
    [2.0, observation.circleFrame, "transform endpoint"],
    [3.0, observation.terminalFrame, "source completion"],
  ]) {
    assert.ok(frame, `SquareToCircle ${label} did not publish a debug frame`);
    assert.ok(
      Math.abs(Number(frame.time) - time) < 1e-6,
      `SquareToCircle ${label} debug frame time was ${frame.time}, expected ${time}`,
    );
  }

  const partialCreateObject = visibleObject(
    observation.partialCreateFrame,
    "SquareToCircle partial create phase",
  );
  const createObject = visibleObject(
    observation.fullCreateFrame,
    "SquareToCircle full create endpoint",
  );
  const circleObject = visibleObject(observation.circleFrame, "SquareToCircle transform endpoint");
  const createWidth = Number(createObject.bounds.width);
  const createHeight = Number(createObject.bounds.height);
  const partialWidth = Number(partialCreateObject.bounds.width);
  const partialHeight = Number(partialCreateObject.bounds.height);
  const circleWidth = Number(circleObject.bounds.width);
  const circleHeight = Number(circleObject.bounds.height);

  assert.ok(Number.isFinite(createWidth) && Number.isFinite(circleWidth));
  assert.ok(Number.isFinite(createHeight) && Number.isFinite(circleHeight));
  assert.ok(Number.isFinite(partialWidth) && partialWidth > 0);
  assert.ok(Number.isFinite(partialHeight) && partialHeight > 0);
  assert.ok(
    Math.abs(Number(partialCreateObject.reveal) - 0.5) < 1e-6,
    `SquareToCircle at 0.5s must show the half-revealed Create state; ` +
      `reveal=${partialCreateObject.reveal}`,
  );
  assert.ok(
    Math.abs(Number(partialCreateObject.transform.rotation) - Math.PI / 4) < 1e-6 &&
      Number(partialCreateObject.bounds.height) > circleHeight * 1.3,
    `SquareToCircle at 0.5s must retain the rotated square's diagonal; ` +
      `rotation=${partialCreateObject.transform.rotation}, ` +
      `partial bounds=${partialCreateObject.bounds.width.toFixed(4)}×` +
      `${partialCreateObject.bounds.height.toFixed(4)}, ` +
      `circle bounds=${circleWidth.toFixed(4)}×${circleHeight.toFixed(4)}`,
  );
  assert.ok(Math.abs(Number(createObject.reveal) - 1) < 1e-6);
  const rotatedSquareExtent = 2 * Math.SQRT2;
  assert.ok(
    Math.abs(createWidth - rotatedSquareExtent) < 1e-5 &&
      Math.abs(createHeight - rotatedSquareExtent) < 1e-5,
    `SquareToCircle full create endpoint must retain the 45-degree square bounds ` +
      `${rotatedSquareExtent.toFixed(4)}×${rotatedSquareExtent.toFixed(4)}; ` +
      `actual=${createWidth.toFixed(4)}×${createHeight.toFixed(4)}`,
  );
  assert.ok(
    createWidth > circleWidth * 1.3 && createHeight > circleHeight * 1.3,
    `SquareToCircle full rotated-square bounds must exceed the circle bounds; ` +
      `square bounds=${createWidth.toFixed(4)}×${createHeight.toFixed(4)}, ` +
      `circle bounds=${circleWidth.toFixed(4)}×${circleHeight.toFixed(4)}`,
  );
  assert.ok(Math.abs(Number(circleObject.reveal) - 1) < 1e-6);
  assert.ok(Math.abs(Number(circleObject.transform.rotation)) < 1e-6);
  assert.ok(
    Math.abs(circleWidth - 2) < 1e-6 && Math.abs(circleHeight - 2) < 1e-6,
    `SquareToCircle transform endpoint must have circular bounds; ` +
      `circle bounds=${circleWidth.toFixed(4)}×${circleHeight.toFixed(4)}`,
  );
  assert.deepEqual(errors, [], `SquareToCircle emitted browser errors:\n${errors.join("\n")}`);

  console.log(
    `✓ SquareToCircle starts from rotated square: ` +
      `${createWidth.toFixed(3)}×${createHeight.toFixed(3)} -> ` +
      `${circleWidth.toFixed(3)}×${circleHeight.toFixed(3)}`,
  );
} finally {
  if (browser !== null) await browser.close();
  await server.close();
}

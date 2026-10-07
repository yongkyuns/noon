import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_SQUARE_TO_CIRCLE_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.join(repoRoot, "browser-smoke-artifacts/product-gate/square-to-circle-start");
const sourcePath = "web/python/examples/manim_parity_square_to_circle.py";

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
const retainServerOutput = (chunk) => {
  serverOutput = (serverOutput + chunk).slice(-65_536);
};
server.stdout.on("data", retainServerOutput);
server.stderr.on("data", retainServerOutput);
server.on("error", (error) => retainServerOutput(String(error)));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/semantic-preview-session.js`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`SquareToCircle regression server did not start: ${lastError}\n${serverOutput}`);
}

function visibleObject(frame, label) {
  const visible = frame.objects.filter((object) => object.present && object.bounds !== null);
  assert.equal(visible.length, 1, `${label}: expected exactly one visible geometry object`);
  return visible[0];
}

let browser = null;
const errors = [];
let evidence = {};
try {
  await mkdir(artifactDir, { recursive: true });
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  // Only the empty test shell is intercepted. All production clients, workers,
  // WASM and Python assets are served unchanged from the checked-out package.
  await page.route(`${baseUrl}/square-to-circle-test-shell`, (route) => route.fulfill({
    contentType: "text/html",
    body: '<!doctype html><html><head><link rel="icon" href="data:,"></head><body></body></html>',
  }));
  await page.goto(`${baseUrl}/square-to-circle-test-shell`, { waitUntil: "load" });

  const source = await readFile(path.join(repoRoot, sourcePath), "utf8");
  evidence = { sourcePath, sourceSha256: createHash("sha256").update(source).digest("hex") };
  const result = await page.evaluate(async (pythonSource) => {
    const [{ SemanticPreviewSession }, { PythonAuthoringClient }, { AuthoringExecutionClient }] =
      await Promise.all([
        import("/web/semantic-preview-session.js"),
        import("/web/authoring-client.js"),
        import("/web/authoring-execution-client.js"),
      ]);
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    canvas.style.width = "640px";
    canvas.style.height = "360px";
    document.body.append(canvas);
    let execution;
    const preview = new SemanticPreviewSession({
      createAuthoringClient: () => new PythonAuthoringClient(),
      createExecutionClient: (options) => {
        execution = new AuthoringExecutionClient(canvas, options);
        return execution;
      },
      timeoutMs: 30_000,
      maxSamples: 8,
      maxTimeSeconds: 3,
    });
    const samples = [];
    let result;
    try {
      await preview.open(pythonSource, { loopDurationSeconds: 3 });
      // The shared source is live: do not run construct() to completion and then
      // reconstruct a deleted scene-document representation to seek it.
      for (const time of [0.5, 2.0]) {
        const snapshot = await preview.sample(time);
        samples.push({
          time,
          observation: snapshot.frame,
          frame: await execution.debugFrame(),
          metrics: await execution.metrics(),
        });
      }
      const completed = await preview.sample(3, { stopAtSourceCompletion: true });
      result = { samples, completed };
    } catch (error) {
      result = { samples, error: String(error), snapshot: preview.snapshot };
    } finally {
      const closed = preview.close();
      result = { ...result, cleanupErrors: closed.cleanupErrors };
      canvas.remove();
    }
    return result;
  }, source);
  evidence = { ...evidence, ...result };
  await writeFile(path.join(artifactDir, "samples.json"), JSON.stringify(evidence, null, 2));
  assert.equal(result.error, undefined, `SquareToCircle sampling failed: ${result.error}`);
  assert.deepEqual(result.cleanupErrors, [], "SquareToCircle cleanup failed");
  assert.equal(result.completed.sourceState, "completed", "SquareToCircle source did not finish");
  assert.equal(result.completed.authoredDuration, 3, "SquareToCircle duration drifted");
  assert.equal(result.completed.frame.publishedTime, 3, "SquareToCircle endpoint time drifted");
  assert.equal(result.samples.length, 2);
  for (const sample of result.samples) {
    assert.equal(sample.observation.requestedTime, sample.time);
    assert.equal(sample.observation.publishedTime, sample.time, "sample time drifted");
    assert.equal(sample.frame.time, sample.time, "debug frame is not the sampled state");
    assert.equal(sample.metrics.executionMode, "semantic");
    assert.equal(sample.metrics.metrics.retained, true);
    assert.ok(sample.metrics.metrics.presentedFrames > 0);
  }
  const [createFrame, circleFrame] = result.samples.map((sample) => sample.frame);
  const createObject = visibleObject(createFrame, "SquareToCircle create phase");
  const circleObject = visibleObject(circleFrame, "SquareToCircle transform endpoint");
  const createWidth = Number(createObject.bounds.width);
  const createHeight = Number(createObject.bounds.height);
  const circleWidth = Number(circleObject.bounds.width);
  const circleHeight = Number(circleObject.bounds.height);

  assert.ok(Number.isFinite(createWidth) && Number.isFinite(circleWidth));
  assert.ok(Number.isFinite(createHeight) && Number.isFinite(circleHeight));
  assert.ok(
    createWidth > circleWidth * 1.3 && createHeight > circleHeight * 1.3,
    `SquareToCircle must start from the rotated square before morphing to the circle; ` +
      `create bounds=${createWidth.toFixed(4)}×${createHeight.toFixed(4)}, ` +
      `circle bounds=${circleWidth.toFixed(4)}×${circleHeight.toFixed(4)}`,
  );
  assert.deepEqual(errors, [], `SquareToCircle emitted browser errors:\n${errors.join("\n")}`);

  console.log(
    `✓ SquareToCircle starts from rotated square: ` +
      `${createWidth.toFixed(3)}×${createHeight.toFixed(3)} -> ` +
      `${circleWidth.toFixed(3)}×${circleHeight.toFixed(3)}`,
  );
} catch (error) {
  await writeFile(path.join(artifactDir, "failure.json"), JSON.stringify({
    ...evidence, error: String(error), stack: error.stack, errors, serverOutput,
  }, null, 2));
  throw error;
} finally {
  try {
    if (browser !== null) await browser.close();
  } finally {
    server.kill("SIGTERM");
  }
}

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

import { compareForegroundCoverage } from "./browser-visual-parity-lib.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_MANIM_HOST_DIAGNOSTICS_PORT ?? "4194");
const baseUrl = `http://127.0.0.1:${port}`;
const targetObject = 2;
const targetFrame = 90;
const targetTime = 3.0;
const frameTimes = Array.from({ length: targetFrame + 1 }, (_, index) => index / 30);
const rasterTolerances = {
  backgroundDistance: 32,
  neighborRadius: 1,
  maxMismatchFraction: 0.02,
  maxBoundsDelta: 2,
};
const source = await readFile(
  path.join(repoRoot, "web/python/examples/manim_host_updater_mixed_primitive.py"),
  "utf8",
);
const authoredSource = source;

function browserArgs(backend) {
  if (backend === "webgpu") {
    return [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  return [
    "--disable-features=WebGPU",
    "--enable-unsafe-swiftshader",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/manim-raster-host.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`host-updater diagnostic server did not start: ${lastError}\n${serverOutput}`);
}

function assertNear(actual, expected, label) {
  assert.ok(
    Math.abs(Number(actual) - Number(expected)) <= 1e-6,
    `${label}: expected ${expected}, got ${actual}`,
  );
}

function assertSameObjectState(left, right, label) {
  assert.equal(right.object, left.object, `${label}: object identity diverged`);
  assert.equal(right.frame_index, left.frame_index, `${label}: frame index diverged`);
  assert.deepEqual(right.slot, left.slot, `${label}: execution slot diverged`);
  assert.deepEqual(right.transform, left.transform, `${label}: transform diverged`);
  assert.deepEqual(right.world_endpoints, left.world_endpoints, `${label}: endpoints diverged`);
}

function assertDiagnostic(diagnostic, backend) {
  assert.ok(diagnostic, `${backend}: missing host-updater diagnostic`);
  assert.equal(diagnostic.schema_version, 1);
  assert.equal(diagnostic.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
  assert.equal(diagnostic.execution.session, 1);
  // Host patch sequences are zero-based. The frame-90 diagnostic is produced by
  // the 90th host phase, whose submitted patch sequence is therefore 89.
  assert.equal(diagnostic.execution.sequence, targetFrame - 1);
  assert.equal(diagnostic.execution.layout_generation, 0);
  assertNear(diagnostic.execution.time, targetTime, `${backend}: diagnostic time`);

  const committed = diagnostic.committed;
  assert.equal(committed.object, targetObject);
  assert.ok(Number.isSafeInteger(committed.frame_index));
  assert.deepEqual(committed.slot, { slot: targetObject, generation: 0 });
  assert.equal(committed.dirty_classification, "updated");
  // The host callback phase resets dt when the updater switches direction at
  // t=2.0, so the frame at t=3.0 contains 29 backward steps after the boundary.
  assertNear(committed.transform.rotation, 29 / 30, `${backend}: committed rotation`);

  assertSameObjectState(committed, diagnostic.prepared.state, `${backend}: prepared`);

  assert.equal(diagnostic.prepared.instance_kind, "line");
  const lineInstanceIndex = diagnostic.prepared.instance_index;
  assert.ok(Number.isSafeInteger(lineInstanceIndex));
  assert.ok(diagnostic.prepared.instance_range);
  assert.ok(diagnostic.prepared.instance_range.start <= lineInstanceIndex);
  assert.ok(diagnostic.prepared.instance_range.end > lineInstanceIndex);
  assert.equal(diagnostic.prepared.full_rebuilds, 0);
  assert.equal(diagnostic.prepared.instances_repacked, 1);

  assert.ok(diagnostic.upload.instance_generation > 1);
  assert.equal(diagnostic.upload.buffer_reallocations, 0);
  assert.ok(diagnostic.upload.target_write, `${backend}: target upload missing`);
  assert.equal(diagnostic.upload.target_write.buffer, "line");
  assert.ok(diagnostic.upload.target_write.instance_range.start <= lineInstanceIndex);
  assert.ok(diagnostic.upload.target_write.instance_range.end > lineInstanceIndex);
  assert.ok(diagnostic.upload.target_write.byte_length > 0);
  assert.ok(diagnostic.upload.target_write.payload_hash > 0);
  assert.ok(diagnostic.upload.writes.length > 0);
  assert.equal(
    diagnostic.upload.bytes_uploaded,
    diagnostic.upload.writes.reduce((total, write) => total + write.byte_length, 0),
  );
  assert.equal(diagnostic.upload.total_bytes_uploaded, diagnostic.upload.bytes_uploaded);

  assert.equal(diagnostic.draw_plan.submission_membership, true);
  assert.ok(
    diagnostic.draw_plan.batches.some(
      (batch) =>
        batch.primitive === "line" &&
        batch.instance_range.start <= lineInstanceIndex &&
        batch.instance_range.end > lineInstanceIndex,
    ),
    `${backend}: target is absent from draw plan`,
  );
  assert.ok(diagnostic.draw_plan.draw_calls > 0);
  assert.ok(diagnostic.draw_plan.instances_drawn > 0);

  assert.equal(diagnostic.present_call.submit_called, true);
  assert.equal(diagnostic.present_call.present_called, true);
  assert.ok(["success", "suboptimal"].includes(diagnostic.present_call.surface_status));
}

async function runBackend(browser, backend) {
  const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  try {
    await page.goto(`${baseUrl}/web/manim-raster-host.html`, { waitUntil: "load" });
    await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30_000 });
    await page.evaluate(() => window.noonHostRaster.ready());
    const loaded = await page.evaluate(
      ({ pythonSource, duration }) => window.noonHostRaster.load(pythonSource, duration),
      { pythonSource: authoredSource, duration: 4.5 },
    );
    assert.equal(loaded.rendererBackend, backend === "webgl" ? "WebGL2" : "WebGPU");
    assert.equal(loaded.objectCount, 3);
    await page.evaluate((objectId) => {
      window.noonHostRaster.setHostUpdaterDiagnosticObject(objectId);
    }, targetObject);
    const result = await page.evaluate(
      ({ frame, times }) => window.noonHostRaster.renderThrough(frame, times),
      { frame: targetFrame, times: frameTimes },
    );
    assert.equal(result.frameIndex, targetFrame);
    assertNear(result.time, targetTime, `${backend}: logical frame time`);
    assertDiagnostic(result.diagnostic, backend);
    assert.deepEqual(errors, [], `${backend}: browser errors`);
    const screenshot = await page.locator("#scene").screenshot();
    return { diagnostic: result.diagnostic, screenshot };
  } finally {
    await page.close();
  }
}

await waitForServer();
let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgl"),
  });
  const webgl = await runBackend(browser, "webgl");
  await browser.close();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgpu"),
  });
  const webgpu = await runBackend(browser, "webgpu");

  for (const field of ["committed", "prepared", "upload", "draw_plan"]) {
    assert.deepEqual(
      webgl.diagnostic[field],
      webgpu.diagnostic[field],
      `WebGL2/WebGPU ${field} diagnostic state diverged`,
    );
  }
  assert.deepEqual(
    webgl.diagnostic.present_call.submit_called,
    webgpu.diagnostic.present_call.submit_called,
  );
  assert.deepEqual(
    webgl.diagnostic.present_call.present_called,
    webgpu.diagnostic.present_call.present_called,
  );

  const rasterComparison = compareForegroundCoverage(
    PNG.sync.read(webgpu.screenshot),
    PNG.sync.read(webgl.screenshot),
    rasterTolerances,
  );
  assert.equal(
    rasterComparison.pass,
    true,
    `WebGL2/WebGPU frame-90 foreground coverage diverged: ` +
      `${(rasterComparison.mismatchFraction * 100).toFixed(3)}% unmatched foreground, ` +
      `${rasterComparison.boundsDelta}px bounds delta`,
  );
  console.log(
    `✓ RotationUpdater frame 90 raster parity: ` +
      `${(rasterComparison.mismatchFraction * 100).toFixed(3)}% unmatched foreground, ` +
      `${rasterComparison.boundsDelta}px bounds delta`,
  );
  console.log(
    "✓ RotationUpdater frame 90 / t=3.0s: committed, prepared, uploaded, drawn, presented, and raster-visible state agree on WebGL2 and WebGPU",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_MANIM_HOST_DIAGNOSTICS_PORT ?? "4194");
const baseUrl = `http://127.0.0.1:${port}`;
const targetObject = 1;
const targetFrame = 90;
const targetTime = 3.0;
const frameTimes = Array.from({ length: targetFrame + 1 }, (_, index) => index / 30);
const source = await readFile(
  path.join(repoRoot, "web/python/examples/manim_gallery_rotation_updater.py"),
  "utf8",
);
const authoredSource = `${source}

result = RotationUpdater()
result.setup()
try:
    result.construct()
finally:
    result.tear_down()
`;

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
  // The canonical RotationUpdater has no timeline-authored delta. The initial
  // snapshot is sequence 0, then each non-empty host patch advances one sequence;
  // frame zero's dt=0 patch is empty.
  assert.equal(diagnostic.execution.sequence, targetFrame);
  assert.equal(diagnostic.execution.layout_generation, 0);
  assertNear(diagnostic.execution.time, targetTime, `${backend}: diagnostic time`);

  const committed = diagnostic.committed;
  assert.equal(committed.object, targetObject);
  assert.equal(committed.frame_index, targetObject);
  assert.deepEqual(committed.slot, { slot: targetObject, generation: 0 });
  assert.equal(committed.dirty_classification, "updated");
  assertNear(committed.transform.rotation, 1.0, `${backend}: committed rotation`);

  assertSameObjectState(committed, diagnostic.prepared.state, `${backend}: prepared`);
  assertSameObjectState(committed, diagnostic.gpu.state, `${backend}: gpu`);
  assertSameObjectState(committed, diagnostic.draw.state, `${backend}: draw`);

  assert.equal(diagnostic.prepared.instance_kind, "line");
  assert.deepEqual(diagnostic.prepared.instance_range, { start: 1, end: 2 });
  assert.equal(diagnostic.prepared.full_rebuilds, 0);
  assert.equal(diagnostic.prepared.instances_repacked, 1);

  assert.equal(diagnostic.gpu.instance_kind, "line");
  assert.deepEqual(diagnostic.gpu.instance_range, { start: 1, end: 2 });
  assert.deepEqual(diagnostic.gpu.dirty_ranges, [{ start: 1, end: 2 }]);
  assert.equal(diagnostic.gpu.bytes_uploaded, 88);
  assert.equal(diagnostic.gpu.total_bytes_uploaded, 88);
  assert.equal(diagnostic.gpu.buffer_reallocations, 0);
  assert.ok(diagnostic.gpu.instance_generation > 1);

  assert.equal(diagnostic.draw.submission_membership, true);
  assert.deepEqual(diagnostic.draw.batches, [
    { primitive: "line", instance_range: { start: 0, end: 2 } },
  ]);
  assert.equal(diagnostic.draw.draw_calls, 1);
  assert.equal(diagnostic.draw.instances_drawn, 2);

  assert.equal(diagnostic.presentation.submitted, true);
  assert.equal(diagnostic.presentation.presented, true);
  assert.ok(["success", "suboptimal"].includes(diagnostic.presentation.surface_status));
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
    assert.equal(loaded.objectCount, 2);
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
    return result.diagnostic;
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
  const webglDiagnostic = await runBackend(browser, "webgl");
  await browser.close();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgpu"),
  });
  const webgpuDiagnostic = await runBackend(browser, "webgpu");

  for (const field of ["committed", "prepared", "gpu", "draw", "presentation"]) {
    assert.deepEqual(
      webglDiagnostic[field],
      webgpuDiagnostic[field],
      `WebGL2/WebGPU ${field} diagnostic state diverged`,
    );
  }
  console.log(
    "✓ RotationUpdater frame 90 / t=3.0s: committed, prepared, uploaded, drawn, and presented state agree on WebGL2 and WebGPU",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

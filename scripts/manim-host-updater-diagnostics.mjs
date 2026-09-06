import assert from "node:assert/strict";
import { createReadStream } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
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
const targetTime = 1.0;
const rasterTolerances = {
  backgroundDistance: 32,
  neighborRadius: 1,
  maxMismatchFraction: 0.02,
  maxBoundsDelta: 2,
};
const source = await readFile(
  path.join(repoRoot, "web/python/examples/renderer_observation_callbacks.py"),
  "utf8",
);
const lineSource = await readFile(
  path.join(repoRoot, "web/python/examples/renderer_observation_line_callbacks.py"),
  "utf8",
);
const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
  [".py", "text/x-python; charset=utf-8"],
]);

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

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, baseUrl);
    const relative = decodeURIComponent(url.pathname).replace(/^\/+/, "");
    const resolved = path.resolve(repoRoot, relative || "web/execution-worker-smoke.html");
    if (resolved !== repoRoot && !resolved.startsWith(`${repoRoot}${path.sep}`)) {
      response.writeHead(403).end("forbidden");
      return;
    }
    const info = await stat(resolved);
    if (!info.isFile()) {
      response.writeHead(404).end("not found");
      return;
    }
    response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader(
      "Content-Type",
      contentTypes.get(path.extname(resolved)) ?? "application/octet-stream",
    );
    response.writeHead(200);
    createReadStream(resolved).pipe(response);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});

function assertTargetWrite(write, buffer, label) {
  assert.ok(write, `${label}: target upload missing`);
  assert.equal(write.buffer, buffer);
  assert.ok(write.instance_end > write.instance_start);
  assert.ok(write.byte_length > 0);
  assert.ok(write.payload_hash > 0);
}

function assertCommonObservation(observation, backend, label, expectedTime = targetTime) {
  assert.equal(observation.schema_version, 1);
  assert.equal(observation.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
  assert.ok(Number.isSafeInteger(observation.publication.session));
  assert.ok(Number.isSafeInteger(observation.publication.sequence));
  assert.equal(observation.committed.time, expectedTime);
  assert.equal(observation.committed.dirty, "updated");
  assert.equal(observation.committed.presence, true);
  assert.equal(observation.mirrored.object, observation.committed.object);
  assert.equal(observation.mirrored.frame_index, observation.committed.frame_index);
  assert.equal(observation.mirrored.time, observation.committed.time);
  assert.deepEqual(observation.mirrored.transform, observation.committed.transform);
  assert.deepEqual(observation.mirrored.style, observation.committed.style);
  assert.equal(observation.mirrored.presence, observation.committed.presence);
  assert.equal(observation.prepared.full_rebuilds, 0, `${label}: local prepare rebuilt frame`);
  assert.ok(observation.prepared.instances_repacked <= 1);
  assert.equal(observation.upload.buffer_reallocations, 0);
  assert.equal(observation.draw.submission_membership, true);
  assert.ok(observation.draw.geometry_draw_calls > 0);
  assert.ok(observation.draw.geometry_instances_drawn > 0);
  assert.ok(observation.draw.text_draw_calls > 0);
  assert.ok(observation.draw.text_instances_drawn > 0);
  assert.ok(["success", "suboptimal"].includes(observation.presentation.surface_status));
  assert.ok(observation.presentation.presentation_sequence > 0);
  assert.equal(observation.presentation.submit_called, true);
  assert.equal(observation.presentation.present_called, true);
}

function assertLineObservation(observation, backend, expectedTime, expectedRotation) {
  assertCommonObservation(observation, backend, `${backend} Line at ${expectedTime}`, expectedTime);
  assert.deepEqual(observation.committed.transform.translation, { x: 0, y: 0 });
  assert.ok(Math.abs(observation.committed.transform.rotation - expectedRotation) < 1e-6);
  assert.equal(observation.committed.frame_index, 2);
  assert.equal(observation.prepared.kind, "geometry");
  assert.equal(observation.prepared.primitive, "line");
  assert.deepEqual(
    [observation.prepared.instance_start, observation.prepared.instance_end],
    [1, 2],
  );
  assert.equal(observation.prepared.glyph_item_count, 0);
  assert.deepEqual(observation.prepared.glyph_ranges, []);
  assertTargetWrite(observation.upload.target_write, "line", `${backend} Line`);
  assert.deepEqual(
    [observation.upload.target_write.instance_start, observation.upload.target_write.instance_end],
    [1, 2],
  );
  assert.deepEqual(observation.upload.target_text_writes, []);
  assert.equal(observation.upload.text_bytes_uploaded, 0);
  assert.ok(observation.upload.geometry_bytes_uploaded >= observation.upload.target_write.byte_length);
}

function assertGeometryObservation(observation, backend) {
  assertCommonObservation(observation, backend, `${backend} geometry`);
  assert.deepEqual(observation.committed.transform.translation, { x: 1, y: 1 });
  assert.equal(observation.prepared.kind, "geometry");
  assert.equal(observation.prepared.primitive, "circle");
  assert.ok(observation.prepared.instance_end > observation.prepared.instance_start);
  assert.equal(observation.prepared.glyph_item_count, 0);
  assert.deepEqual(observation.prepared.glyph_ranges, []);
  assertTargetWrite(observation.upload.target_write, "circle", `${backend} geometry`);
  assert.ok(observation.upload.target_write.instance_start < observation.prepared.instance_end);
  assert.ok(observation.upload.target_write.instance_end > observation.prepared.instance_start);
  const { transform } = observation.committed;
  assert.deepEqual(observation.prepared.transform, {
    translation: [transform.translation.x, transform.translation.y],
    scale: [transform.scale.x, transform.scale.y],
    rotation: transform.rotation,
  });
  assert.deepEqual(observation.upload.target_text_writes, []);
  assert.ok(observation.upload.geometry_bytes_uploaded >= observation.upload.target_write.byte_length);
}

function assertTextObservation(observation, backend) {
  assertCommonObservation(observation, backend, `${backend} text`);
  assert.deepEqual(observation.committed.transform.translation, { x: 1, y: -2 });
  assert.equal(observation.prepared.kind, "text");
  assert.equal(observation.prepared.primitive, null);
  assert.equal(observation.prepared.instance_start, null);
  assert.equal(observation.prepared.instance_end, null);
  assert.equal(observation.prepared.render_item_count, observation.prepared.glyph_item_count);
  assert.ok(observation.prepared.glyph_item_count > 0);
  assert.equal(
    observation.prepared.glyph_ranges.length,
    observation.prepared.glyph_item_count,
  );
  assert.ok(observation.prepared.glyph_ranges.every((range) =>
    ["mask", "color"].includes(range.plane) &&
    range.instance_end > range.instance_start &&
    range.instance_dirty === true));
  assert.equal(observation.upload.target_write, null);
  assert.ok(observation.upload.target_text_writes.length > 0);
  for (const write of observation.upload.target_text_writes) {
    assert.ok(["text_mask", "text_color"].includes(write.buffer));
    assert.ok(write.instance_end > write.instance_start);
    assert.ok(write.byte_length > 0);
    assert.ok(write.payload_hash > 0);
    assert.ok(observation.prepared.glyph_ranges.some((range) =>
      write.buffer === `text_${range.plane}` &&
      write.instance_start < range.instance_end && write.instance_end > range.instance_start),
    "text upload does not overlap the observed target");
  }
  assert.ok(observation.upload.text_bytes_uploaded >= observation.upload.target_text_writes
    .reduce((total, write) => total + write.byte_length, 0));
}

function assertVisibleScene(screenshot) {
  const png = PNG.sync.read(screenshot);
  const counts = { animated: 0, anchor: 0, label: 0 };
  for (let y = 0; y < png.height; y += 1) {
    for (let x = 0; x < png.width; x += 1) {
      const offset = (y * png.width + x) * 4;
      if (Math.min(...png.data.subarray(offset, offset + 3)) <= 35) continue;
      if (x > 320 && x < 410 && y > 90 && y < 180) counts.animated += 1;
      if (x > 160 && x < 210 && y > 155 && y < 205) counts.anchor += 1;
      if (x > 300 && x < 450 && y > 230 && y < 310) counts.label += 1;
    }
  }
  assert.ok(counts.animated > 500, "animated callback geometry was blank");
  assert.ok(counts.anchor > 100, "unchanged resident geometry was blank");
  assert.ok(counts.label > 20, "observed callback Text was blank");
}

function assertVisibleLineScene(screenshot, direction) {
  const png = PNG.sync.read(screenshot);
  const counts = { moving: 0, wrongDirection: 0, marker: 0, label: 0 };
  for (let y = 0; y < png.height; y += 1) {
    for (let x = 0; x < png.width; x += 1) {
      const offset = (y * png.width + x) * 4;
      const [red, green, blue] = png.data.subarray(offset, offset + 3);
      const yellow = red > 120 && green > 90 && blue < 100;
      const expectedSide = direction === "forward" ? y > 182 : y < 178;
      if (yellow && x > 288 && x < 322 && expectedSide && Math.abs(y - 180) < 45) {
        counts.moving += 1;
      }
      if (yellow && x > 288 && x < 322 && !expectedSide && Math.abs(y - 180) < 45) {
        counts.wrongDirection += 1;
      }
      if (Math.max(red, green, blue) > 35 && x > 160 && x < 210 && y > 155 && y < 205) {
        counts.marker += 1;
      }
      if (Math.max(red, green, blue) > 35 && x > 285 && x < 405 && y > 235 && y < 305) {
        counts.label += 1;
      }
    }
  }
  assert.ok(counts.moving > 20, `${direction} callback Line pixels were blank`);
  assert.ok(counts.moving > counts.wrongDirection * 2, `${direction} callback Line pointed the wrong way`);
  assert.ok(counts.marker > 50, "resident Circle sibling was blank");
  assert.ok(counts.label > 20, "resident Text sibling was blank");
}

async function installHarness(page) {
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
  await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    window.rendererObservationHarness = { PythonAuthoringClient, AuthoringExecutionClient, authoring };
  });
}

async function observeTarget(page, target) {
  const result = await page.evaluate(async ({ source, target, targetTime }) => {
    const harness = window.rendererObservationHarness;
    const authored = await harness.authoring.run(source, { observation_target: target });
    const canvas = document.createElement("canvas");
    canvas.id = `renderer-observation-${target}`;
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.activeExecution = execution;
    try {
      const ready = await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: harness.authoring,
        loopDurationSeconds: 4,
        transportMode: "transferable",
      });
      const paused = await execution.pause();
      if (paused.time > targetTime) {
        throw new Error(`renderer observation advanced past ${targetTime} before pause`);
      }
      const advanced = await execution.advanceToWithRendererObservation(targetTime);
      if (advanced.time !== targetTime || advanced.playing !== false) {
        throw new Error(`renderer observation did not remain paused at ${targetTime}`);
      }
      if (advanced.rendererObservation?.outcome !== "presented") {
        throw new Error(`renderer observation was not presented: ${JSON.stringify(advanced)}`);
      }
      return {
        canvasId: canvas.id,
        rendererBackend: ready.render.backend,
        observation: advanced.rendererObservation,
      };
    } catch (error) {
      execution.terminate();
      harness.activeExecution = null;
      throw error;
    }
  }, { source, target, targetTime });
  try {
    const screenshot = await page.locator(`#${result.canvasId}`).screenshot();
    return { ...result, screenshot };
  } finally {
    await page.evaluate(() => {
      window.rendererObservationHarness.activeExecution.terminate();
      window.rendererObservationHarness.activeExecution = null;
    });
  }
}

async function observeLineSequence(page) {
  const result = await page.evaluate(async ({ source, firstSampleTime }) => {
    const harness = window.rendererObservationHarness;
    const authored = await harness.authoring.run(source);
    const canvas = document.createElement("canvas");
    canvas.id = "renderer-observation-line";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    const execution = new harness.AuthoringExecutionClient(canvas);
    harness.activeExecution = execution;
    try {
      const ready = await execution.startSemanticExecution(authored.semanticExecution, {
        authoringClient: harness.authoring,
        loopDurationSeconds: 4.5,
        transportMode: "transferable",
      });
      const paused = await execution.pause();
      if (paused.time > firstSampleTime) {
        throw new Error(`Line observation advanced past ${firstSampleTime} before pause`);
      }
      return { canvasId: canvas.id, rendererBackend: ready.render.backend };
    } catch (error) {
      execution.terminate();
      harness.activeExecution = null;
      throw error;
    }
  }, { source: lineSource, firstSampleTime: 1.0 });
  try {
    const observations = [];
    for (const time of [1.0, 3.0]) {
      const observation = await page.evaluate(async (time) => {
        const execution = window.rendererObservationHarness.activeExecution;
        const advanced = await execution.advanceToWithRendererObservation(time);
        if (advanced.time !== time || advanced.playing !== false) {
          throw new Error(`Line observation did not remain paused at ${time}`);
        }
        if (advanced.rendererObservation?.outcome !== "presented") {
          throw new Error(`Line observation was not presented: ${JSON.stringify(advanced)}`);
        }
        return advanced.rendererObservation;
      }, time);
      observations.push({
        observation,
        screenshot: await page.locator(`#${result.canvasId}`).screenshot(),
      });
    }
    return { ...result, observations };
  } finally {
    await page.evaluate(() => {
      window.rendererObservationHarness.activeExecution.terminate();
      window.rendererObservationHarness.activeExecution = null;
    });
  }
}

async function runBackend(backend) {
  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend),
  });
  const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  try {
    await installHarness(page);
    const geometry = await observeTarget(page, "geometry");
    const text = await observeTarget(page, "text");
    const line = await observeLineSequence(page);
    const expectedBackend = backend === "webgl" ? "WebGL2" : "WebGPU";
    assert.equal(geometry.rendererBackend, expectedBackend);
    assert.equal(text.rendererBackend, expectedBackend);
    assert.equal(line.rendererBackend, expectedBackend);
    assertGeometryObservation(geometry.observation, backend);
    assertTextObservation(text.observation, backend);
    assertVisibleScene(geometry.screenshot);
    assertVisibleScene(text.screenshot);
    assertLineObservation(line.observations[0].observation, backend, 1.0, 1.0);
    assertLineObservation(line.observations[1].observation, backend, 3.0, -1.0);
    assertVisibleLineScene(line.observations[0].screenshot, "forward");
    assertVisibleLineScene(line.observations[1].screenshot, "backward");
    assert.deepEqual(errors, [], `${backend}: browser errors`);
    return { geometry, text, line };
  } finally {
    await page.evaluate(() => window.rendererObservationHarness?.authoring.terminate());
    await browser.close();
  }
}

await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

try {
  const webgl = await runBackend("webgl");
  const webgpu = await runBackend("webgpu");
  for (const target of ["geometry", "text"]) {
    for (const field of ["object", "semantic_slot", "semantic_generation", "time", "transform", "style", "presence"]) {
      assert.deepEqual(webgpu[target].observation.committed[field],
        webgl[target].observation.committed[field], `${target}: backend ${field} diverged`);
    }
    const rasterComparison = compareForegroundCoverage(
      PNG.sync.read(webgpu[target].screenshot),
      PNG.sync.read(webgl[target].screenshot),
      rasterTolerances,
    );
    assert.equal(
      rasterComparison.pass,
      true,
      `${target} callback raster diverged: ` +
        `${(rasterComparison.mismatchFraction * 100).toFixed(3)}% unmatched foreground, ` +
        `${rasterComparison.boundsDelta}px bounds delta`,
    );
  }
  for (let sample = 0; sample < 2; sample += 1) {
    const webglLine = webgl.line.observations[sample];
    const webgpuLine = webgpu.line.observations[sample];
    for (const field of ["object", "semantic_slot", "semantic_generation", "time", "transform", "style", "presence"]) {
      assert.deepEqual(webgpuLine.observation.committed[field],
        webglLine.observation.committed[field], `Line sample ${sample}: backend ${field} diverged`);
    }
    const rasterComparison = compareForegroundCoverage(
      PNG.sync.read(webgpuLine.screenshot),
      PNG.sync.read(webglLine.screenshot),
      rasterTolerances,
    );
    assert.equal(rasterComparison.pass, true,
      `Line callback sample ${sample} raster diverged: ` +
      `${(rasterComparison.mismatchFraction * 100).toFixed(3)}% unmatched foreground, ` +
      `${rasterComparison.boundsDelta}px bounds delta`);
  }
  console.log(
    "✓ Canonical callback Circle, Line, and Text targets were locally prepared, uploaded, drawn, presented, and raster-visible on WebGL2 and WebGPU",
  );
} finally {
  server.close();
}

import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { summarizeSamples } from "../web/frame-metrics.js";
import { serveRepository } from "./browser-test-server.mjs";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cases = parseCases(process.env.NOON_HOST_CALLBACK_CASES ?? "600:1,600:16,600:64");
const frames = positiveInteger(process.env.NOON_HOST_CALLBACK_FRAMES ?? "300", "frames");
const warmup = positiveInteger(process.env.NOON_HOST_CALLBACK_WARMUP ?? "30", "warmup");
const port = positiveInteger(process.env.NOON_HOST_CALLBACK_PORT ?? "4184", "port");
const artifactPath = path.resolve(repoRoot, process.env.NOON_HOST_CALLBACK_ARTIFACT ?? "perf-artifacts/host-callback-perf.json");
const runtimeIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(repoRoot), runtimeIdentity,
  "callback profile requires a clean source snapshot and matching runtime package");
assert.match(runtimeIdentity.sourceRevision ?? "", /^[0-9a-f]{40}$/, "callback profile requires a source revision");
const server = await serveRepository(repoRoot, port);
const { baseUrl } = server;

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });
  const results = [];
  for (const testCase of cases) {
    const page = await browser.newPage();
    const query = new URLSearchParams({
      objects: String(testCase.objects),
      active: String(testCase.active),
      frames: String(frames),
      warmup: String(warmup),
    });
    process.stdout.write(`Host callback ${testCase.active}/${testCase.objects}… `);
    await page.goto(`${baseUrl}/web/host-callback-perf.html?${query}`, { waitUntil: "load" });
    await page.waitForFunction(
      () => window.__NOON_HOST_CALLBACK_PERF__ || document.querySelector("#status")?.dataset.state === "error",
      null,
      { timeout: 600_000 },
    );
    const state = await page.locator("#status").getAttribute("data-state");
    if (state === "error") throw new Error(await page.locator("#status").textContent());
    const report = await page.evaluate(() => window.__NOON_HOST_CALLBACK_PERF__);
    assert.equal(report.schemaVersion, 4);
    assert.equal(report.workload.objects, testCase.objects);
    assert.equal(report.workload.active, testCase.active);
    assert.equal(report.host.rendererBackend, "WebGL2");
    assert.equal(report.host.locality.lastPublication.objectCount, testCase.objects);
    assert.equal(report.host.finalState.playing, false);
    const { samples, wallMs, clock } = report.host.stages;
    assert.equal(clock, "semantic-endpoint-wall-durations");
    assert.equal(samples.length, frames, "retain every measured callback sample");
    const fields = ["roundTripMs", "rustDriveMs", "callbackPhaseMs", "callbackRunMs",
      "callbackCommitMs", "callbackCompleteMs", "presentationWaitMs", "deltaDrainMs",
      "deltaMetadataMs", "deltaSendMs", "segmentHandoffMs", "authoringBoundaryWaitMs", "endpointMs"];
    assert.deepEqual(Object.keys(wallMs).sort(), [...fields].sort());
    for (const [index, sample] of samples.entries()) {
      assert.deepEqual(Object.keys(sample).sort(), ["time", ...fields].sort());
      assert.ok(Math.abs(sample.time - (report.host.authoredSamples.measuredFirst +
        index * report.host.authoredSamples.stepSeconds)) <= 1e-9, "exact ordered authored samples");
      assert.ok(fields.every(key => Number.isFinite(sample[key]) && sample[key] >= 0));
      // Allow reduced browser clock precision. Parent durations contain their
      // children; never add callback sub-stages to callbackPhaseMs again.
      assert.ok(sample.callbackRunMs + sample.callbackCommitMs + sample.callbackCompleteMs <=
        sample.callbackPhaseMs + 1, "callback sub-stages must fit their barrier");
      assert.ok(sample.callbackPhaseMs + sample.rustDriveMs + sample.presentationWaitMs +
        sample.deltaDrainMs + sample.deltaMetadataMs + sample.deltaSendMs <= sample.endpointMs + 1,
      "endpoint stages must fit their request");
      assert.ok(sample.endpointMs <= sample.roundTripMs + 1, "endpoint must fit client round trip");
    }
    for (const key of fields) {
      assert.deepEqual(wallMs[key], summarizeSamples(samples.map(sample => sample[key])),
        `${key}: summary must include every raw sample`);
    }
    assert.deepEqual(report.host.advanceRoundTripMs, wallMs.roundTripMs);
    results.push(report);
    console.log(
      `host ${fmt(report.host.advanceRoundTripMs?.p95)} ms, ` +
        `callback ${fmt(wallMs.callbackRunMs.p95)} ms, ack ${fmt(wallMs.presentationWaitMs.p95)} ms, ` +
        `host upload ${fmt(report.host.locality.lastPublication.bytesUploaded)} B`,
    );
    await page.close();
  }
  assert.deepEqual(await createRuntimeBuildIdentity(repoRoot), runtimeIdentity,
    "callback profile source/package changed during measurement");
  const artifact = {
    schemaVersion: 4,
    benchmark: "Noon canonical Python callback active-set matrix",
    generatedAt: new Date().toISOString(),
    commit: runtimeIdentity.sourceRevision,
    runtimeIdentity,
    browserVersion: browser.version(),
    host: { platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0]?.model ?? null },
    configuration: { cases, frames, warmup, collectTimings: true, rendererBackend: "WebGL2 (SwiftShader)" },
    results,
  };
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(`Wrote ${path.relative(repoRoot, artifactPath)}`);
} finally {
  await browser?.close();
  await server.close();
}

function parseCases(value) {
  return String(value).split(",").map((entry) => {
    const [objectsText, activeText] = entry.trim().split(":");
    const objects = positiveInteger(objectsText, "objects");
    const active = positiveInteger(activeText, "active");
    assert.ok(active <= objects, "active must not exceed objects");
    return { objects, active };
  });
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function fmt(value) {
  return Number.isFinite(value) ? Number(value).toFixed(3) : "—";
}

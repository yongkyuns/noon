import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

import {
  STRESS_DURATION_SECONDS,
  STRESS_FINAL_VISIBLE_OBJECT_COUNT,
  STRESS_OBJECT_COUNT,
  STRESS_PHASES,
  STRESS_REACTIVATION_TIMES,
  STRESS_SAMPLE_HZ,
  STRESS_SOURCE_SHA256,
  STRESS_TRACK_COUNT,
  assertRetainedMorphReactivation,
  assertFirstMorphActivationLatency,
  assertSteadyStressTelemetry,
} from "./retained-dynamic-stress-perf-lib.mjs";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = "web/python/examples/manim_parity_stress_grid.py";
const source = await readFile(path.join(repoRoot, sourcePath), "utf8");
const sourceSha256 = createHash("sha256").update(source).digest("hex");
assert.equal(
  sourceSha256,
  STRESS_SOURCE_SHA256,
  "Dynamic Load Stress fixture changed; review the performance workload before updating its hash",
);
const backend = process.env.NOON_RETAINED_STRESS_BACKEND ?? "webgpu";
assert.ok(backend === "webgpu" || backend === "webgl", `unsupported backend ${backend}`);
const port = positiveInteger(process.env.NOON_RETAINED_STRESS_PORT ?? "4192", "port");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactPath = path.resolve(
  repoRoot,
  process.env.NOON_RETAINED_STRESS_ARTIFACT ??
    `perf-artifacts/retained-dynamic-stress-${backend}.json`,
);
const workerLoops = positiveInteger(
  process.env.NOON_RETAINED_STRESS_WORKER_LOOPS ?? "2",
  "worker loop count",
);
const screenshotPath = artifactPath.replace(/\.json$/u, ".png");
const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });

const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
  [".py", "text/plain; charset=utf-8"],
]);
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
    if (backend === "webgl" && relative === "web/authoring-render-worker.js") {
      const workerSource = await readFile(resolved, "utf8");
      response.writeHead(200);
      response.end(`
Object.defineProperty(Object.getPrototypeOf(navigator), "gpu", {
  value: undefined,
  configurable: true,
});
${workerSource}
`);
      return;
    }
    response.writeHead(200);
    createReadStream(resolved).pipe(response);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end(String(error));
  }
});
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs(backend),
  });
  const context = await browser.newContext({ viewport: { width: 1100, height: 720 } });
  if (backend === "webgl") {
    await context.addInitScript(() => {
      Object.defineProperty(Object.getPrototypeOf(navigator), "gpu", {
        value: undefined,
        configurable: true,
      });
    });
  }
  const page = await context.newPage();
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const result = await page.evaluate(
    async ({
      expectedSourceSha256,
      sourcePath,
      phases,
      duration,
      objectCount,
      trackCount,
      sampleHz,
      workerLoops,
      backendRequested,
      reactivationTimes,
    }) => {
      const wasm = await import("./pkg/noon_web.js");
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
      const { summarizeSamples } = await import("./frame-metrics.js");
      const {
        EXECUTION_TRANSPORT_SHARED,
        EXECUTION_TRANSPORT_TRANSFERABLE,
      } = await import("./execution-transport.js");
      await wasm.default();

      const sourceResponse = await fetch(`../${sourcePath}`);
      if (!sourceResponse.ok) throw new Error(`stress source fetch failed: ${sourceResponse.status}`);
      const fixtureSource = await sourceResponse.text();
      const sourceDigest = new Uint8Array(
        await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fixtureSource)),
      );
      const sourceSha256 = [...sourceDigest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
      if (sourceSha256 !== expectedSourceSha256) throw new Error("browser loaded a different stress fixture");

      const authoringStarted = performance.now();
      const authoring = new PythonAuthoringClient();
      await authoring.ready();
      const authored = await authoring.run(fixtureSource, {}, { exportDocument: true });
      const authoringMs = performance.now() - authoringStarted;
      authoring.terminate();
      if (authored.kind !== "scene_document" || authored.sceneSpec === null) {
        throw new Error("explicit stress export did not produce canonical SceneSpec");
      }
      if (authored.semanticExecution !== null && authored.semanticExecution !== undefined) {
        throw new Error("performance export unexpectedly selected static semantic execution");
      }
      if (authored.duration !== duration) {
        throw new Error(`stress duration changed: ${authored.duration}`);
      }
      if (authored.sceneSpec.objects.length !== objectCount) {
        throw new Error(`stress object count changed: ${authored.sceneSpec.objects.length}`);
      }
      if (authored.sceneSpec.tracks.length !== trackCount) {
        throw new Error(`stress track count changed: ${authored.sceneSpec.tracks.length}`);
      }
      const familyAnimations = authored.sceneSpec.family_animations ?? [];
      if (!Array.isArray(familyAnimations) || familyAnimations.length !== 0) {
        throw new Error("stress workload unexpectedly changed retained execution variants");
      }
      const sceneSpecJson = JSON.stringify(authored.sceneSpec);

      const adapter = await navigator.gpu?.requestAdapter();
      const adapterInfo = adapter
        ? {
            vendor: adapter.info?.vendor ?? null,
            architecture: adapter.info?.architecture ?? null,
            device: adapter.info?.device ?? null,
            description: adapter.info?.description ?? null,
          }
        : null;

      const directCanvas = document.createElement("canvas");
      directCanvas.width = 960;
      directCanvas.height = 540;
      directCanvas.dataset.profile = "direct";
      document.body.replaceChildren(directCanvas);
      const engine = new wasm.CanonicalRetainedEngineScenePlayer(sceneSpecJson, duration, 1);
      const rendererCreateStarted = performance.now();
      const renderer = await wasm.RetainedExecutionCanvasRenderer.create(
        directCanvas.transferControlToOffscreen(),
        engine.resourceBundleBytes(),
      );
      const rendererCreateMs = performance.now() - rendererCreateStarted;
      const expectedBackend = backendRequested === "webgpu" ? "WebGPU" : "WebGL2";
      if (renderer.rendererBackend() !== expectedBackend) {
        throw new Error(
          `requested ${expectedBackend}, but direct renderer selected ${renderer.rendererBackend()}`,
        );
      }
      const initial = engine.initialDeltaJson();
      if (!renderer.applyDeltaJson(initial) || !renderer.render()) {
        throw new Error("stress initial retained frame was not presented");
      }

      const samples = [];
      const sampleCount = Math.round(duration * sampleHz);
      for (let index = 0; index <= sampleCount; index += 1) {
        await new Promise(requestAnimationFrame);
        const sceneTime = index / sampleHz;
        const started = performance.now();
        const engineStarted = performance.now();
        const delta = engine.seekDeltaJson(sceneTime);
        const engineMs = performance.now() - engineStarted;
        let transportApplyMs = 0;
        let rendererRenderMs = 0;
        let dirty = false;
        let inlineRenderGeometryCount = 0;
        let renderGeometryResourceCount = 0;
        if (delta !== undefined && delta !== null) {
          const envelope = JSON.parse(delta);
          const applyStarted = performance.now();
          dirty = renderer.applyDeltaJson(delta);
          transportApplyMs = performance.now() - applyStarted;
          if (dirty) {
            const renderStarted = performance.now();
            if (!renderer.render()) throw new Error(`renderer did not present ${sceneTime}`);
            rendererRenderMs = performance.now() - renderStarted;
          }
          inlineRenderGeometryCount = envelope.objects.filter(
            (object) => object.render_geometry !== null && object.render_geometry !== undefined,
          ).length;
          renderGeometryResourceCount = envelope.objects.filter(
            (object) => object.render_geometry_resource !== null &&
              object.render_geometry_resource !== undefined,
          ).length;
        }
        samples.push({
          sceneTime,
          phase: phases.find((phase, phaseIndex) =>
            sceneTime >= phase.start &&
            (sceneTime < phase.end || (phaseIndex === phases.length - 1 && sceneTime <= phase.end)),
          )?.id ?? null,
          dirty,
          engineMs,
          transportApplyMs,
          rendererRenderMs,
          totalMs: performance.now() - started,
          deltaBytes: delta === undefined || delta === null
            ? 0
            : new TextEncoder().encode(delta).byteLength,
          inlineRenderGeometryCount,
          renderGeometryResourceCount,
          geometryCacheMisses: renderer.lastGeometryCacheMisses(),
          uploadBytes: renderer.lastBytesUploaded(),
          drawCalls: renderer.lastDrawCalls(),
          instancesDrawn: renderer.lastInstancesDrawn(),
        });
      }
      const direct = {
        backend: renderer.rendererBackend(),
        rendererCreateMs,
        preloadedGeometryCount: renderer.preloadedGeometryCount(),
        preloadBytesUploaded: renderer.preloadBytesUploaded(),
        visibleObjectCount: renderer.objectCount(),
        samples,
        totals: {
          engineMs: summarizeSamples(samples.map((sample) => sample.engineMs)),
          transportApplyMs: summarizeSamples(samples.map((sample) => sample.transportApplyMs)),
          rendererRenderMs: summarizeSamples(samples.map((sample) => sample.rendererRenderMs)),
          totalMs: summarizeSamples(samples.map((sample) => sample.totalMs)),
        },
        phases: phases.map((phase) => {
          const rows = samples.filter((sample) => sample.phase === phase.id);
          return {
            ...phase,
            samples: rows.length,
            dirtySamples: rows.filter((sample) => sample.dirty).length,
            engineMs: summarizeSamples(rows.map((sample) => sample.engineMs)),
            transportApplyMs: summarizeSamples(rows.map((sample) => sample.transportApplyMs)),
            rendererRenderMs: summarizeSamples(rows.map((sample) => sample.rendererRenderMs)),
            totalMs: summarizeSamples(rows.map((sample) => sample.totalMs)),
            deltaBytes: summarizeSamples(rows.map((sample) => sample.deltaBytes)),
            uploadBytes: summarizeSamples(rows.map((sample) => sample.uploadBytes)),
            geometryCacheMisses: summarizeSamples(rows.map((sample) => sample.geometryCacheMisses)),
          };
        }),
      };
      const reactivation = [];
      for (const sceneTime of reactivationTimes) {
        await new Promise(requestAnimationFrame);
        const delta = engine.seekDeltaJson(sceneTime);
        let inlineRenderGeometryCount = 0;
        let renderGeometryResourceCount = 0;
        let dirty = false;
        if (delta !== undefined && delta !== null) {
          const envelope = JSON.parse(delta);
          dirty = renderer.applyDeltaJson(delta);
          if (dirty && !renderer.render()) {
            throw new Error(`reactivated frame ${sceneTime} was not presented`);
          }
          inlineRenderGeometryCount = envelope.objects.filter(
            (object) => object.render_geometry !== null && object.render_geometry !== undefined,
          ).length;
          renderGeometryResourceCount = envelope.objects.filter(
            (object) => object.render_geometry_resource !== null &&
              object.render_geometry_resource !== undefined,
          ).length;
        }
        reactivation.push({
          sceneTime,
          dirty,
          inlineRenderGeometryCount,
          renderGeometryResourceCount,
          geometryCacheMisses: renderer.lastGeometryCacheMisses(),
          uploadBytes: renderer.lastBytesUploaded(),
          drawCalls: renderer.lastDrawCalls(),
          instancesDrawn: renderer.lastInstancesDrawn(),
        });
      }
      direct.reactivation = reactivation;
      window.__noonRetainedStressCaptureAt = (sceneTime) => {
        const delta = engine.seekDeltaJson(sceneTime);
        if (delta !== undefined && delta !== null) {
          if (!renderer.applyDeltaJson(delta) || !renderer.render()) {
            throw new Error(`capture frame ${sceneTime} was not presented`);
          }
        }
        return {
          sceneTime: engine.time(),
          objects: renderer.objectCount(),
          drawCalls: renderer.lastDrawCalls(),
          uploadBytes: renderer.lastBytesUploaded(),
          geometryCacheMisses: renderer.lastGeometryCacheMisses(),
        };
      };
      window.__noonRetainedStressDispose = () => {
        renderer.free?.();
        engine.free?.();
        delete window.__noonRetainedStressCaptureAt;
        delete window.__noonRetainedStressDispose;
      };

      async function runWorkerMode(transportMode) {
        const canvas = document.createElement("canvas");
        canvas.width = 960;
        canvas.height = 540;
        canvas.dataset.profile = transportMode;
        document.body.append(canvas);
        const errors = [];
        const client = new ExecutionWorkerClient(canvas, {
          onError(error, owner) {
            errors.push(`${owner}: ${error}`);
          },
        });
        const started = performance.now();
        const ready = await client.startRetainedCanonical(sceneSpecJson, {
          loopDurationSeconds: duration,
          transportMode,
          sharedSlotCapacity: 32 * 1024 * 1024,
        });
        if (ready.render.backend !== expectedBackend) {
          client.terminate();
          canvas.remove();
          throw new Error(
            `requested ${expectedBackend}, but ${transportMode} renderer selected ${ready.render.backend}`,
          );
        }
        if (ready.render.time !== 0 || ready.render.presentedFrames !== 1) {
          client.terminate();
          canvas.remove();
          throw new Error(
            `${transportMode} renderer advanced before ready: time=${ready.render.time}, ` +
              `frames=${ready.render.presentedFrames}`,
          );
        }
        const startupMs = performance.now() - started;
        await client.pause();
        const morphWarmSeeks = phases
          .filter((phase) => phase.id === "morph-a" || phase.id === "morph-b")
          .flatMap((phase) => [phase.start + 1 / sampleHz, phase.start + 2 / sampleHz]);
        const seekTimes = [...new Set(
          phases.map((phase) => phase.start).concat(morphWarmSeeks, duration - 1 / sampleHz),
        )].sort((left, right) => left - right);
        const seeks = [];
        for (const sceneTime of seekTimes) {
          const before = await client.metrics();
          const expectPresentation = sceneTime > 0;
          const seekStarted = performance.now();
          await client.seek(sceneTime);
          let after = null;
          const deadline = performance.now() + 15_000;
          do {
            after = await client.metrics();
            if (
              Math.abs(after.engineMetrics.time - sceneTime) <= 1e-9 &&
              (!expectPresentation || after.metrics.presentedFrames > before.metrics.presentedFrames)
            ) break;
            await new Promise((resolve) => setTimeout(resolve, 4));
          } while (performance.now() < deadline);
          if (Math.abs(after.engineMetrics.time - sceneTime) > 1e-9) {
            throw new Error(`${transportMode} engine did not converge to ${sceneTime}`);
          }
          if (expectPresentation && after.metrics.presentedFrames <= before.metrics.presentedFrames) {
            throw new Error(`${transportMode} renderer did not present ${sceneTime}`);
          }
          seeks.push({
            sceneTime,
            roundTripToPresentedMs: performance.now() - seekStarted,
            rendererTime: after.metrics.time,
            engineTime: after.engineMetrics.time,
            presentedFrames: after.metrics.presentedFrames,
            bufferedDeltas: after.metrics.bufferedDeltas,
            uploadBytes: after.metrics.bytesUploaded,
            geometryCacheMisses: after.metrics.geometryCacheMisses,
            visibleObjectCount: after.metrics.objectCount,
          });
        }
        await client.restartPlayback();
        const cadenceBefore = await client.metrics();
        const cadenceStarted = performance.now();
        await client.resume();
        await new Promise((resolve) => setTimeout(resolve, duration * workerLoops * 1000));
        await client.pause();
        const cadenceWallMs = performance.now() - cadenceStarted;
        const cadenceAfter = await client.metrics();
        const cadencePresentedFrames =
          cadenceAfter.metrics.presentedFrames - cadenceBefore.metrics.presentedFrames;
        const finalMetrics = await client.metrics();
        client.terminate();
        client.canvas.remove();
        canvas.remove();
        if (errors.length > 0) throw new Error(`${transportMode} worker errors: ${errors.join("\n")}`);
        return {
          transportMode,
          startupMs,
          backend: ready.render.backend,
          preloadedGeometryCount: ready.render.preloadedGeometryCount,
          preloadBytesUploaded: ready.render.preloadBytesUploaded,
          seeks,
          seekRoundTripMs: summarizeSamples(seeks.map((seek) => seek.roundTripToPresentedMs)),
          cadence: {
            loops: workerLoops,
            wallMs: cadenceWallMs,
            presentedFrames: cadencePresentedFrames,
            effectiveFps: cadencePresentedFrames * 1000 / cadenceWallMs,
            engineTime: cadenceAfter.engineMetrics.time,
            bufferedDeltas: cadenceAfter.metrics.bufferedDeltas,
          },
          finalMetrics,
        };
      }

      const workerModes = [];
      workerModes.push(await runWorkerMode(EXECUTION_TRANSPORT_TRANSFERABLE));
      if (crossOriginIsolated && typeof SharedArrayBuffer === "function") {
        workerModes.push(await runWorkerMode(EXECUTION_TRANSPORT_SHARED));
      }
      return {
        sourceSha256,
        adapterInfo,
        crossOriginIsolated,
        authoring: {
          authoringMs,
          duration: authored.duration,
          objectCount: authored.sceneSpec.objects.length,
          trackCount: authored.sceneSpec.tracks.length,
          familyAnimationCount: familyAnimations.length,
          sceneSpecBytes: new TextEncoder().encode(sceneSpecJson).byteLength,
        },
        direct,
        workerModes,
      };
    },
    {
      expectedSourceSha256: sourceSha256,
      sourcePath,
      phases: STRESS_PHASES,
      duration: STRESS_DURATION_SECONDS,
      objectCount: STRESS_OBJECT_COUNT,
      trackCount: STRESS_TRACK_COUNT,
      sampleHz: STRESS_SAMPLE_HZ,
      workerLoops,
      backendRequested: backend,
      reactivationTimes: STRESS_REACTIVATION_TIMES,
    },
  );

  // Preserve expensive device measurements even when a regression oracle below
  // rejects the run. Successful validation overwrites this provisional artifact
  // with the complete host/capture record.
  await mkdir(path.dirname(artifactPath), { recursive: true });
  await writeFile(
    artifactPath,
    `${serializeArtifact({
      schemaVersion: 1,
      benchmark: "Noon retained Dynamic Load Stress regression profile",
      generatedAt: new Date().toISOString(),
      commit: commit.status === 0 ? commit.stdout.trim() : null,
      validation: "pending",
      backendRequested: backend,
      result,
    })}\n`,
  );

  assert.equal(result.sourceSha256, sourceSha256);
  assert.equal(result.authoring.duration, STRESS_DURATION_SECONDS);
  assert.equal(result.authoring.objectCount, STRESS_OBJECT_COUNT);
  assert.equal(result.authoring.trackCount, STRESS_TRACK_COUNT);
  assert.equal(result.authoring.familyAnimationCount, 0);
  assert.equal(result.direct.visibleObjectCount, STRESS_FINAL_VISIBLE_OBJECT_COUNT);
  assert.ok(result.direct.rendererCreateMs > 0, "renderer preload duration must be measured");
  assert.ok(result.direct.preloadedGeometryCount >= 1200, "renderer must preload both morph sets");
  assert.ok(result.direct.preloadBytesUploaded > 0, "renderer preload must report GPU upload bytes");
  assert.equal(result.direct.samples.length, STRESS_DURATION_SECONDS * STRESS_SAMPLE_HZ + 1);
  assert.deepEqual(
    result.direct.phases.map((phase) => phase.id),
    STRESS_PHASES.map((phase) => phase.id),
  );
  const steadyTelemetry = assertSteadyStressTelemetry(result.direct.samples);
  const retainedReactivation = assertRetainedMorphReactivation(result.direct.reactivation);
  const firstMorphActivationLatency = assertFirstMorphActivationLatency(
    result.direct.samples,
    result.direct.backend,
  );
  assert.deepEqual(
    result.workerModes.map((mode) => mode.transportMode).sort(),
    ["shared", "transferable"],
    "cross-origin-isolated stress profile must exercise both worker transports",
  );
  for (const mode of result.workerModes) {
    assert.equal(mode.finalMetrics.metrics.retained, true);
    assert.equal(mode.finalMetrics.engineMetrics.canonical, true);
    assert.ok(mode.preloadedGeometryCount >= 1200);
    assert.ok(mode.preloadBytesUploaded > 0);
    assert.equal(mode.finalMetrics.metrics.preloadedGeometryCount, mode.preloadedGeometryCount);
    assert.equal(mode.finalMetrics.metrics.preloadBytesUploaded, mode.preloadBytesUploaded);
    assert.ok(mode.seeks.length >= STRESS_PHASES.length);
    assert.equal(
      mode.seeks.at(-1).visibleObjectCount,
      STRESS_FINAL_VISIBLE_OBJECT_COUNT,
      `${mode.transportMode} final deterministic seek has the wrong visible object count`,
    );
    assert.ok(mode.cadence.presentedFrames > 0, `${mode.transportMode} cadence presented no frames`);
    assert.ok(
      Number.isFinite(mode.cadence.effectiveFps) && mode.cadence.effectiveFps > 0,
      `${mode.transportMode} cadence did not produce a measurable frame rate`,
    );
  }
  assert.deepEqual(browserErrors, [], browserErrors.join("\n"));

  const captures = [];
  for (const [id, sceneTime] of [
    ["morph-a", 1.2],
    ["morph-b", 2.45],
    ["lifecycle-churn", 4.3],
    ["final-wave", 4.7],
  ]) {
    const metrics = await page.evaluate((time) => window.__noonRetainedStressCaptureAt(time), sceneTime);
    assert.ok(Math.abs(metrics.sceneTime - sceneTime) <= 1e-9, `${id} capture time drifted`);
    const capturePath = screenshotPath.replace(/\.png$/u, `-${id}.png`);
    await page.locator("canvas[data-profile='direct']").screenshot({ path: capturePath });
    captures.push({ id, sceneTime, path: path.relative(repoRoot, capturePath), metrics });
  }
  await page.evaluate(() => window.__noonRetainedStressDispose());
  const artifact = {
    schemaVersion: 1,
    benchmark: "Noon retained Dynamic Load Stress regression profile",
    generatedAt: new Date().toISOString(),
    commit: commit.status === 0 ? commit.stdout.trim() : null,
    fixture: {
      source: sourcePath,
      sha256: sourceSha256,
      durationSeconds: STRESS_DURATION_SECONDS,
      expectedObjects: STRESS_OBJECT_COUNT,
      expectedTracks: STRESS_TRACK_COUNT,
      sampleHz: STRESS_SAMPLE_HZ,
      workerLoops,
      phases: STRESS_PHASES,
    },
    host: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpu: os.cpus()[0]?.model ?? null,
      logicalCpuCount: os.cpus().length,
      node: process.version,
      browser: await browser.version(),
    },
    backendRequested: backend,
    captures,
    steadyTelemetry,
    retainedReactivation,
    firstMorphActivationLatency,
    ...result,
  };
  await writeFile(artifactPath, `${serializeArtifact(artifact)}\n`);
  console.log(
    `retained Dynamic Load Stress ${backend}: ${result.direct.backend}, ` +
      `${result.direct.totals.totalMs.p95.toFixed(2)} ms direct p95, ` +
      `${result.workerModes.length} worker transports; wrote ${path.relative(repoRoot, artifactPath)}`,
  );
  await context.close();
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}

function browserArgs(mode) {
  if (mode === "webgpu") {
    return [
      "--enable-unsafe-webgpu",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
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

function positiveInteger(value, label) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be positive`);
  return parsed;
}

function serializeArtifact(value) {
  return JSON.stringify(
    value,
    (_key, item) => typeof item === "bigint" ? item.toString() : item,
    2,
  );
}

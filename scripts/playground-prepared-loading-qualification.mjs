// Compare separately built normal and prepared packages over ordinary streamed
// HTTP. Each package root contains pkg/ and runtime-build-identity.json. Sources
// are unchanged; only the existing qualification run option selects preparation.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { readFile, writeFile, mkdir, readdir, symlink, mkdtemp, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { createProcessTreeRssSampler } from "./playground-cold-start-memory.mjs";
import { browserArgs, classifyRendererGpuIdentity } from "./manim-raster-support.mjs";
import { sceneInkPixels, summarizeLoading } from "./playground-prepared-loading-observation.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const env = process.env;
const trials = Number(env.NOON_PLAYBACK_LOADING_TRIALS ?? 6);
assert.ok(Number.isSafeInteger(trials) && trials > 0);
const backend = env.NOON_PLAYBACK_LOADING_BACKEND ?? "webgpu";
assert.ok(["webgpu", "webgl"].includes(backend));
const comparisonArm = env.NOON_PLAYBACK_LOADING_COMPARISON ?? "prepared";
assert.ok(["prepared", "live-control"].includes(comparisonArm));
const wasmAccounting = env.NOON_PLAYBACK_LOADING_WASM_ACCOUNTING === "1";
const seekProbe = env.NOON_PLAYBACK_LOADING_SEEK === "1";
const output = path.resolve(env.NOON_PLAYBACK_LOADING_OUTPUT ?? path.join(os.tmpdir(), "noon-playback-loading"));
const examples = (env.NOON_PLAYBACK_LOADING_EXAMPLES ??
  "parity-create-circle,showcase-code-blocks,showcase-gpu-glow,showcase-cuda-transpose,showcase-triangle-light,showcase-pointer-selection,showcase-always-redraw,showcase-translation-drag").split(",");
const encode = value => JSON.stringify(value, (_key, item) => typeof item === "bigint" ? String(item) : item, 2);
const digest = value => createHash("sha256").update(value).digest("hex");
function gitObject(name) {
  const result = spawnSync("git", ["rev-parse", name], { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, `missing source object ${name}`);
  return result.stdout.trim();
}
const revision = gitObject("HEAD");
const runtimeTrees = commit => ({ crates: gitObject(`${commit}:crates`), web: gitObject(`${commit}:web`), lock: gitObject(`${commit}:Cargo.lock`) });
const inventory = (await Promise.all(["manim_tutorial_manifest.json", "manim_compatibility_manifest.json",
  "manim_stress_manifest.json", "noon_showcase_manifest.json"].map(async name =>
  JSON.parse(await readFile(path.join(root, "web/python/examples", name), "utf8")).entries))).flat();
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
const report = { status: "running", scope: "loading observations on unchanged catalog sources; fresh browser process followed by reload/new workers; not a warm-interpreter, cold-network, physical-scanout, full-source or total-memory qualification",
  harnessRevision: revision, runtimeSourceTrees: runtimeTrees(revision), harnessSha256: digest(await readFile(fileURLToPath(import.meta.url))),
  backend, trials, comparisonArm, wasmAccounting, seekProbe,
  launchArgs: browserArgs(backend, { gpuMode: "hardware" }), observer: { inkThreshold: 128, channelDifference: 8, edgeMargin: 8, pollIntervalMs: 25,
    usefulTiming: "upper bound at completion of the first positive canvas capture; native first-any-frame time is separate",
    memoryWindow: "navigation/reload through first visible observation plus the recorded post-observation window and optional seek; aggregate sampled process-tree RSS",
    progress: "sampled renderer publications and UI observations; short buffering episodes can fall between samples",
    runPromise: "inFlight is the page run promise, including continuation and handoff waits; it does not measure source CPU activity or prove parallel preparation",
    wasmCapacity: "optional probe changes worker entrypoints and HTTP caching; instrumented timings are diagnostic only. Linear memory capacity is not live allocation, RSS, GPU or total memory." },
  host: { platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0]?.model,
    logicalCpus: os.cpus().length, totalMemoryBytes: os.totalmem() }, packages: {}, cases: [] };
await mkdir(output, { recursive: true });
let overlay, browser, browserServer;
const servers = {};

async function fileHash(filename) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest("hex");
}

async function packageHost(arm, folder) {
  assert.ok(folder, `NOON_PLAYBACK_LOADING_${arm.toUpperCase()}_ROOT is required`);
  folder = path.resolve(folder);
  const identity = JSON.parse(await readFile(path.join(folder, "runtime-build-identity.json"), "utf8"));
  assert.match(identity.sourceRevision, /^[0-9a-f]{40}$/, `${arm} needs a clean recorded package source revision`);
  assert.deepEqual(runtimeTrees(identity.sourceRevision), report.runtimeSourceTrees,
    `${arm} package Rust, web and lockfile sources must match the measured host`);
  for (const [name, entry] of Object.entries(identity.files)) {
    const filename = name === "wasm" || name === "glue"
      ? path.join(folder, entry.path.replace(/^\.\//, "")) : path.join(root, "web", entry.path);
    assert.equal(await fileHash(filename), entry.sha256, `${arm} ${name} package identity drift`);
  }
  const host = path.join(overlay, arm);
  await mkdir(path.join(host, "web"), { recursive: true });
  for (const entry of await readdir(root)) {
    if (entry === "web" || entry.startsWith(".") || entry === "node_modules") continue;
    await symlink(path.join(root, entry), path.join(host, entry));
  }
  for (const entry of await readdir(path.join(root, "web"))) {
    if (entry === "pkg" || entry === "runtime-build-identity.json") continue;
    await symlink(path.join(root, "web", entry), path.join(host, "web", entry));
  }
  await symlink(path.join(folder, "pkg"), path.join(host, "web/pkg"));
  await symlink(path.join(folder, "runtime-build-identity.json"), path.join(host, "web/runtime-build-identity.json"));
  const server = await serveRepository(host, 0, { crossOriginIsolated: true });
  report.packages[arm] = { identity, wasmBytes: (await stat(path.join(folder, "pkg/noon_web_bg.wasm"))).size };
  return server;
}

async function observation(page) {
  return observationDeadline(page.evaluate(async () => {
    const gallery = globalThis.__noonExampleGallery;
    let response = null;
    try { response = await gallery?.executionMetrics?.(); } catch {}
    const controls = document.querySelector(".playback-controls");
    const prefix = controls?.querySelector(".playback-prepared-seek");
    return { metrics: response?.metrics, run: gallery?.runPhaseMetrics,
      controls: controls ? { ...controls.dataset,
        preparedFrontier: prefix && !prefix.hidden ? Number(prefix.max) : null } : null,
      inFlight: gallery?.runInFlight, selected: gallery?.selectedExampleId,
      status: document.querySelector("#patch-status")?.dataset.state,
      problem: document.querySelector("#authoring-problem")?.textContent,
      epochMs: performance.timeOrigin + performance.now(), originMs: performance.timeOrigin };
  }));
}

async function observationDeadline(pending) {
  let timer;
  try {
    return await Promise.race([pending, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("metrics observation exceeded 30 seconds")), 30_000);
    })]);
  } finally { clearTimeout(timer); }
}

async function installWasmAccounting(page) {
  const entries = new Set(["/web/python-worker.js", "/web/execution-render-worker.js"]);
  await page.route(url => entries.has(new URL(url).pathname), async route => {
    // Preserve the worker's later integrity-verification fetch unchanged.
    if (!["script", "worker"].includes(route.request().resourceType())) return route.continue();
    const response = await route.fetch(), headers = { ...response.headers() };
    delete headers["content-length"]; delete headers["content-encoding"];
    await route.fulfill({ response, headers,
      body: `import "./playground-cold-start-wasm-accounting.js";\n${await response.text()}` });
  });
}

async function measureSeek(page, settled) {
  if (settled.controls?.controllable !== "true") {
    return { available: false, reason: "source-owned forward execution has no retained seek range" };
  }
  if (settled.controls.playing === "true") await page.getByRole("button", { name: "Pause animation", exact: true }).click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "false");
  const issued = await page.evaluate(() => {
    const controls = document.querySelector(".playback-controls");
    if (controls.dataset.controllable !== "true") return null;
    const open = controls.querySelector(".playback-prepared-seek");
    const input = open.hidden ? controls.querySelector(".playback-scrubber") : open;
    const epochMs = performance.timeOrigin + performance.now();
    input.value = "0";
    input.dispatchEvent(new Event(input === open ? "change" : "input", { bubbles: true }));
    return { epochMs, target: 0 };
  });
  if (issued === null) return { available: false, reason: "the source retired its reader before the seek" };
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    return controls.dataset.busy === "false" && controls.dataset.elapsedSeconds === "0";
  });
  const after = await observation(page);
  assert.notEqual(after.status, "error", after.problem);
  assert.equal(after.controls.elapsedSeconds, "0");
  assert.equal(after.controls.playing, "false");
  return { available: true, ...issued, commandCompletionUpperMs: after.epochMs - issued.epochMs,
    meaning: "cached UI seek to zero through its actual command/presentation acknowledgement, plus polling and observation; no physical scanout claim", after };
}

async function measure(page, workers, example, arm, phase, trial, warmup) {
  const postObservationWindowMs = ["showcase-pointer-selection", "showcase-translation-drag"].includes(example) ? 4000 : 500;
  const row = { example, arm, phase, trial, warmup, status: "running", inkAttempts: 0, captures: [],
    postObservationWindowMs, preparedPlaybackRequested: arm === "prepared", progress: [], loadAvg: os.loadavg(),
    freeMemoryBytes: os.freemem(), pageErrors: [], consoleErrors: [] };
  report.cases.push(row);
  const onPageError = error => row.pageErrors.push(String(error));
  const onConsole = message => { if (message.type() === "error") row.consoleErrors.push(message.text()); };
  page.on("pageerror", onPageError); page.on("console", onConsole);
  const sampler = createProcessTreeRssSampler(browserServer.process().pid); sampler.start();
  workers.length = 0;
  try {
    const catalog = example.startsWith("showcase-") ? "&catalog=showcase" : "";
    if (phase === "fresh-process") await page.goto(`${servers[arm].baseUrl}/web/index.html?example=${example}${catalog}`);
    else await page.reload();
    await page.addStyleTag({ content: ".canvas-wrap { background: #090c13 !important; } #scene { border-radius: 0 !important; box-shadow: none !important; }" });
    const deadline = performance.now() + 180_000;
    let last;
    while (performance.now() < deadline) {
      const started = performance.now();
      last = await observation(page);
      row.lastObservation = last;
      if (last.status === "error") throw new Error(last.problem || "source failed");
      const metrics = last.metrics;
      if (metrics?.ready && Number.isFinite(metrics.firstPresentedSessionAtMs) && metrics.objectCount > 0
          && last.run?.exampleId === example && last.selected === example) {
        const captureStarted = performance.now();
        const bytes = await page.locator("#scene").screenshot();
        const after = await observation(page);
        row.inkAttempts++;
        const ink = sceneInkPixels(PNG.sync.read(bytes));
        row.captures.push({ beforeEpochMs: last.epochMs, afterEpochMs: after.epochMs,
          sceneInkPixels: ink, captureAndObservationMs: performance.now() - captureStarted,
          presentedSession: metrics.presentedSession, frameTimeBefore: metrics.time, frameTimeAfter: after.metrics?.time });
        if (ink >= 128 && after.run?.runGeneration === last.run.runGeneration && after.selected === example
            && after.metrics?.presentedSession === metrics.presentedSession) {
          if (after.status === "error") throw new Error(after.problem || "source failed during capture");
          const sourceEpoch = last.originMs + last.run.sourceRunStartedAtMs;
          const firstAnyEpoch = metrics.performanceTimeOriginMs + metrics.firstPresentedSessionAtMs;
          assert.ok(Number.isFinite(sourceEpoch) && Number.isFinite(firstAnyEpoch) && firstAnyEpoch >= sourceEpoch,
            "authored render needs a coherent native/page timing association");
          Object.assign(row, { sourceToFirstAnyPresentedMs: firstAnyEpoch - sourceEpoch,
            sourceToUsefulObservationUpperMs: after.epochMs - sourceEpoch,
            navigationToUsefulObservationUpperMs: after.epochMs - last.originMs,
            captureAndObservationMs: performance.now() - captureStarted,
            observationRoundTripMs: captureStarted - started, sceneInkPixels: ink,
            runPromisePendingAtUsefulObservation: after.inFlight,
            usefulObservation: after, presentedBeforeCapture: metrics });
          assert.equal(metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
          await writeFile(path.join(output, `${example}-${trial}-${arm}-${phase}.png`), bytes);
          break;
        }
      }
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    assert.ok(Number.isFinite(row.sourceToUsefulObservationUpperMs), "no useful authored-scene observation");
    const progressEnd = performance.now() + postObservationWindowMs;
    while (performance.now() < progressEnd) {
      const sample = await observation(page);
      row.progress.push({ epochMs: sample.epochMs, inFlight: sample.inFlight, status: sample.status,
        time: sample.metrics?.time, presentedFrames: sample.metrics?.presentedFrames,
        presentedSession: sample.metrics?.presentedSession, controls: sample.controls });
      if (sample.status === "error") throw new Error(sample.problem || "source failed in startup window");
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    const settled = await observation(page);
    row.settledObservation = settled;
    if (settled.status === "error") throw new Error(settled.problem || "source failed in startup window");
    assert.deepEqual(row.pageErrors, []);
    assert.deepEqual(row.consoleErrors, []);
    if (seekProbe) row.seek = await measureSeek(page, settled);
    if (wasmAccounting) row.wasm = await Promise.all(workers.filter(worker => /\/(python-worker|execution-render-worker)\.js/.test(worker.url())).map(async worker => ({
      url: worker.url(), accounting: await worker.evaluate(() => {
        const probe = globalThis.__noonColdStartWasmAccounting;
        return probe ? { records: probe.records, linearMemory: probe.snapshot() } : null;
      }),
    })));
    row.status = "observed";
  } catch (error) {
    row.status = "failed"; row.error = String(error.stack ?? error);
    await page.screenshot({ path: path.join(output, `${example}-${trial}-${arm}-${phase}-failure.png`) }).catch(() => {});
  } finally {
    row.memory = await sampler.stop();
    if (!row.memory.enabled || row.memory.sampleCount === 0 || row.memory.samplingError) {
      row.status = "failed"; row.memoryError = row.memory.samplingError ?? "no RSS observations";
    }
    page.off("pageerror", onPageError); page.off("console", onConsole);
    await writeFile(path.join(output, "report.json"), encode(report));
    console.log(JSON.stringify({ example, trial, arm, phase, warmup, status: row.status,
      sourceToUsefulObservationUpperMs: row.sourceToUsefulObservationUpperMs, error: row.error }));
  }
  return row;
}

try {
  overlay = await mkdtemp(path.join(os.tmpdir(), "noon-playback-loading-host-"));
  servers.live = await packageHost("live", env.NOON_PLAYBACK_LOADING_LIVE_ROOT);
  servers[comparisonArm] = await packageHost(comparisonArm, comparisonArm === "prepared"
    ? env.NOON_PLAYBACK_LOADING_PREPARED_ROOT : env.NOON_PLAYBACK_LOADING_LIVE_ROOT);
  for (const example of examples) {
    const entry = inventory.find(item => item.id === example);
    assert.ok(entry, `unknown unchanged example ${example}`);
    const sourceHash = await fileHash(path.join(root, "web", entry.path));
    for (let trial = -1; trial < trials; trial++) {
      // Retain warm-up evidence but exclude its dependency downloads from timing summaries.
      for (const arm of trial % 2 === 0 ? ["live", comparisonArm] : [comparisonArm, "live"]) {
        browserServer = await playwright.chromium.launchServer({ headless: true,
          ...(env.NOON_PLAYBACK_LOADING_EXECUTABLE ? { executablePath: env.NOON_PLAYBACK_LOADING_EXECUTABLE } : { channel: "chromium" }),
          args: browserArgs(backend, { gpuMode: "hardware" }) });
        try {
          browser = await playwright.chromium.connect(browserServer.wsEndpoint());
          report.browserVersion = browser.version();
          const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
          await cache.install(context);
          await context.addInitScript(({ prepared }) => {
            globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ = { authoringRunOptions: () => ({ preparedPlayback: prepared, preparationLookaheadSeconds: 1 }) };
          }, { prepared: arm === "prepared" });
          const page = await context.newPage(); page.setDefaultTimeout(180_000);
          const workers = []; page.on("worker", worker => workers.push(worker));
          if (wasmAccounting) await installWasmAccounting(page);
          for (const phase of ["fresh-process", "reload-new-workers"]) {
            const row = await measure(page, workers, example, arm, phase, trial, trial < 0);
            row.sourcePath = entry.path; row.sourceSha256 = sourceHash;
            if (row.status === "failed") break;
          }
          const identity = await page.evaluate(async () => (await __noonExampleGallery.executionMetrics({ includeGpuIdentity: true })).metrics.rendererGpuIdentity).catch(() => null);
          const rows = report.cases.filter(row => row.example === example && row.arm === arm && row.trial === trial);
          for (const row of rows) { row.rendererGpuIdentity = identity; row.gpuClassification = identity ? classifyRendererGpuIdentity(identity, backend) : null; }
          await context.close();
        } finally {
          try { await browser?.close(); }
          finally { await browserServer.close(); browser = null; browserServer = null; }
        }
      }
    }
    assert.equal(await fileHash(path.join(root, "web", entry.path)), sourceHash, "source changed during measurement");
  }
  report.status = report.cases.every(row => row.status === "observed") ? "observed" : "incomplete";
  report.summaries = summarizeLoading(report.cases, examples, trials, comparisonArm);
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); process.exitCode = 1;
} finally {
  report.resourceCache = cache.stats();
  await writeFile(path.join(output, "report.json"), encode(report));
  try { await browser?.close(); }
  finally {
    try { await browserServer?.close(); }
    finally {
      for (const server of Object.values(servers)) await server.close();
      if (overlay) await rm(overlay, { recursive: true, force: true });
    }
  }
}
if (report.status !== "observed") process.exitCode = 1;

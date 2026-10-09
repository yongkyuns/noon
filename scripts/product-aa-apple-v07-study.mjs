// #1933 diagnostic only. Invoke the frozen #1875 measurement and scoring code.
// No engine edits, score overrides, result-dependent scheduling, or retries.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync, spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
const WORKLOADS = [
  "parity-square-and-circle", "showcase-camera-follows-path",
  "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths",
];
const LATENCIES = ["shell ready", "cold Run → applied", "warm Run → applied", "edit → applied"];
const EXPECTED_ENV = {
  NOON_PRODUCT_MAX_LATENCY_RATIO: "1.25", NOON_PRODUCT_LATENCY_SLACK_MS: "350",
  NOON_PRODUCT_MIN_FPS_RATIO: "0.80", NOON_PRODUCT_STRICT_MIN_FPS_RATIO: "0.97",
  NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO: "0.015",
};

export function validatePlan(p) {
  assert.equal(p.schema, 1);
  assert.equal(p.kind, "diagnostic-only");
  assert.equal(p.qualification, false);
  assert.equal(p.issue, 1933);
  assert.equal(p.qualificationPr, 1875);
  assert.equal(p.studyId, "1933-product-aa-20261009-07", "frozen Apple A/A study changed");
  assert.equal(p.harnessSha, "439c4c4d33096665afa68bb070baaba52bd20f43",
    "Apple Metal measurement overlay commit differs from the approved frozen harness");
  assert.equal(p.harnessTree, "3cc0f65789861939992bf5e42b0095a4f71b74a1",
    "Apple Metal measurement overlay tree differs from the approved frozen harness");
  assert.deepEqual(p.workloads, WORKLOADS);
  assert.deepEqual(Object.keys(p.authoredSourceHashes ?? {}).sort(), [...WORKLOADS].sort(),
    "all five original authored-source hashes must be pinned");
  for (const workload of WORKLOADS) {
    assert.match(p.authoredSourceHashes[workload], /^[a-f0-9]{64}$/,
      `invalid authored-source hash for ${workload}`);
  }
  assert.deepEqual(p.priorInvalidStudy, {
    studyId: "1933-product-aa-20261008-01", runId: 37797078117,
    artifactId: 11558563159,
    artifactSha256: "0b1ba75f6559b492455a924c7a22ed509abf11a34a63af807322fcb1c5e24d47",
    completedPairCommands: 0,
    reason: "Diagnostic used raw example source SHA instead of frozen product harness authored-source SHA",
  });
  assert.deepEqual(p.cohorts, [{ name: "seven-pair", pairs: 7 }, { name: "three-pair", pairs: 3 }]);
  assert.deepEqual(p.builds.map(b => b.name), ["baseline", "candidate"]);
  assert.equal(p.sourcePair.schema, 1);
  for (const name of ["baseline", "candidate", "head", "eventBase"]) {
    assert.match(p.sourcePair[name], /^[a-f0-9]{40}$/);
  }
  assert.equal(p.sourcePair.head, p.apple?.diagnosticOverlaySource, "original production head must remain pinned, separate from Apple Metal harness overlay");
  assert.ok(Number.isSafeInteger(p.producerRun) && p.producerRun > 0);
  for (const build of p.builds) {
    assert.equal(build.source, p.sourcePair[build.name]);
    assert.ok(Number.isSafeInteger(build.artifactId) && build.artifactId > 0);
    assert.match(build.zipSha256, /^[a-f0-9]{64}$/);
    assert.match(build.buildId, /^[a-f0-9]{64}$/);
  }
  assert.equal(new Set(p.builds.map(b => b.artifactId)).size, 2);
  assert.equal(p.retries, 0);
  assert.equal(p.stopOnTrialError, true);
  assert.equal(p.plannedPairCommands, 100);
  assert.equal(p.plannedObservations, 200);
  assert.deepEqual(p.execution, {
    event: "pull_request.opened", allowedRunAttempt: 1,
    pairDeadlineSeconds: 420, termGraceSeconds: 5, jobDeadlineMinutes: 180,
  });
  assert.equal(p.environment.playwright, "1.62.1");
  assert.equal(p.environment.pngjs, "7.0.0");
  assert.equal(p.environment.node, "22.23.3");
  assert.equal(p.environment.browserVersion, "151.0.7922.34");
  assert.equal(p.environment.renderer, "WebGL2");
  assert.equal(p.environment.browserMode, "hardware-WebGL");
  assert.equal(p.environment.runner, "macos-15");
  assert.equal(p.environment.port, 4205);
  assert.equal(p.environment.headless, false, "Apple Metal A/A requires tested headed Chromium");
  assert.equal(p.environment.display, "native macOS headed display");
  assert.equal(p.environment.expectedUnmaskedRenderer, "Apple Paravirtual");
  assert.deepEqual(p.apple, {
    diagnosticOverlaySource: "f5ec15c7a1a9a70c368e141abfc740f5081efc4e",
    diagnosticOverlayFiles: ["scripts/playground-product-pair.mjs", "scripts/playground-product-e2e.mjs", "scripts/product-aa-apple-http-server.py"],
    actualRenderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple Paravirtual device)",
    rendererProof: "macos-renderer-proof-${pair}.json",
    browserFlags: ["--disable-features=WebGPU,WebGPUService,WebGPUBlobCache", "--use-gpu-in-tests", "--ignore-gpu-blocklist",
      "--force-high-performance-gpu", "--disable-gpu-sandbox", "--disable-dev-shm-usage"],
    environment: { os: "macos-15", arch: "arm64", browserHeadless: false,
      runnerTier: "standard public-repository GitHub hosted" },
    feasibility: { run: 37968230377, artifactId: 11634228899,
      sha256: "3991d67fe47e3b0eb75796f342bd8d99e7e8a538ea54943e7763e1e9e4a76607" },
    expectedPairProofCount: 100, requiresAttestedDeviceEachPair: true,
    qualification: false, performanceAcceptance: false, mergeApproval: false,
  }, "hosted standard Apple Metal environment plan changed");
  assert.deepEqual(p.priorAppleFailedAttempt, {
    studyId: "1933-product-aa-20261009-05",
    run: 37977203253, artifactId: 11639543175,
    zipSha256: "6c5c4c6339b6b396b73c873eb529047693d04a51b478bc8e0a09d13db2b43074",
    completedPairCommands: 0,
    reason: "Python 3.14 -m http.server spawned but never listened; initial A/A static E2E readiness timed out before sample",
  }, "prior failed Apple A/A acquisition must remain immutable");
  assert.deepEqual(p.serverPreflight, {
    run: 37980247462, artifactId: 11640943016,
    zipSha256: "99b3dfd118c46e47ee54ca979d5425b3a77000c2b085408f4a9d979a0d47bf65",
    originalPythonHttpCli: false, directSocketServerSucceeded: true,
    staticServerScript: "scripts/product-aa-apple-http-server.py",
    readiness: "unique random token HTTP response, then complete runtime-build-identity.json comparison; no dependence on stdout banner",
    threaded: true, expectedPython: "3.14.7",
  }, "preregistered original CLI/TCPServer diagnostic provenance changed");
  assert.deepEqual(p.priorAppleWebgpuFailedAttempt, {
    studyId: "1933-product-aa-20261009-06",
    run: 37982867044, artifactId: 11642047201,
    zipSha256: "b5e65388c329503ac2ff5e4e2b1d98f8bcbe6a128923280601a6f0c6beb5b8f8",
    completedPairCommands: 0,
    reason: "Verified Python socket/static files and Apple Metal WebGL2, but actual original Noon runtime selected WebGPU; strict first product trial rejected prior to scoring.",
  }, "historical v06 runtime WebGPU exclusion must not be rewritten");
  assert.deepEqual(p.actualWebglSelection, {
    diagnosticRun: 37984673136, pr: 1971, artifactId: 11642288589,
    zipSha256: "c34c869dab2c91e10e1395665f1b46cdc102c6aaabf4750d32f71f00557b3e03",
    chosenCase: "webgpu-service-off",
    requiredDisabledFeatures: ["WebGPU", "WebGPUService", "WebGPUBlobCache"],
    observedOriginalNoonBackend: "WebGL2",
    observedAdapter: "unavailable",
    source: "original fixed four-case cold original Noon scene on same standard hosted Apple Metal",
    qualification: false, performanceAcceptance: false, mergeApproval: false,
  }, "proven real original Noon WebGL2 selection must remain preregistered");
  assert.equal(p.pairRunner, "scripts/playground-product-pair.mjs");
  assert.equal(p.comparator, "scripts/playground-product-compare.mjs");
  assert.equal(p.scorer, "scripts/paired-product-metrics.mjs");
  return p;
}

export function studySchedule(plan) {
  validatePlan(plan);
  const result = [];
  for (const cohort of plan.cohorts) {
    for (const workload of plan.workloads) {
      for (let pair = 1; pair <= cohort.pairs; pair++) {
        // Alternate which original build receives its A/A pair first. Do not
        // inspect outcomes to choose the next build, source, trial or position.
        const builds = pair % 2 ? plan.builds : [...plan.builds].reverse();
        for (const build of builds) {
          result.push({
            ordinal: result.length + 1, cohort: cohort.name, pairs: cohort.pairs,
            build: build.name, workload, pair,
            logicalOrder: pair % 2 ? ["baseline", "candidate"] : ["candidate", "baseline"],
          });
        }
      }
    }
  }
  assert.equal(result.length, plan.plannedPairCommands);
  return result;
}

export function pairEnvironment(plan, event, root, evidence, inherited = {}) {
  // A/A labels designate order ONLY. Both logical sides serve the SAME checkout
  // and immutable package; neither runtime identity nor product provenance is relabeled.
  const env = { ...inherited };
  for (const key of Object.keys(env)) {
    if (key.startsWith("NOON_PRODUCT_") || key.startsWith("NOON_PERF_")) delete env[key];
  }
  return {
    ...env, ...EXPECTED_ENV,
    NOON_PRODUCT_REFERENCE_ROOT: root, NOON_PRODUCT_CANDIDATE_ROOT: root,
    NOON_PRODUCT_REFERENCE_ARTIFACT_ROLE: "baseline",
    NOON_PRODUCT_EVIDENCE_ROOT: evidence, NOON_PRODUCT_PAIR_INDEX: String(event.pair),
    NOON_PRODUCT_EXAMPLE: event.workload, NOON_PRODUCT_PORT: String(plan.environment.port),
  };
}

// Mirror the EXACT source edit in the frozen playground-product-e2e.mjs.
// Product sample reports hash the source actually entered into the editor,
// not the untouched fixture on disk. Only square/circle has a source edit.
// Pin the full authored-source hashes from original Product Gate 37710961499
// and fail before starting any A/A sample if source or protocol has drifted.
export function authoredSourceFingerprint(workload, measurement, rawSource) {
  assert.equal(typeof rawSource, "string", "fixture must decode as UTF-8");
  assert.ok(WORKLOADS.includes(workload), "unknown product fixture");
  let authoredSource = rawSource;
  if (workload === "parity-square-and-circle") {
    const target = "self.play(Create(circle), Create(square))";
    assert.equal(authoredSource.split(target).length, 2,
      "product square/circle fixture must contain exactly one original play call");
    authoredSource = authoredSource.replace(target,
      `self.play(Create(circle), Create(square), run_time=${measurement.windowEndSeconds})\n        self.wait(${measurement.endpointHoldSeconds})`);
  }
  return sha256(authoredSource);
}

export function validateReports(plan, event, reports, measurement, sourceHash) {
  const build = plan.builds.find(b => b.name === event.build);
  assert.ok(build, "unknown pinned build");
  for (const side of ["baseline", "candidate"]) {
    const r = reports[side];
    assert.equal(r?.label, side, "missing or wrong side");
    assert.equal(r.exampleId, event.workload, "workload changed");
    assert.deepEqual(r.pair, {
      index: event.pair, position: event.logicalOrder.indexOf(side) + 1,
    }, "pair order or index changed");
    assert.deepEqual(r.measurement, measurement, "measurement protocol changed");
    assert.deepEqual(r.source, { path: measurement.sourcePath, sha256: sourceHash },
      "authored source changed");
    assert.equal(r.runtimeIdentity?.buildId, build.buildId, "wrong production build");
    assert.equal(r.runtimeIdentity?.sourceRevision, build.source, "wrong production source");
    assert.equal(r.runtime?.browserVersion, plan.environment.browserVersion, "browser changed");
    assert.equal(r.runtime?.backend, plan.environment.renderer, "renderer changed");
    assert.equal(r.runtime?.gpuMode, plan.environment.browserMode, "GPU mode changed");
    assert.equal(r.runtime?.sharedBrowserProcess, true, "pair must share the original browser process");
    assert.ok(Number.isFinite(r.fps?.effectiveFps) && r.fps.effectiveFps > 0, "invalid FPS");
  }
  assert.deepEqual(reports.baseline.runtimeIdentity, reports.candidate.runtimeIdentity,
    "A/A must not compare different packages or sources");
}

export function strictLatencyFailures(comparison) {
  assert.deepEqual(Object.keys(comparison.latency ?? {}).sort(), [...LATENCIES].sort(),
    "incomplete latency evidence");
  const failures = [];
  for (const name of LATENCIES) {
    const value = comparison.latency[name];
    assert.ok([value.baselineMs, value.candidateMs].every(n => Number.isFinite(n) && n >= 0));
    // Same strict adjunct as the frozen python-host-perf-product.mjs.
    if (value.candidateMs > value.baselineMs * 1.03 + 20) failures.push({ name, ...value });
  }
  return failures;
}

// Pure fail-closed guard: evidence comes from the *same browser server* that
// will serve the two unchanged production E2E trials for this logical pair.
export function validateAppleProof(plan, run, proof) {
  assert.equal(proof?.pairIndex, run.pair, "wrong renderer proof pair identity");
  assert.equal(proof?.exampleId, run.workload, "wrong renderer proof workload");
  assert.equal(proof?.backend, "WebGL2", "actual WebGL2 unavailable");
  assert.match(proof?.unmaskedRenderer ?? "", /Apple.*Metal Renderer.*Apple Paravirtual/i,
    "standard macOS paravirtual ANGLE Metal identity unavailable");
  assert.doesNotMatch(proof.unmaskedRenderer, /swiftshader|llvmpipe|software/i,
    "silent software fallback");
  assert.match(proof.unmaskedVendor || "", /Apple/i, "renderer vendor identity invalid");
  assert.equal(proof.platform, "darwin", "not macOS");
  assert.equal(proof.arch, "arm64", "not standard Apple Silicon macOS");
  assert.equal(proof.browserVersion, plan.environment.browserVersion, "browser version changed");
  const expectPixels = (pixels, rgb) => Array.isArray(pixels) && pixels.length === 4
    && rgb.every((value, i) => Math.abs(pixels[i] - value) <= 3);
  assert.ok(expectPixels(proof.clearPixel, [51, 102, 153, 255]),
    "real WebGL2 clear/readPixels failed");
  assert.ok(expectPixels(proof.trianglePixel, [153, 51, 102, 255]),
    "real WebGL2 shader/triangle readPixels failed");
  assert.equal(proof.shaderLinked, true, "real GLSL program was not linked");
  assert.equal(proof.glError, 0, "WebGL2 error");
  assert.equal(proof.contextLost, false, "WebGL2 context loss");
  assert.equal(proof.qualification, false);
  assert.equal(proof.performanceAcceptance, false);
  assert.equal(proof.mergeApproval, false);
  return proof;
}

export async function exclusiveDirectory(output) {
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(output); // EEXIST is an error, including after an incomplete attempt.
}

export async function runRecorded(command, args, { cwd, env, log, timeoutMs = 420_000,
  graceMs = 5_000, allowedCodes = [0] }) {
  const output = createWriteStream(log, { flags: "wx" });
  await new Promise((resolve, reject) => { output.once("open", resolve); output.once("error", reject); });
  const start = performance.now();
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env, detached: process.platform !== "win32",
      stdio: ["ignore", "pipe", "pipe"] });
    child.stdout.pipe(output, { end: false });
    child.stderr.pipe(output, { end: false });
    let timedOut = false, forcedKill, spawnError, logError;
    const kill = signal => {
      try {
        if (process.platform === "win32") child.kill(signal);
        else process.kill(-child.pid, signal);
      } catch (error) { if (error.code !== "ESRCH") spawnError ??= error; }
    };
    output.on("error", error => { logError = error; kill("SIGTERM"); });
    const timer = setTimeout(() => {
      timedOut = true; kill("SIGTERM");
      forcedKill = setTimeout(() => kill("SIGKILL"), graceMs);
    }, timeoutMs);
    child.on("error", error => { spawnError = error; });
    child.once("close", (code, signal) => {
      clearTimeout(timer); clearTimeout(forcedKill);
      output.end(() => {
        const result = { code, signal, timedOut, elapsedMs: performance.now() - start };
        if (spawnError || logError || timedOut || !allowedCodes.includes(code)) {
          const error = new Error(`diagnostic command failed: ${command}; ${JSON.stringify(result)}`);
          error.cause = spawnError ?? logError; error.result = result; reject(error);
        } else resolve(result);
      });
    });
  });
}

function git(root, ...args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024 }).trim();
}

async function snapshot() {
  const files = {};
  for (const file of ["/etc/os-release", "/proc/loadavg", "/proc/meminfo",
    "/proc/pressure/cpu", "/proc/pressure/memory", "/sys/fs/cgroup/cpu.max",
    "/sys/fs/cgroup/cpu.stat", "/sys/fs/cgroup/cpuset.cpus.effective",
    "/sys/fs/cgroup/memory.max"]) {
    files[file] = await readFile(file, "utf8").catch(error => `unavailable: ${error.code}`);
  }
  return {
    at: new Date().toISOString(), node: process.version, platform: process.platform,
    arch: process.arch, cpu: os.cpus().map(c => ({ model: c.model, speed: c.speed, times: c.times })),
    load: os.loadavg(), freeMemory: os.freemem(), totalMemory: os.totalmem(), files,
    image: { os: process.env.ImageOS, version: process.env.ImageVersion,
      runner: process.env.RUNNER_NAME, run: process.env.GITHUB_RUN_ID,
      attempt: process.env.GITHUB_RUN_ATTEMPT, event: process.env.GITHUB_EVENT_NAME },
  };
}

const save = (file, value) => writeFile(file, JSON.stringify(value, null, 2) + "\n");

export async function runStudy(planFile, harness, baseline, candidate, output) {
  const planBytes = await readFile(planFile);
  const plan = validatePlan(JSON.parse(planBytes));
  assert.equal(process.env.GITHUB_RUN_ATTEMPT, "1", "no rerun of this registered study");
  assert.equal(process.env.GITHUB_EVENT_NAME, "pull_request", "one opened diagnostic PR is required");
  const eventPayload = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  assert.equal(eventPayload.action, "opened", "synchronize/reopened/manual executions are not this study");
  assert.equal(process.version, `v${plan.environment.node}`, "Node revision changed");
  assert.equal(process.platform, "darwin", "standard hosted macOS required");
  assert.equal(process.arch, "arm64", "standard hosted Apple Silicon required");
  assert.equal(process.env.RUNNER_OS, "macOS", "GitHub macOS runner identity changed");
  assert.equal(process.env.RUNNER_ARCH, "ARM64", "GitHub arm64 runner identity changed");
  assert.equal(git(harness, "rev-parse", "HEAD"), plan.harnessSha);
  assert.equal(git(harness, "rev-parse", "HEAD^{tree}"), plan.harnessTree);
  git(harness, "diff", "--exit-code", "HEAD", "--");
  const roots = { baseline, candidate };
  for (const build of plan.builds) {
    assert.equal(git(roots[build.name], "rev-parse", "HEAD"), build.source);
  }
  await exclusiveDirectory(output);
  const schedule = studySchedule(plan);
  await save(path.join(output, "registered-plan.json"), { plan, planSha256: sha256(planBytes), schedule });
  await save(path.join(output, "environment-start.json"), await snapshot());
  const summary = {
    studyId: plan.studyId, diagnosticOnly: true, qualification: false,
    performanceAcceptance: false, mergeApproval: false,
    planSha256: sha256(planBytes), acquisitionComplete: false, status: "incomplete",
    plannedPairCommands: schedule.length, completedPairCommands: 0, cohorts: [], errors: [],
  };
  try {
    const { verifyProductArtifact } = await import(pathToFileURL(path.join(harness, ".github/ci/product-artifact.mjs")));
    const { productMeasurement } = await import(pathToFileURL(path.join(harness, "scripts/playground-product-fps.mjs")));
    const { qualifyProductMetrics } = await import(pathToFileURL(path.join(harness, plan.scorer)));
    const originalEnv = {
      ...process.env, GITHUB_SHA: plan.sourcePair.candidate,
      NOON_PRODUCT_CANDIDATE_SHA: plan.sourcePair.candidate,
      NOON_PRODUCT_BASE_SHA: plan.sourcePair.baseline, NOON_PRODUCT_HEAD_SHA: plan.sourcePair.head,
      NOON_PRODUCT_EVENT_BASE_SHA: plan.sourcePair.eventBase,
      NOON_WASM_PROFILE: "release", NOON_WASM_SKIP_OPT: "0", NOON_RENDERER_SMOKE: "0",
    };
    const identities = {};
    for (const build of plan.builds) {
      identities[build.name] = await verifyProductArtifact(roots[build.name], originalEnv, build.name);
      const runtime = JSON.parse(await readFile(path.join(roots[build.name], "web/runtime-build-identity.json")));
      assert.equal(runtime.buildId, build.buildId, "package identity changed");
    }
    await save(path.join(output, "verified-original-artifacts.json"), identities);
    // Precheck all ten package/workload source combinations before acquiring
    // the first trial. All expected hashes came from the original retained
    // qualification archive, not from selectively successful A/A results.
    const authoredHashes = {};
    for (const build of plan.builds) {
      const sourceHashes = {};
      for (const workload of plan.workloads) {
        const measurement = productMeasurement(workload);
        const sourceFile = path.join(roots[build.name], "web", measurement.sourcePath);
        const rawSource = await readFile(sourceFile, "utf8");
        const hash = authoredSourceFingerprint(workload, measurement, rawSource);
        assert.equal(hash, plan.authoredSourceHashes[workload],
          `frozen product authored-source changed: ${build.name}/${workload}`);
        sourceHashes[workload] = { sourcePath: measurement.sourcePath,
          rawSha256: sha256(rawSource), authoredSha256: hash };
      }
      authoredHashes[build.name] = sourceHashes;
    }
    await save(path.join(output, "verified-authored-sources.json"), authoredHashes);
    for (const run of schedule) {
      const root = roots[run.build];
      const directory = path.join(output, run.cohort, run.build, run.workload);
      await mkdir(directory, { recursive: true });
      const log = path.join(directory, `pair-${run.pair}.log`);
      const record = { ...run, before: await snapshot(), status: "started" };
      await appendFile(path.join(output, "acquisition.jsonl"), JSON.stringify(record) + "\n");
      try {
        record.command = await runRecorded(process.execPath, [path.join(harness, plan.pairRunner)], {
          cwd: harness, env: pairEnvironment(plan, run, root, directory, process.env), log,
          timeoutMs: plan.execution.pairDeadlineSeconds * 1000,
          graceMs: plan.execution.termGraceSeconds * 1000,
        });
        // This browser-server guard runs BEFORE both sides of each logical pair,
        // and writes one exclusive proof; never accept a silent SwiftShader fallback.
        const proofPath = path.join(directory, `macos-renderer-proof-${run.pair}.json`);
        const proof = JSON.parse(await readFile(proofPath, "utf8"));
        record.rendererProof = validateAppleProof(plan, run, proof);
        const pair = {};
        for (const side of ["baseline", "candidate"]) {
          pair[side] = JSON.parse(await readFile(path.join(directory, side, `trial-${run.pair}`, "report.json")));
        }
        const measurement = productMeasurement(run.workload);
        validateReports(plan, run, pair, measurement,
          authoredHashes[run.build][run.workload].authoredSha256);
        record.status = "recorded"; summary.completedPairCommands++;
      } catch (error) {
        record.status = "failed"; record.error = error.stack;
        throw error; // retain the failed trial and stop; do not replace or resume it
      } finally {
        record.after = await snapshot();
        await appendFile(path.join(output, "acquisition.jsonl"), JSON.stringify(record) + "\n");
      }
    }
    summary.acquisitionComplete = true;
    // Evaluate only after the entire fixed acquisition schedule. A confidence
    // classification cannot influence whether another trial is collected.
    for (const cohort of plan.cohorts) for (const build of plan.builds) for (const workload of plan.workloads) {
      const directory = path.join(output, cohort.name, build.name, workload);
      const result = { cohort: cohort.name, build: build.name, workload, pairs: cohort.pairs };
      try {
        result.comparator = await runRecorded(process.execPath, [
          path.join(harness, plan.comparator), path.join(directory, "baseline"),
          path.join(directory, "candidate"), "--pairs", String(cohort.pairs),
        ], { cwd: harness, env: { ...process.env, ...EXPECTED_ENV },
          log: path.join(directory, "comparison.log"), timeoutMs: 60_000, allowedCodes: [0, 2] });
        const comparison = JSON.parse(await readFile(path.join(directory, "candidate/comparison.json")));
        assert.equal(comparison.exampleId, workload);
        result.metrics = qualifyProductMetrics(comparison, cohort.pairs);
        result.strictLatencyFailures = strictLatencyFailures(comparison);
        result.behaviorFailures = comparison.failures;
        result.diagnosticClassification = result.comparator.code === 0
          && result.metrics.status === "pass" && result.strictLatencyFailures.length === 0
          ? "within-frozen-budgets" : "blocked-on-identical-build";
      } catch (error) {
        result.diagnosticClassification = "invalid-evidence";
        result.error = error.stack; summary.errors.push({ cohort: cohort.name, build: build.name, workload, error: error.stack });
      }
      summary.cohorts.push(result);
    }
    for (const build of plan.builds) {
      assert.deepEqual(await verifyProductArtifact(roots[build.name], originalEnv, build.name), identities[build.name],
        "original package/source changed during study");
    }
    git(harness, "diff", "--exit-code", "HEAD", "--");
    assert.equal(summary.cohorts.length, 20);
    summary.status = summary.errors.length === 0 ? "diagnostic-complete" : "invalid-evidence";
  } catch (error) {
    summary.errors.push({ error: error.stack });
    throw error;
  } finally {
    await save(path.join(output, "summary.json"), summary);
    await save(path.join(output, "environment-end.json"), await snapshot());
    if (process.env.GITHUB_STEP_SUMMARY) {
      const counts = {};
      for (const result of summary.cohorts) counts[result.diagnosticClassification] = (counts[result.diagnosticClassification] ?? 0) + 1;
      await appendFile(process.env.GITHUB_STEP_SUMMARY,
        `## #1933 A/A diagnostic — NOT PR qualification\n\nStudy: ${plan.studyId}\n\n` +
        `Completed pair commands: ${summary.completedPairCommands}/${schedule.length}\n\n` +
        `Classification counts: ${JSON.stringify(counts)}\n\n` +
        `Acquisition status: ${summary.status}. Qualification: false. Merge approval: false.\n\n` +
        "All original #1875 failures remain blocking. This study is not a candidate-vs-baseline performance verdict.\n");
    }
  }
  assert.equal(summary.errors.length, 0, "incomplete or invalid diagnostic evidence");
  return summary;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length !== 5) {
    console.error("usage: product-aa-apple-v07-study.mjs <plan.json> <frozen-harness> <baseline-source> <candidate-source> <new-output-dir>");
    process.exitCode = 2;
  } else {
    runStudy(...args.map(arg => path.resolve(arg))).catch(error => { console.error(error); process.exitCode = 1; });
  }
}

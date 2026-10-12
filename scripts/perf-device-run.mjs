import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseBuildProcesses } from "./perf-physical-readiness.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const deviceId = process.env.NOON_PERF_DEVICE_ID?.trim();
assert.ok(deviceId, "NOON_PERF_DEVICE_ID is required for a physical-machine baseline");
assert.match(deviceId, /^[a-zA-Z0-9._-]+$/, "device ID must be filesystem-safe");
const gpuMode = process.env.NOON_PERF_GPU_MODE ?? "hardware";
assert.ok(["hardware", "software"].includes(gpuMode), "NOON_PERF_GPU_MODE must be hardware or software");

const backends = list(process.env.NOON_PERF_BACKENDS ?? "webgpu,webgl");
for (const backend of backends) {
  assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
}
const suites = list(process.env.NOON_PERF_SUITES ?? "frame,corpus");
for (const suite of suites) {
  assert.ok(
    suite === "frame" || suite === "authoring" || suite === "corpus" || suite === "dynamic-stress",
    `unknown suite: ${suite}`,
  );
}

const stamp = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const relativeDir = process.env.NOON_PERF_RUN_DIR ?? `perf-artifacts/${deviceId}/${stamp}`;
const runDir = path.resolve(repoRoot, relativeDir);
await mkdir(runDir, { recursive: true });

const artifacts = [];
const buildActivityChecks = [];
for (const backend of backends) {
  if (suites.includes("frame")) {
    const relative = path.join(relativeDir, `frame-${backend}.json`);
    await run("scripts/perf-profile.mjs", {
      NOON_PERF_BACKEND: backend,
      NOON_PERF_ARTIFACT: relative,
    }, backend);
    artifacts.push({ suite: "frame", backend, path: relative });
  }
  if (suites.includes("corpus")) {
    const relative = path.join(relativeDir, `corpus-${backend}.json`);
    await run("scripts/perf-corpus.mjs", {
      NOON_CORPUS_BACKEND: backend,
      NOON_CORPUS_GPU_MODE: gpuMode,
      NOON_CORPUS_ARTIFACT: relative,
    }, backend);
    artifacts.push({ suite: "corpus", backend, path: relative });
  }
  if (suites.includes("authoring")) {
    const relative = path.join(relativeDir, `authoring-${backend}.json`);
    await run("scripts/authoring-perf.mjs", {
      NOON_AUTHORING_PERF_BACKEND: backend,
      NOON_AUTHORING_PERF_ARTIFACT: relative,
    }, backend);
    artifacts.push({ suite: "authoring", backend, path: relative });
  }
  if (suites.includes("dynamic-stress")) {
    const relative = path.join(relativeDir, `dynamic-stress-${backend}.json`);
    const webgpuMinimumFps = process.env.NOON_PERF_DYNAMIC_STRESS_WEBGPU_MIN_FPS?.trim();
    await run("scripts/retained-dynamic-stress-perf.mjs", {
      NOON_RETAINED_STRESS_BACKEND: backend,
      NOON_RETAINED_STRESS_GPU_MODE: gpuMode,
      NOON_RETAINED_STRESS_ARTIFACT: relative,
      NOON_RETAINED_STRESS_SAMPLE_HZ: process.env.NOON_PERF_TARGET_HZ ?? "60",
      ...(backend === "webgpu" && webgpuMinimumFps
        ? { NOON_RETAINED_STRESS_MIN_EFFECTIVE_FPS: webgpuMinimumFps }
        : {}),
    }, backend);
    artifacts.push({ suite: "dynamic-stress", backend, path: relative });
  }
}

const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
const bundle = {
  schemaVersion: 2,
  benchmark: "Noon named physical-device performance bundle",
  generatedAt: new Date().toISOString(),
  deviceId,
  operatorNotes: process.env.NOON_PERF_DEVICE_NOTES ?? null,
  commit: commit.status === 0 ? commit.stdout.trim() : null,
  configuration: {
    backends,
    suites,
    gpuMode,
    width: process.env.NOON_PERF_WIDTH ?? "960",
    height: process.env.NOON_PERF_HEIGHT ?? "540",
    devicePixelRatio: process.env.NOON_PERF_DPR ?? "1",
    targetHz: process.env.NOON_PERF_TARGET_HZ ?? "60",
    dynamicStressWebgpuMinimumFps: process.env.NOON_PERF_DYNAMIC_STRESS_WEBGPU_MIN_FPS ?? null,
  },
  artifacts,
  buildActivityChecks,
};
await writeFile(path.join(runDir, "manifest.json"), `${JSON.stringify(bundle, null, 2)}\n`);
console.log(`Named-device bundle: ${path.relative(repoRoot, runDir)}`);

async function checkBuildActivity(script, backend, phase) {
  if (gpuMode !== "hardware") return;
  const result = spawnSync("ps", ["-axo", "pid=,comm="], {
    encoding: "utf8", timeout: 6000, maxBuffer: 256 * 1024,
  });
  const processes = result.status === 0 ? parseBuildProcesses(result.stdout) : null;
  const file = `${path.basename(script, ".mjs")}-${backend}-builds-${phase}.json`;
  await writeFile(path.join(runDir, file), `${JSON.stringify({
    generatedAt: new Date().toISOString(), phase, processes,
    probeExitCode: result.status, probeError: result.error?.code ?? null,
    scope: "Known build tools only; absence is not quiet-host or performance qualification",
  }, null, 2)}\n`, { flag: "wx" });
  buildActivityChecks.push(file);
  assert.ok(processes !== null, `host build activity unknown; see ${file}`);
  assert.equal(processes.length, 0, `compiler/build/archive tooling is active; see ${file}`);
}

async function run(script, extraEnv, backend) {
  await checkBuildActivity(script, backend, "before");
  const result = spawnSync(process.execPath, [script], {
    cwd: repoRoot,
    env: { ...process.env, ...extraEnv },
    stdio: "inherit",
  });
  // Keep raw suite reports even if competing work began during capture. This
  // observation happens after measurement, never in the scored frame loop.
  await checkBuildActivity(script, backend, "after");
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function list(value) {
  return String(value)
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const deviceId = process.env.NOON_PERF_DEVICE_ID?.trim();
assert.ok(deviceId, "NOON_PERF_DEVICE_ID is required for a physical-machine baseline");
assert.match(deviceId, /^[a-zA-Z0-9._-]+$/, "device ID must be filesystem-safe");

const backends = list(process.env.NOON_PERF_BACKENDS ?? "webgpu,webgl");
for (const backend of backends) {
  assert.ok(backend === "webgpu" || backend === "webgl", `unknown backend: ${backend}`);
}
const suites = list(process.env.NOON_PERF_SUITES ?? "frame,corpus");
for (const suite of suites) {
  assert.ok(
    suite === "frame" || suite === "authoring" || suite === "corpus",
    `unknown suite: ${suite}`,
  );
}

const stamp = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const relativeDir = process.env.NOON_PERF_RUN_DIR ?? `perf-artifacts/${deviceId}/${stamp}`;
const runDir = path.resolve(repoRoot, relativeDir);
await mkdir(runDir, { recursive: true });

const artifacts = [];
for (const backend of backends) {
  if (suites.includes("frame")) {
    const relative = path.join(relativeDir, `frame-${backend}.json`);
    run("scripts/perf-profile.mjs", {
      NOON_PERF_BACKEND: backend,
      NOON_PERF_ARTIFACT: relative,
    });
    artifacts.push({ suite: "frame", backend, path: relative });
  }
  if (suites.includes("corpus")) {
    const relative = path.join(relativeDir, `corpus-${backend}.json`);
    run("scripts/perf-corpus.mjs", {
      NOON_CORPUS_BACKEND: backend,
      NOON_CORPUS_ARTIFACT: relative,
    });
    artifacts.push({ suite: "corpus", backend, path: relative });
  }
  if (suites.includes("authoring")) {
    const relative = path.join(relativeDir, `authoring-${backend}.json`);
    run("scripts/authoring-perf.mjs", {
      NOON_AUTHORING_PERF_BACKEND: backend,
      NOON_AUTHORING_PERF_ARTIFACT: relative,
    });
    artifacts.push({ suite: "authoring", backend, path: relative });
  }
}

const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
const bundle = {
  schemaVersion: 1,
  benchmark: "Noon named physical-device performance bundle",
  generatedAt: new Date().toISOString(),
  deviceId,
  operatorNotes: process.env.NOON_PERF_DEVICE_NOTES ?? null,
  commit: commit.status === 0 ? commit.stdout.trim() : null,
  configuration: {
    backends,
    suites,
    width: process.env.NOON_PERF_WIDTH ?? "960",
    height: process.env.NOON_PERF_HEIGHT ?? "540",
    targetHz: process.env.NOON_PERF_TARGET_HZ ?? "60",
  },
  artifacts,
};
await writeFile(path.join(runDir, "manifest.json"), `${JSON.stringify(bundle, null, 2)}\n`);
console.log(`Named-device bundle: ${path.relative(repoRoot, runDir)}`);

function run(script, extraEnv) {
  const result = spawnSync(process.execPath, [script], {
    cwd: repoRoot,
    env: { ...process.env, ...extraEnv },
    stdio: "inherit",
  });
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

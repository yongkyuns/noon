import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { optimizedConfig, assertReplaySurface, validateBuildEnvironment } from "../.github/ci/optimized-replay-artifact.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const source = name => readFile(path.join(root, name), "utf8");
const js = "export function verifyDirectExecutionReplay(example, targets, samples, count) {}";
const dts = "export function verifyDirectExecutionReplay(example: string, targets: Float64Array, samples: number, count: number): void;";

test("production and replay retain release optimization with distinct feature identities", () => {
  const production = optimizedConfig("production");
  const replay = optimizedConfig("replay");
  assert.equal(production.profile, "release");
  assert.equal(production.skipOpt, "0");
  assert.equal(production.features, "default");
  assert.equal(replay.features, "default,replay-smoke");
  assert.deepEqual({ ...replay, features: production.features }, production);
  assert.throws(() => optimizedConfig("dev"));
});

test("generated JavaScript and declarations must agree with the artifact role", () => {
  assertReplaySurface("", "", "production");
  assertReplaySurface(js, dts, "replay");
  for (const [javascript, declarations] of [[js, ""], ["", dts]]) {
    assert.throws(() => assertReplaySurface(javascript, declarations, "production"));
    assert.throws(() => assertReplaySurface(javascript, declarations, "replay"));
  }
  assert.throws(() => assertReplaySurface(js, dts, "production"));
  assert.throws(() => assertReplaySurface("", "", "replay"));
});

test("optimized identity rejects debug, no-opt and unrelated feature overrides", () => {
  validateBuildEnvironment({}, "production");
  validateBuildEnvironment({ NOON_REPLAY_SMOKE: "1" }, "replay");
  for (const env of [
    { CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS: "true" },
    { CARGO_PROFILE_RELEASE_OPT_LEVEL: "0" },
    { RUSTFLAGS: "-C debug-assertions=yes" },
    { NOON_WASM_PROFILE: "dev" }, { NOON_WASM_SKIP_OPT: "1" },
    { NOON_RENDERER_SMOKE: "1" }, { NOON_REPLAY_SMOKE: "1" },
  ]) assert.throws(() => validateBuildEnvironment(env, "production"));
  assert.throws(() => validateBuildEnvironment({}, "replay"));
});

test("release fixture export is opt-in without enabling renderer diagnostics", async () => {
  const cargo = await source("crates/noon-web/Cargo.toml");
  assert.match(cargo, /^default = \["renderer"\]$/m);
  assert.match(cargo, /^replay-smoke = \[\]$/m);
  assert.match(cargo, /^renderer-smoke = \["renderer"\]$/m);
  const rust = await source("crates/noon-web/src/determinism.rs");
  assert.match(rust, /#\[cfg\(all\(\s*target_arch = "wasm32",\s*any\(debug_assertions, feature = "replay-smoke"\)\s*\)\)\]\s*mod wasm/);
});

// Exercise the real build-script argument selection without compiling a fake engine.
// Stub only the external tool invocations; runtime evidence is the browser CI job.
async function buildArguments(env) {
  const temp = await mkdtemp(path.join(tmpdir(), "noon-replay-contract-"));
  try {
    await mkdir(path.join(temp, "scripts"));
    await mkdir(path.join(temp, "bin"));
    await writeFile(path.join(temp, "scripts/build-web-demo.sh"), await source("scripts/build-web-demo.sh"));
    await writeFile(path.join(temp, "bin/node"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
    await writeFile(path.join(temp, "bin/wasm-pack"), '#!/bin/sh\nprintf "%s\\n" "$@" > "$NOON_TEST_ARGS"\n', { mode: 0o755 });
    const argsFile = path.join(temp, "args.txt");
    const result = spawnSync("bash", ["scripts/build-web-demo.sh"], { cwd: temp, encoding: "utf8",
      env: { ...process.env, PATH: `${temp}/bin:${process.env.PATH}`, NOON_TEST_ARGS: argsFile,
        NOON_SKIP_WEB_PREFLIGHT: "1", NOON_WEB_PREFLIGHT_ONLY: "0", NOON_WASM_PROFILE: "release",
        NOON_WASM_SKIP_OPT: "0", NOON_RENDERER_SMOKE: "0", NOON_REPLAY_SMOKE: "0", ...env } });
    assert.equal(result.status, 0, result.stderr);
    return (await readFile(argsFile, "utf8")).trim().split("\n");
  } finally { await rm(temp, { recursive: true, force: true }); }
}

test("normal release build keeps fixture features absent", async () => {
  const args = await buildArguments({});
  assert.ok(args.includes("--release"));
  assert.ok(!args.includes("--features"));
  assert.ok(!args.includes("--no-opt"));
});

test("replay build explicitly enables only replay-smoke and keeps optimization", async () => {
  const args = await buildArguments({ NOON_REPLAY_SMOKE: "1" });
  assert.ok(args.includes("--release"));
  assert.equal(args[args.indexOf("--features") + 1], "replay-smoke");
  assert.ok(!args.includes("--no-opt"));
});

test("renderer diagnostic feature does not silently opt into replay", async () => {
  const args = await buildArguments({ NOON_RENDERER_SMOKE: "1" });
  assert.equal(args[args.indexOf("--features") + 1], "renderer-smoke");
  assert.ok(!args.includes("replay-smoke"));
});

test("all six replay fixtures and original sampling remain mandatory", async () => {
  const smoke = await source("scripts/deterministic-replay-smoke.mjs");
  for (const fixture of ["exact-property-tracks", "specialized-geometry", "family-placement",
    "painter-order", "analytic-stress", "create-morph-fade"]) assert.ok(smoke.includes(`"${fixture}"`));
  assert.match(smoke, /const forwardSampleCount = 32;/);
  assert.match(smoke, /const targets = \[0, 0\.25, 0\.5, 0\.999, 1, 1\.001, 1\.5, 2, 2\.5, 3, 3\.75\];/);
  assert.match(smoke, /typeof wasm\.verifyDirectExecutionReplay !== "function"/);
  assert.match(smoke, /window\.noonDeterminism\.verify\(example, new Float64Array\(targetTimes\), sampleCount, count\)/);
});

test("workflow retains production before replay and retries browsers without compilation", async () => {
  const workflow = await source(".github/workflows/platform-release.yml");
  assert.match(workflow, /pull_request:\s*paths:/);
  for (const file of [".github/ci/optimized-replay-artifact.mjs", "web/optimized-replay-contract.test.mjs"])
    assert.ok(workflow.includes(file));
  const productionUpload = workflow.indexOf("name: optimized-web-pkg");
  const replayBuild = workflow.indexOf("name: Build optimized replay qualification package");
  assert.ok(productionUpload > 0 && replayBuild > productionUpload);
  assert.ok(workflow.indexOf("name: optimized-replay-pkg") > replayBuild);
  const browser = workflow.split("\n  optimized-replay:\n")[1];
  assert.ok(browser, "separate browser-only job is required");
  assert.match(browser, /needs: optimized-wasm/);
  assert.match(browser, /actions\/download-artifact@/);
  assert.match(browser, /optimized-replay-artifact\.mjs verify replay \./);
  assert.match(browser, /node scripts\/deterministic-replay-smoke\.mjs/);
  assert.doesNotMatch(browser, /cargo |rustup |wasm-pack |build-web-demo\.sh/);
  assert.doesNotMatch(workflow, /continue-on-error: true|CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS|NOON_WASM_PROFILE:.*dev|NOON_WASM_SKIP_OPT:.*1/);
});

// Optimized production and replay qualification are distinct same-run artifacts.
// Reuse the existing source/configuration/lock/package provenance contract.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { config, prepareArtifact, stamp, verify } from "./wasm-build.mjs";

export function optimizedConfig(role) {
  assert.ok(["production", "replay"].includes(role), "invalid optimized artifact role");
  return { ...config, profile: "release", skipOpt: "0", sccache: "none",
    features: role === "replay" ? "default,replay-smoke" : "default", binaryen: "132" };
}

export function assertReplaySurface(javascript, declarations, role) {
  optimizedConfig(role);
  const exported = /\bexport\s+function\s+verifyDirectExecutionReplay\s*\(/;
  for (const [name, source] of [["JavaScript", javascript], ["declarations", declarations]]) {
    assert.equal(exported.test(source), role === "replay",
      `${role} ${name}: replay fixture export must be ${role === "replay" ? "present" : "absent"}`);
  }
}

export function validateBuildEnvironment(env, role) {
  optimizedConfig(role);
  for (const [name, value] of Object.entries(env)) {
    if (!value) continue;
    assert.ok(!/^(RUSTFLAGS|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTUP_TOOLCHAIN|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|CARGO_BUILD_TARGET|CARGO_TARGET_.*_(RUSTFLAGS|LINKER)|CARGO_PROFILE_RELEASE_.*)$/.test(name),
      `unsupported optimized-build override: ${name}`);
  }
  for (const [name, expected] of Object.entries({ CARGO_INCREMENTAL: "0",
    NOON_WASM_PROFILE: "release", NOON_WASM_SKIP_OPT: "0", NOON_RENDERER_SMOKE: "0",
    NOON_REPLAY_SMOKE: role === "replay" ? "1" : "0" })) {
    if (env[name] !== undefined) assert.equal(env[name], expected, `unexpected ${name}`);
  }
  // Replay must be explicitly opted in; absence cannot be mislabeled as replay.
  if (role === "replay") assert.equal(env.NOON_REPLAY_SMOKE, "1");
}

async function checkSurface(root, role) {
  const [javascript, declarations] = await Promise.all([
    readFile(path.join(root, "web/pkg/noon_web.js"), "utf8"),
    readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8"),
  ]);
  assertReplaySurface(javascript, declarations, role);
}

function commandOutput(root, command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, `${command} failed: ${result.stderr || result.error}`);
  return result.stdout.trim();
}

async function main() {
  const [command, role, checkout] = process.argv.slice(2);
  const build = optimizedConfig(role);
  assert.ok(checkout, "missing optimized checkout");
  const root = path.resolve(checkout);
  const identityPath = path.join(root, `ci-artifacts/optimized-${role}.json`);
  if (command === "prepare") {
    validateBuildEnvironment(process.env, role);
    assert.equal(commandOutput(root, "wasm-pack", ["--version"]), `wasm-pack ${build.wasmPack}`);
    assert.match(commandOutput(root, "wasm-opt", ["--version"]),
      new RegExp(`^wasm-opt version ${build.binaryen}(?:\\s|$)`));
    const compiler = commandOutput(root, "rustc", ["-vV"]);
    const identity = await prepareArtifact(root, process.env, compiler, build);
    await mkdir(path.dirname(identityPath), { recursive: true });
    await writeFile(identityPath, JSON.stringify(identity));
  } else if (command === "stamp") {
    validateBuildEnvironment(process.env, role);
    await checkSurface(root, role);
    await stamp(root, JSON.parse(await readFile(identityPath, "utf8")), process.env, build);
  } else if (command === "verify") {
    const manifest = await verify(root, process.env, build);
    await checkSurface(root, role);
    console.log(`Verified optimized ${role} artifact for ${manifest.source}; features=${manifest.build.features}.`);
  } else throw new Error("expected prepare, stamp, or verify");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}

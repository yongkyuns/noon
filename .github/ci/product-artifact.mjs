// Same-run release artifacts for the product gate. Reuse the dev artifact contract.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { config, prepareArtifact, stamp, verify } from "./wasm-build.mjs";

export function productConfig(role) {
  assert.ok(["baseline", "candidate", "candidate-fixture"].includes(role), "invalid product artifact role");
  const rendererSmoke = role === "candidate-fixture" ? "1" : "0";
  return {
    ...config,
    profile: "release",
    features: rendererSmoke === "1" ? "default,renderer-smoke" : "default",
    skipOpt: "0",
    rendererSmoke,
    binaryen: "132",
  };
}

export function validateProductEnvironment(env, role) {
  const build = productConfig(role);
  for (const [name, expected] of Object.entries({
    NOON_WASM_PROFILE: build.profile,
    NOON_WASM_SKIP_OPT: build.skipOpt,
    NOON_RENDERER_SMOKE: build.rendererSmoke,
  })) {
    assert.equal(env[name], expected, `unexpected ${name} for ${role} product artifact`);
  }
}

function commandOutput(root, command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, `${command} failed: ${result.stderr || result.error}`);
  return result.stdout.trim();
}

async function main() {
  const [command, role, checkout] = process.argv.slice(2);
  const build = productConfig(role);
  assert.ok(checkout, "missing product checkout");
  const root = path.resolve(checkout);
  // Both checkouts use immutable event SHAs. The candidate is the tested merge,
  // never a substituted PR head; the baseline is the event's pinned base commit.
  const sha = role === "baseline" ? process.env.NOON_PRODUCT_BASE_SHA : process.env.GITHUB_SHA;
  assert.match(sha ?? "", /^[0-9a-f]{40}$/, "missing product source SHA");
  const env = { ...process.env, GITHUB_SHA: sha };
  const identityPath = path.join(root, "ci-artifacts/product-build.json");
  if (command === "prepare") {
    validateProductEnvironment(env, role);
    assert.equal(commandOutput(root, "wasm-pack", ["--version"]), `wasm-pack ${build.wasmPack}`);
    assert.match(commandOutput(root, "wasm-opt", ["--version"]),
      new RegExp(`^wasm-opt version ${build.binaryen}(?:\\s|$)`));
    const compiler = spawnSync("rustc", ["-vV"], { cwd: root, encoding: "utf8" });
    assert.equal(compiler.status, 0, "rustc -vV failed");
    const identity = await prepareArtifact(root, env, compiler.stdout.trim(), build);
    await mkdir(path.dirname(identityPath), { recursive: true });
    await writeFile(identityPath, JSON.stringify(identity));
  } else if (command === "stamp") {
    validateProductEnvironment(env, role);
    await stamp(root, JSON.parse(await readFile(identityPath, "utf8")), env, build);
  } else if (command === "verify") {
    validateProductEnvironment(env, role);
    const manifest = await verify(root, env, build);
    console.log(`Verified ${role} release artifact for ${manifest.source}: ${Object.keys(manifest.files).length} files.`);
  } else throw new Error("expected prepare, stamp, or verify");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}

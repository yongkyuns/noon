// Same-run release artifacts for the product gate. Reuse the dev artifact contract.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { config, prepareArtifact, sourceSha, stamp, verify } from "./wasm-build.mjs";

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

function mergeParents(root) {
  const parents = commandOutput(root, "git", ["show", "-s", "--format=%P", "HEAD"]).split(" ");
  assert.ok(parents.length === 2 && parents.every(sha => /^[0-9a-f]{40}$/.test(sha)),
    "product candidate must be a two-parent merge with fetched parents");
  return parents;
}

// Select the actual tested merge's first parent, even if the event base is older.
// Pin these commits in the producer so a comparison retry cannot change sources.
export function resolveProductSources(root, env) {
  assert.match(env.GITHUB_SHA ?? "", /^[0-9a-f]{40}$/, "missing tested product merge SHA");
  const candidate = sourceSha(root, { GITHUB_SHA: env.GITHUB_SHA });
  const [baseline, head] = mergeParents(root);
  assert.equal(head, env.NOON_PRODUCT_HEAD_SHA, "product merge does not contain the requested PR head");
  assert.match(env.NOON_PRODUCT_EVENT_BASE_SHA ?? "", /^[0-9a-f]{40}$/, "missing event base SHA");
  return { schema: 1, candidate, baseline, head, eventBase: env.NOON_PRODUCT_EVENT_BASE_SHA };
}

function productContext(root, env, role) {
  const build = productConfig(role);
  validateProductEnvironment(env, role);
  const sources = { schema: 1, candidate: env.NOON_PRODUCT_CANDIDATE_SHA,
    baseline: env.NOON_PRODUCT_BASE_SHA, head: env.NOON_PRODUCT_HEAD_SHA,
    eventBase: env.NOON_PRODUCT_EVENT_BASE_SHA };
  for (const [name, sha] of Object.entries(sources)) {
    if (name !== "schema") assert.match(sha ?? "", /^[0-9a-f]{40}$/, `missing product ${name} SHA`);
  }
  const source = role === "baseline" ? sources.baseline : sources.candidate;
  sourceSha(root, { GITHUB_SHA: source });
  if (role !== "baseline") {
    assert.deepEqual(mergeParents(root), [sources.baseline, sources.head],
      "product baseline/head do not match the tested merge parents");
  }
  return { build, sources, env: { ...env, GITHUB_SHA: source } };
}

export async function prepareProductArtifact(root, env, compiler, role) {
  const context = productContext(root, env, role);
  return { ...await prepareArtifact(root, context.env, compiler, context.build),
    productSources: context.sources };
}

export async function stampProductArtifact(root, prepared, env, role) {
  const context = productContext(root, env, role);
  assert.deepEqual(prepared.productSources, context.sources, "product source pair changed during build");
  const manifest = { ...await stamp(root, prepared, context.env, context.build),
    productSources: context.sources };
  await writeFile(path.join(root, "web/ci-artifact.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

export async function verifyProductArtifact(root, env, role) {
  const context = productContext(root, env, role);
  const manifest = await verify(root, context.env, context.build);
  assert.deepEqual(manifest.productSources, context.sources, "artifact product source pair mismatch");
  return manifest;
}

async function main() {
  if (process.argv[2] === "sources") {
    assert.ok(process.argv[3], "missing product candidate checkout");
    const sources = resolveProductSources(path.resolve(process.argv[3]), process.env);
    const values = { NOON_PRODUCT_CANDIDATE_SHA: sources.candidate, NOON_PRODUCT_BASE_SHA: sources.baseline,
      NOON_PRODUCT_HEAD_SHA: sources.head, NOON_PRODUCT_EVENT_BASE_SHA: sources.eventBase };
    await appendFile(process.env.GITHUB_ENV, Object.entries(values).map(([key, sha]) => `${key}=${sha}\n`).join(""));
    await appendFile(process.env.GITHUB_OUTPUT, Object.entries(sources).filter(([key]) => key !== "schema")
      .map(([key, sha]) => `${key === "eventBase" ? "event-base" : key}-sha=${sha}\n`).join(""));
    console.log(`Product sources: ${JSON.stringify(sources)}`);
    return;
  }
  const [command, role, checkout] = process.argv.slice(2);
  assert.ok(checkout, "missing product checkout");
  const root = path.resolve(checkout);
  const env = process.env;
  const identityPath = path.join(root, "ci-artifacts/product-build.json");
  if (command === "prepare") {
    const build = productConfig(role);
    validateProductEnvironment(env, role);
    assert.equal(commandOutput(root, "wasm-pack", ["--version"]), `wasm-pack ${build.wasmPack}`);
    assert.match(commandOutput(root, "wasm-opt", ["--version"]),
      new RegExp(`^wasm-opt version ${build.binaryen}(?:\\s|$)`));
    const compiler = spawnSync("rustc", ["-vV"], { cwd: root, encoding: "utf8" });
    assert.equal(compiler.status, 0, "rustc -vV failed");
    const identity = await prepareProductArtifact(root, env, compiler.stdout.trim(), role);
    await mkdir(path.dirname(identityPath), { recursive: true });
    await writeFile(identityPath, JSON.stringify(identity));
  } else if (command === "stamp") {
    await stampProductArtifact(root, JSON.parse(await readFile(identityPath, "utf8")), env, role);
  } else if (command === "verify") {
    const manifest = await verifyProductArtifact(root, env, role);
    console.log(`Verified ${role} release artifact for ${manifest.source}: ${Object.keys(manifest.files).length} files.`);
  } else throw new Error("expected prepare, stamp, or verify");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}

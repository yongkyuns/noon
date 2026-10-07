// Same-run release artifacts for the product gate. Reuse the dev artifact contract.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { appendFile, copyFile, lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { config, configuration, fileDigest, prepareArtifact, sourceSha, stamp, verify } from "./wasm-build.mjs";

const lockPath = "Cargo.lock";
const dependencyInputs = name => name === "Cargo.toml" || name === "rust-toolchain.toml"
  || name.startsWith(".cargo/") || /^crates\/.+\/Cargo\.toml$/.test(name);

function trackedLockfile(root) {
  return commandOutput(root, "git", ["ls-files", "--", lockPath]) !== "";
}

async function assertSafeLockfile(root, { required = false } = {}) {
  try {
    const info = await lstat(path.join(root, lockPath));
    assert.ok(!info.isSymbolicLink(), `refusing symlink ${lockPath} in ${root}`);
    assert.ok(info.isFile(), `${lockPath} is not a regular file in ${root}`);
    return true;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    assert.ok(!required, `missing ${lockPath} in ${root}`);
    return false;
  }
}

async function resolveCargoLockfile(root, runCargo) {
  await runCargo(root, ["generate-lockfile"]);
  await assertSafeLockfile(root, { required: true });
}

async function validateCargoLockfile(root, runCargo) {
  const stdout = await runCargo(root, ["metadata", "--locked", "--format-version", "1"]);
  const metadata = JSON.parse(stdout);
  assert.ok(Array.isArray(metadata.packages), `cargo metadata returned no package list for ${root}`);
  assert.ok(metadata.resolve && Array.isArray(metadata.resolve.nodes),
    `cargo metadata returned no dependency resolution for ${root}`);
}

async function dependencyConfiguration(root) {
  const build = await configuration(root);
  const inputs = Object.fromEntries(Object.entries(build.inputs).filter(([name]) => dependencyInputs(name)));
  assert.ok(inputs["Cargo.toml"] && inputs["rust-toolchain.toml"],
    `missing tracked dependency inputs in ${root}`);
  return inputs;
}

// Resolve the baseline once, then reuse its ignored Cargo.lock only where the
// tracked dependency inputs match. A differing input set gets an explicit,
// independent resolution; every resulting lock is checked with --locked.
export async function prepareProductDependencies(baselineRoot, targetRoots, options = {}) {
  assert.ok(Array.isArray(targetRoots) && targetRoots.length > 0, "missing product dependency targets");
  const runCargo = options.runCargo ?? ((root, args) => commandOutput(root, "cargo", args));
  const baselineTracked = await trackedLockfile(baselineRoot);
  await assertSafeLockfile(baselineRoot, { required: baselineTracked });
  const baselineInputs = await dependencyConfiguration(baselineRoot);

  if (!baselineTracked) {
    console.log(`Resolving baseline dependency lock once: ${baselineRoot}`);
    await resolveCargoLockfile(baselineRoot, runCargo);
  } else {
    console.log(`Preserving tracked baseline ${lockPath}: ${baselineRoot}`);
  }
  await assertSafeLockfile(baselineRoot, { required: true });
  await validateCargoLockfile(baselineRoot, runCargo);
  const baselineDigest = await fileDigest(baselineRoot, lockPath);
  const results = [{ root: baselineRoot, mode: baselineTracked ? "tracked-preserved" : "resolved", lockDigest: baselineDigest }];

  for (const targetRoot of targetRoots) {
    const targetTracked = await trackedLockfile(targetRoot);
    await assertSafeLockfile(targetRoot, { required: targetTracked });
    const targetInputs = await dependencyConfiguration(targetRoot);
    const inputsMatch = JSON.stringify(targetInputs) === JSON.stringify(baselineInputs);
    let mode;

    if (targetTracked) {
      mode = "tracked-preserved";
      if (!inputsMatch) {
        console.log(`Dependency inputs differ; preserving tracked ${lockPath} and validating it without rewriting: ${targetRoot}`);
      }
    } else if (inputsMatch) {
      console.log(`Reusing baseline dependency resolution for matching inputs: ${targetRoot}`);
      await copyFile(path.join(baselineRoot, lockPath), path.join(targetRoot, lockPath));
      mode = "reused-baseline";
    } else {
      console.log(`Dependency inputs differ; independently resolving ${lockPath}: ${targetRoot}`);
      await resolveCargoLockfile(targetRoot, runCargo);
      mode = "independently-resolved";
    }

    await assertSafeLockfile(targetRoot, { required: true });
    await validateCargoLockfile(targetRoot, runCargo);
    results.push({ root: targetRoot, mode, inputsMatchBaseline: inputsMatch,
      lockDigest: await fileDigest(targetRoot, lockPath) });
  }

  return { baseline: baselineRoot, baselineTracked, baselineInputs, results };
}

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
  // Full workspace metadata exceeds Node's default 1 MiB capture buffer.
  const result = spawnSync(command, args, {
    cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
  });
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
  if (process.argv[2] === "dependencies") {
    const [baseline, ...targets] = process.argv.slice(3);
    assert.ok(baseline && targets.length > 0, "expected dependencies <baseline> <target> [target ...]");
    const roots = [baseline, ...targets].map(root => path.resolve(root));
    const prepared = await prepareProductDependencies(roots[0], roots.slice(1));
    console.log(`Product dependency locks: ${JSON.stringify(prepared.results)}`);
    return;
  }
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

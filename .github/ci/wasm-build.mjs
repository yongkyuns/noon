// CI-only cache identity, measurement, and same-run artifact qualification (#1265).
// Compiler caches are hints. No cache hit may skip the source build below.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { appendFile, copyFile, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { constants } from "node:os";
import { fileURLToPath } from "node:url";

export const config = Object.freeze({
  schema: 1,
  target: "wasm32-unknown-unknown",
  profile: "dev",
  features: "default",
  incremental: "0",
  debug: "0",
  skipOpt: "1",
  wasmPack: "0.15.0",
  sccache: "0.16.0",
});
const evidence = "ci-artifacts/wasm-build";
const manifestPath = "web/ci-artifact.json";
const lockPath = "web/ci-Cargo.lock";
const bundlePattern = /^compat-bundle\.[0-9a-f]{64}\.json$/;
const digest = (value) => createHash("sha256").update(value).digest("hex");
const json = (value) => `${JSON.stringify(value, null, 2)}\n`;

function git(root, ...args) {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8" });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")}: ${result.stderr || result.error}`);
  return result.stdout.trim();
}

export function sourceSha(root, env = process.env) {
  const sha = git(root, "rev-parse", "HEAD");
  assert.match(sha, /^[0-9a-f]{40}$/);
  // GITHUB_SHA is the tested synthetic merge on pull_request, NOT the PR head.
  if (env.GITHUB_SHA) assert.equal(sha, env.GITHUB_SHA, "checkout differs from event commit");
  return sha;
}

function assertClean(root) {
  const result = spawnSync("git", ["diff", "--quiet", "HEAD", "--"], { cwd: root });
  assert.equal(result.status, 0, "tracked checkout was modified; cannot label artifacts with HEAD");
}

export async function fileDigest(root, name) {
  const file = path.join(root, name);
  assert.ok((await lstat(file)).isFile(), `not a regular file: ${name}`);
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

export async function configuration(root, buildConfig = config) {
  const inputs = {};
  // Hash tracked build inputs without requiring Cargo in artifact consumers.
  const names = git(root, "ls-files", "-z", "Cargo.toml", "rust-toolchain.toml",
    ":(glob)crates/**/Cargo.toml", ".cargo", "scripts/build-web-demo.sh",
    ".github/actions/dev-wasm-build/action.yml").split("\0").filter(Boolean).sort();
  for (const name of names) inputs[name] = await fileDigest(root, name);
  for (const required of ["Cargo.toml", "rust-toolchain.toml", "crates/noon-web/Cargo.toml",
    "scripts/build-web-demo.sh", ".github/actions/dev-wasm-build/action.yml"]) {
    assert.ok(inputs[required], `missing tracked build input: ${required}`);
  }
  const toolchain = await readFile(path.join(root, "rust-toolchain.toml"), "utf8");
  const channel = toolchain.match(/^channel\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$/m)?.[1];
  assert.ok(channel, "expected an exact Rust version in rust-toolchain.toml");
  return { ...buildConfig, toolchain: channel, inputs };
}

export function trustedWriter(env) {
  const requested = env.NOON_CI_PUBLISH_CACHE ?? "false";
  assert.ok(["true", "false"].includes(requested), "invalid publish-cache input");
  const trusted = env.GITHUB_REF === "refs/heads/master"
    && ["push", "workflow_dispatch"].includes(env.GITHUB_EVENT_NAME);
  assert.ok(requested !== "true" || trusted, "cache publication requires a trusted master run");
  return requested === "true" && trusted;
}

export function validateEnvironment(env) {
  if (env.NOON_CI_PREFLIGHT !== undefined) {
    assert.ok(["true", "false"].includes(env.NOON_CI_PREFLIGHT), "invalid preflight input");
  }
  // Do not silently give customized builds the default configuration's identity.
  for (const [name, value] of Object.entries(env)) {
    if (!value) continue;
    if (/^(RUSTFLAGS|RUSTC|RUSTC_WORKSPACE_WRAPPER|RUSTUP_TOOLCHAIN|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|CARGO_BUILD_TARGET|CARGO_TARGET_DIR|CARGO_TARGET_.*_(RUSTFLAGS|LINKER)|CARGO_PROFILE_DEV_.*)$/.test(name)) {
      assert.ok(name === "CARGO_PROFILE_DEV_DEBUG" && value === config.debug,
        `unsupported dev-WASM environment override: ${name}`);
    }
  }
  if (env.CARGO_INCREMENTAL) assert.equal(env.CARGO_INCREMENTAL, config.incremental);
  if (env.RUSTC_WRAPPER) assert.equal(env.RUSTC_WRAPPER, "sccache");
}

// Shared artifact identity for dev consumers and the two release product builds.
export async function prepareArtifact(root, env, compiler, buildConfig = config) {
  assertClean(root);
  const build = await configuration(root, buildConfig);
  assert.match(compiler, new RegExp(`^release: ${build.toolchain.replaceAll(".", "\\.")}$`, "m"),
    "compiler does not match the repository pin");
  return { schema: 1, source: sourceSha(root, env), build, compiler,
    dependencyLock: await fileDigest(root, "Cargo.lock") };
}

export async function prepare(root, env, compiler) {
  validateEnvironment(env);
  const identity = await prepareArtifact(root, env, compiler);
  const { source, build, dependencyLock } = identity;
  const cacheRef = env.NOON_CI_CACHE_REF || source;
  assert.match(cacheRef, /^[0-9a-f]{40}$/, "invalid cache revision");
  const namespace = `${env.RUNNER_OS}-${env.RUNNER_ARCH}-wasm-build-v2-${digest(json({ build, compiler }))}-`;
  return { schema: 1, source, build, compiler, dependencyLock,
    key: `${namespace}${dependencyLock}-${cacheRef}`,
    restoreKeys: [`${namespace}${dependencyLock}-`, namespace,
      // Legacy seeds contain compiler objects, never web/pkg or Cargo target binaries.
      // Cargo and sccache still validate the current invocation after any fallback.
      `${env.RUNNER_OS}-wasm-build-v1-`] };
}

async function packageFiles(root) {
  const files = [];
  assert.ok((await lstat(path.join(root, "web/python"))).isDirectory(), "Python artifact directory is not a real directory");
  async function visit(name) {
    const info = await lstat(path.join(root, name));
    assert.ok(!info.isSymbolicLink(), `symlink in artifact: ${name}`);
    if (info.isDirectory()) {
      for (const entry of (await readdir(path.join(root, name))).sort()) {
        // upload-artifact excludes hidden files; the generated .gitignore is not runtime data.
        if (!entry.startsWith(".")) await visit(`${name}/${entry}`);
      }
    } else {
      assert.ok(info.isFile(), `unsupported artifact entry: ${name}`);
      files.push(name);
    }
  }
  await visit("web/pkg");
  for (const required of ["web/pkg/noon_web.js", "web/pkg/noon_web_bg.wasm", "web/pkg/package.json"]) {
    assert.ok(files.includes(required), `missing generated package file: ${required}`);
  }
  const worker = await readFile(path.join(root, "web/python-worker.js"), "utf8");
  const references = [...new Set(worker.match(/compat-bundle\.[0-9a-f]{64}\.json/g) || [])];
  const bundles = (await readdir(path.join(root, "web/python"))).filter((name) => bundlePattern.test(name));
  assert.equal(references.length, 1, "worker must reference exactly one immutable bundle");
  assert.deepEqual(bundles.sort(), references, "stale or missing Python compatibility bundle");
  assert.ok(!worker.includes("./python/compat-bundle.json"), "mutable bundle reference");
  return [...files, "web/python-worker.js", `web/python/${references[0]}`, lockPath].sort();
}

export async function stamp(root, prepared, env = process.env, buildConfig = config) {
  assert.equal(prepared.source, sourceSha(root, env), "source changed during build");
  assert.deepEqual(prepared.build, await configuration(root, buildConfig), "configuration changed during build");
  assert.equal(prepared.dependencyLock, await fileDigest(root, "Cargo.lock"), "dependency resolution changed during build");
  assertClean(root);
  await copyFile(path.join(root, "Cargo.lock"), path.join(root, lockPath));
  const files = {};
  for (const name of await packageFiles(root)) files[name] = await fileDigest(root, name);
  const manifest = { schema: 1, source: prepared.source, build: prepared.build,
    compiler: prepared.compiler, dependencyLock: prepared.dependencyLock, files };
  await writeFile(path.join(root, manifestPath), json(manifest));
  return manifest;
}

export async function verify(root, env = process.env, buildConfig = config) {
  const manifest = JSON.parse(await readFile(path.join(root, manifestPath), "utf8"));
  assert.equal(manifest.schema, 1, "unsupported artifact schema");
  assert.equal(manifest.source, sourceSha(root, env), "artifact is not from this checkout");
  assert.deepEqual(manifest.build, await configuration(root, buildConfig), "artifact configuration mismatch");
  assert.match(manifest.compiler, new RegExp(`^release: ${manifest.build.toolchain.replaceAll(".", "\\.")}$`, "m"),
    "artifact compiler does not match the repository pin");
  // Inventory is derived locally, never traversed from untrusted manifest paths.
  assert.deepEqual(Object.keys(manifest.files).sort(), await packageFiles(root), "artifact inventory mismatch");
  for (const [name, hash] of Object.entries(manifest.files)) {
    assert.equal(await fileDigest(root, name), hash, `artifact content mismatch: ${name}`);
  }
  assert.equal(manifest.dependencyLock, manifest.files[lockPath], "artifact lockfile mismatch");
  // A future tracked lockfile must also agree with the producer's resolution.
  if (git(root, "ls-files", "Cargo.lock")) {
    assert.equal(await fileDigest(root, "Cargo.lock"), manifest.dependencyLock, "checkout lockfile mismatch");
  }
  assertClean(root);
  return manifest;
}

async function output(name, value) {
  if (process.env.GITHUB_OUTPUT) {
    assert.ok(!String(value).includes("\n"), "multiline scalar output");
    await appendFile(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
  }
}

async function record(root, phase, value) {
  assert.match(phase, /^[a-z][a-z0-9-]*$/);
  await mkdir(path.join(root, evidence), { recursive: true });
  await writeFile(path.join(root, evidence, `${phase}.json`), json(value));
}

export async function measure(root, phase, command) {
  assert.ok(command.length, "missing measured command");
  const start = performance.now();
  const result = spawnSync(command[0], command.slice(1), { cwd: root, stdio: "inherit" });
  const status = result.status ?? (result.signal ? 128 + (constants.signals[result.signal] ?? 0) : 127);
  const seconds = (performance.now() - start) / 1000;
  await record(root, phase, { seconds, status, signal: result.signal });
  await output("seconds", Math.round(seconds));
  if (result.error) console.error(result.error.message);
  return status;
}

async function report(root) {
  await mkdir(path.join(root, evidence), { recursive: true });
  const phases = {};
  for (const name of (await readdir(path.join(root, evidence))).sort()) {
    if (name.endsWith(".json") && !["identity.json", "report.json", "sccache.json"].includes(name)) {
      phases[name.slice(0, -5)] = JSON.parse(await readFile(path.join(root, evidence, name), "utf8"));
    }
  }
  const stats = spawnSync("sccache", ["--show-stats", "--stats-format=json"], { encoding: "utf8" });
  let compilerCache = null;
  if (stats.status === 0) {
    try { compilerCache = JSON.parse(stats.stdout); } catch { /* Report unavailable, not zero hits. */ }
  }
  let identity = null;
  try { identity = JSON.parse(await readFile(path.join(root, evidence, "identity.json"), "utf8")); } catch { /* Setup failed. */ }
  const result = { source: sourceSha(root, {}), eventSource: process.env.GITHUB_SHA || null, identity, phases, compilerCache,
    requestedKey: process.env.NOON_CI_REQUESTED_KEY || null,
    matchedKey: process.env.NOON_CI_MATCHED_KEY || null,
    exactCacheHit: process.env.NOON_CI_CACHE_HIT ? process.env.NOON_CI_CACHE_HIT === "true" : null,
    cacheOutcome: process.env.NOON_CI_CACHE_OUTCOME || "unknown",
    buildOutcome: process.env.NOON_CI_BUILD_OUTCOME || "unknown",
    note: "Phase times exclude hosted-runner queue and post-job cleanup. Artifact upload is timed by Actions. Null stats are unavailable, not zero." };
  await record(root, "report", result);
  const lines = ["### Dev WASM build evidence", "", `Source: \`${result.source}\` (actual checkout).`,
    `Requested cache: \`${result.requestedKey ?? "unavailable"}\`.`,
    `Restored cache: \`${result.matchedKey ?? "miss / unavailable"}\`; exact match: ${result.exactCacheHit ?? "n/a"}; restore outcome: ${result.cacheOutcome}.`,
    `Build outcome: ${result.buildOutcome}. Cache reuse never skips compilation/package validation.`, "",
    "| Phase | Seconds | Exit status |", "| --- | ---: | ---: |"];
  for (const [name, value] of Object.entries(phases)) {
    lines.push(`| ${name} | ${value.seconds?.toFixed(3) ?? "incomplete"} | ${value.status ?? "n/a"} |`);
  }
  const cache = compilerCache?.stats;
  if (cache) {
    lines.push("", `Rust hits: ${cache.cache_hits?.counts?.Rust ?? 0}; Rust misses: ${cache.cache_misses?.counts?.Rust ?? 0}; non-cacheable calls: ${cache.requests_not_cacheable ?? "unknown"}.`);
  } else lines.push("", "sccache statistics unavailable.");
  lines.push("", result.note, "");
  console.log(lines.join("\n"));
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, lines.join("\n"));
}

async function main() {
  const root = process.cwd();
  const [command, ...args] = process.argv.slice(2);
  if (command === "init") {
    assert.equal(process.env.RUNNER_OS, "Linux");
    assert.equal(process.env.RUNNER_ARCH, "X64");
    validateEnvironment(process.env);
    const writer = trustedWriter(process.env);
    await mkdir(path.join(root, evidence), { recursive: true });
    await output("publish", writer);
    const values = { CARGO_INCREMENTAL: "0", CARGO_PROFILE_DEV_DEBUG: "0", RUSTC_WRAPPER: "sccache",
      SCCACHE_GHA_ENABLED: "false", SCCACHE_DIR: "/home/runner/.cache/noon-sccache-wasm",
      SCCACHE_CACHE_SIZE: "1G", SCCACHE_LOCAL_RW_MODE: writer ? "READ_WRITE" : "READ_ONLY" };
    await appendFile(process.env.GITHUB_ENV, Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join(""));
  } else if (command === "prepare") {
    const compiler = spawnSync("rustc", ["-vV"], { encoding: "utf8" });
    assert.equal(compiler.status, 0, "rustc -vV failed");
    const identity = await prepare(root, process.env, compiler.stdout.trim());
    await record(root, "identity", identity);
    await output("key", identity.key);
    for (const [i, key] of identity.restoreKeys.entries()) await output(`restore-${i}`, key);
  } else if (command === "stamp") {
    await stamp(root, JSON.parse(await readFile(path.join(root, evidence, "identity.json"), "utf8")));
  } else if (command === "verify") {
    const manifest = await verify(root);
    console.log(`Verified dev-WASM artifact for ${manifest.source}: ${Object.keys(manifest.files).length} files.`);
  } else if (command === "measure") {
    process.exitCode = await measure(root, args[0], args.slice(1));
  } else if (command === "start") {
    await record(root, args[0], { started: Date.now() });
  } else if (command === "end") {
    assert.match(args[0], /^[a-z][a-z0-9-]*$/);
    const previous = JSON.parse(await readFile(path.join(root, evidence, `${args[0]}.json`), "utf8"));
    await record(root, args[0], { seconds: (Date.now() - previous.started) / 1000 });
  } else if (command === "report") {
    await report(root);
  } else throw new Error("expected init, prepare, stamp, verify, measure, start, end, or report");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error); process.exitCode = 1; });
}

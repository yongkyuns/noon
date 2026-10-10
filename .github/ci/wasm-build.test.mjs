import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { productConfig, validateProductEnvironment, resolveProductSources,
  prepareProductArtifact, prepareProductDependencies, stampProductArtifact, verifyProductArtifact } from "./product-artifact.mjs";
import { configuration, measure, prepare, prepareArtifact, sourceSha, stamp, trustedWriter,
  validateEnvironment, verify, packageSizes, summarizePackageSizes } from "./wasm-build.mjs";

const env = { RUNNER_OS: "Linux", RUNNER_ARCH: "X64" };
const compiler = "rustc 1.98.0\nhost: x86_64-unknown-linux-gnu\nrelease: 1.98.0";
const bundle = `compat-bundle.${"a".repeat(64)}.json`;

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-wasm-ci-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const put = async (name, content) => {
    await mkdir(path.dirname(path.join(root, name)), { recursive: true });
    await writeFile(path.join(root, name), content);
  };
  await put("Cargo.toml", '[workspace]\nmembers = ["crates/noon-web"]\n');
  await put("rust-toolchain.toml", '[toolchain]\nchannel = "1.98.0"\n');
  await put("crates/noon-web/Cargo.toml", '[package]\nname = "noon-web"\nversion = "0.1.0"\n');
  await put("scripts/build-web-demo.sh", "wasm-pack build\n");
  await put("crates/noon-web/src/lib.rs", "// source fixture\n");
  await put(".github/actions/dev-wasm-build/action.yml", "name: test\n");
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "-q");
  git("add", ".");
  git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", "commit", "-qm", "fixture");
  await put("Cargo.lock", "version = 4\n");
  await put("web/pkg/noon_web.js", "export default function init() {}\n");
  await put("web/pkg/noon_web_bg.wasm", Buffer.from([0, 97, 115, 109]));
  await put("web/pkg/package.json", '{"type":"module"}\n');
  await put("web/pkg/.gitignore", "*\n");
  await put("web/python-worker.js", `fetch("./python/${bundle}");\n`);
  await put(`web/python/${bundle}`, '{"version":1,"modules":[]}\n');
  return { root, put, git };
}

async function stamped(t) {
  const result = await fixture(t);
  const identity = await prepare(result.root, env, compiler);
  const manifest = await stamp(result.root, identity, env);
  return { ...result, identity, manifest };
}

async function productFixture(t) {
  const result = await fixture(t);
  const { git, put } = result;
  const eventBase = git("rev-parse", "HEAD");
  const commit = (message, ...parents) => git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid",
    "commit-tree", git("write-tree"), ...parents.flatMap(sha => ["-p", sha]), "-m", message);
  await put("crates/noon-web/src/lib.rs", "// PR change\n");
  git("add", "crates/noon-web/src/lib.rs");
  const head = commit("PR head", eventBase);
  git("reset", "--hard", eventBase);
  await put("unrelated.txt", "target branch advanced after the event\n");
  git("add", "unrelated.txt");
  const baseline = commit("new target base", eventBase);
  await put("crates/noon-web/src/lib.rs", "// PR change\n");
  git("add", "crates/noon-web/src/lib.rs");
  const candidate = commit("tested merge", baseline, head);
  git("reset", "--hard", candidate);
  const sources = { schema: 1, candidate, baseline, head, eventBase };
  const expected = { ...env, GITHUB_SHA: candidate,
    NOON_PRODUCT_CANDIDATE_SHA: candidate, NOON_PRODUCT_BASE_SHA: baseline,
    NOON_PRODUCT_HEAD_SHA: head, NOON_PRODUCT_EVENT_BASE_SHA: eventBase,
    NOON_WASM_PROFILE: "release", NOON_WASM_SKIP_OPT: "0", NOON_RENDERER_SMOKE: "0" };
  return { ...result, commit, sources, expected };
}

test("product source selection excludes unrelated target changes from the PR comparison", async t => {
  const { root, git, sources, expected } = await productFixture(t);
  assert.notEqual(sources.eventBase, sources.baseline);
  assert.deepEqual(resolveProductSources(root, expected), sources);
  assert.equal(git("diff", "--name-only", sources.baseline, sources.candidate), "crates/noon-web/src/lib.rs");
  assert.match(git("diff", "--name-only", sources.eventBase, sources.candidate), /unrelated\.txt/);
  const shallow = await mkdtemp(path.join(os.tmpdir(), "noon-product-shallow-"));
  t.after(() => rm(shallow, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", "--depth", "2", `file://${root}`, shallow]);
  assert.deepEqual(resolveProductSources(shallow, expected), sources);
  assert.equal(execFileSync("git", ["rev-parse", "--is-shallow-repository"], { cwd: shallow, encoding: "utf8" }).trim(), "true");
});

test("product source selection rejects substituted heads, incomplete history, and non-PR merges", async t => {
  const { root, git, commit, sources, expected } = await productFixture(t);
  assert.throws(() => resolveProductSources(root, { ...expected, GITHUB_SHA: sources.head }), /checkout differs/);
  assert.throws(() => resolveProductSources(root, { ...expected, NOON_PRODUCT_HEAD_SHA: sources.eventBase }), /requested PR head/);
  assert.throws(() => resolveProductSources(root, { ...expected, NOON_PRODUCT_EVENT_BASE_SHA: "bad" }), /event base SHA/);
  const octopus = commit("octopus", sources.baseline, sources.head, sources.eventBase);
  git("reset", "--hard", octopus);
  assert.throws(() => resolveProductSources(root, { ...expected, GITHUB_SHA: octopus }), /two-parent merge/);
  git("reset", "--hard", sources.head);
  assert.throws(() => resolveProductSources(root, { ...expected, GITHUB_SHA: sources.head }), /two-parent merge/);
  git("reset", "--hard", sources.candidate);
  await writeFile(path.join(root, ".git/shallow"), `${sources.candidate}\n`);
  assert.throws(() => resolveProductSources(root, expected), /fetched parents/);
});

test("product source CLI pins validated commits in Actions outputs and environment", async t => {
  const { root, expected, sources } = await productFixture(t);
  const output = path.join(root, "actions-output");
  const environment = path.join(root, "actions-env");
  execFileSync(process.execPath, [fileURLToPath(new URL("./product-artifact.mjs", import.meta.url)), "sources", root],
    { env: { ...process.env, ...expected, GITHUB_OUTPUT: output, GITHUB_ENV: environment } });
  assert.equal(await readFile(output, "utf8"), `candidate-sha=${sources.candidate}\nbaseline-sha=${sources.baseline}\nhead-sha=${sources.head}\nevent-base-sha=${sources.eventBase}\n`);
  assert.equal(await readFile(environment, "utf8"), `NOON_PRODUCT_CANDIDATE_SHA=${sources.candidate}\nNOON_PRODUCT_BASE_SHA=${sources.baseline}\nNOON_PRODUCT_HEAD_SHA=${sources.head}\nNOON_PRODUCT_EVENT_BASE_SHA=${sources.eventBase}\n`);
});

function fakeCargo({ rejectMetadata } = {}) {
  const calls = [];
  let resolutions = 0;
  return {
    calls,
    async runCargo(root, args) {
      calls.push({ root, args });
      if (args[0] === "generate-lockfile") {
        resolutions += 1;
        const version = resolutions === 1 ? "0.2.20" : "0.2.21";
        await writeFile(path.join(root, "Cargo.lock"),
          `version = 4\n# resolver selected thin-vec ${version}\n\n[[package]]\nname = "noon-web"\nversion = "0.1.0"\n`);
        return "";
      }
      assert.deepEqual(args, ["metadata", "--locked", "--format-version", "1"]);
      if (rejectMetadata?.(root)) throw new Error("cargo metadata --locked rejected copied Cargo.lock");
      return JSON.stringify({ packages: [], resolve: { nodes: [] } });
    },
  };
}

test("product dependency preparation reuses one baseline lock for matching inputs", async (t) => {
  const { root, sources } = await productFixture(t);
  const baseline = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-baseline-"));
  t.after(() => rm(baseline, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", `file://${root}`, baseline]);
  execFileSync("git", ["checkout", "--quiet", sources.baseline], { cwd: baseline });

  const cargo = fakeCargo();
  const prepared = await prepareProductDependencies(baseline, [root], { runCargo: cargo.runCargo });
  const baselineLock = await readFile(path.join(baseline, "Cargo.lock"), "utf8");
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 1,
    "matching candidate inputs must not resolve dependencies a second time");
  assert.equal(cargo.calls.filter(call => call.args[0] === "metadata").length, 2);
  assert.ok(cargo.calls.filter(call => call.args[0] === "metadata")
    .every(call => call.args.includes("--locked") && !call.args.includes("--no-deps")));
  assert.match(baselineLock, /thin-vec 0\.2\.20/);
  assert.equal(await readFile(path.join(root, "Cargo.lock"), "utf8"), baselineLock);
  assert.deepEqual(prepared.results.map(result => result.mode), ["resolved", "reused-baseline"]);
});

test("changed dependency manifests resolve independently", async (t) => {
  const { root } = await fixture(t);
  const baseline = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-baseline-"));
  t.after(() => rm(baseline, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", `file://${root}`, baseline]);
  await writeFile(path.join(root, "crates/noon-web/Cargo.toml"),
    '[package]\nname = "noon-web"\nversion = "0.1.0"\n[package.metadata.ci]\nvariant = "candidate"\n');

  const cargo = fakeCargo();
  const prepared = await prepareProductDependencies(baseline, [root], { runCargo: cargo.runCargo });
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 2);
  assert.deepEqual(prepared.results.map(result => result.mode), ["resolved", "independently-resolved"]);
  assert.equal(prepared.results[1].inputsMatchBaseline, false);
  assert.notEqual(await readFile(path.join(baseline, "Cargo.lock"), "utf8"),
    await readFile(path.join(root, "Cargo.lock"), "utf8"));
});

test("changed tracked Cargo configuration resolves independently", async (t) => {
  const { root } = await fixture(t);
  const target = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-target-"));
  t.after(() => rm(target, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", `file://${root}`, target]);
  await mkdir(path.join(target, ".cargo"), { recursive: true });
  await writeFile(path.join(target, ".cargo/config.toml"), '[build]\nrustflags = ["--cfg", "product_candidate"]\n');
  execFileSync("git", ["add", ".cargo/config.toml"], { cwd: target });
  execFileSync("git", ["-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid",
    "commit", "-qm", "change cargo configuration"], { cwd: target });

  const cargo = fakeCargo();
  const prepared = await prepareProductDependencies(root, [target], { runCargo: cargo.runCargo });
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 2);
  assert.equal(prepared.results[1].inputsMatchBaseline, false);
  assert.equal(prepared.results[1].mode, "independently-resolved");
});

test("tracked Cargo.lock files are preserved and validated without resolver writes", async (t) => {
  const { root, git } = await fixture(t);
  await writeFile(path.join(root, "Cargo.lock"),
    'version = 4\n# authored lock\n\n[[package]]\nname = "noon-web"\nversion = "0.1.0"\n');
  git("add", "Cargo.lock");
  git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", "commit", "-qm", "track authored lock");
  const before = await readFile(path.join(root, "Cargo.lock"), "utf8");
  const cargo = fakeCargo();
  const prepared = await prepareProductDependencies(root, [root], {
    runCargo: cargo.runCargo,
  });
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 0);
  assert.equal(cargo.calls.filter(call => call.args[0] === "metadata").length, 2);
  assert.equal(await readFile(path.join(root, "Cargo.lock"), "utf8"), before);
  assert.deepEqual(prepared.results.map(result => result.mode), ["tracked-preserved", "tracked-preserved"]);
});

test("symlink Cargo.lock is rejected before a reusable lock can be copied", async (t) => {
  const { root } = await fixture(t);
  const target = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-target-"));
  t.after(() => rm(target, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", `file://${root}`, target]);
  const sentinel = path.join(os.tmpdir(), `noon-product-lock-sentinel-${process.pid}`);
  await writeFile(sentinel, "leave untouched\n");
  t.after(() => rm(sentinel, { force: true }));
  await symlink(sentinel, path.join(target, "Cargo.lock"));
  const cargo = fakeCargo();
  await assert.rejects(prepareProductDependencies(root, [target], {
    runCargo: cargo.runCargo,
  }), /refusing symlink Cargo\.lock/);
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 1,
    "only baseline resolution may precede target symlink rejection");
  assert.equal(await readFile(sentinel, "utf8"), "leave untouched\n");
});

test("a copied lock that fails --locked metadata validation is not regenerated", async (t) => {
  const { root } = await fixture(t);
  const baseline = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-baseline-"));
  t.after(() => rm(baseline, { recursive: true, force: true }));
  execFileSync("git", ["clone", "--quiet", `file://${root}`, baseline]);
  const cargo = fakeCargo({ rejectMetadata: checkout => checkout === root });
  await assert.rejects(prepareProductDependencies(baseline, [root], { runCargo: cargo.runCargo }),
    /rejected copied Cargo\.lock/);
  assert.equal(cargo.calls.filter(call => call.args[0] === "generate-lockfile").length, 1,
    "metadata failure must not fall back to another resolution");
  assert.equal(cargo.calls.filter(call => call.args[0] === "metadata").length, 2);
  assert.equal(await readFile(path.join(root, "Cargo.lock"), "utf8"), await readFile(path.join(baseline, "Cargo.lock"), "utf8"));
});

test("offline Cargo validation rejects an incomplete local dependency lock without rewriting it", async (t) => {
  const { root, git, put } = await fixture(t);
  const { toolchain } = await configuration(fileURLToPath(new URL("../../", import.meta.url)));
  await put("rust-toolchain.toml", `[toolchain]\nchannel = "${toolchain}"\n`);
  await put("Cargo.toml", '[workspace]\nmembers = ["crates/noon-web", "crates/local-helper"]\nresolver = "2"\n');
  await put("crates/noon-web/Cargo.toml", '[package]\nname = "noon-web"\nversion = "0.1.0"\n[dependencies]\nlocal-helper = { path = "../local-helper" }\n');
  await put("crates/local-helper/Cargo.toml", '[package]\nname = "local-helper"\nversion = "0.1.0"\n');
  await put("crates/local-helper/src/lib.rs", "pub fn value() -> u32 { 1 }\n");
  git("add", "Cargo.toml", "rust-toolchain.toml", "crates");
  git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", "commit", "-qm", "add offline path dependency");

  const baseline = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-baseline-"));
  const target = await mkdtemp(path.join(os.tmpdir(), "noon-product-lock-target-"));
  t.after(() => Promise.all([
    rm(baseline, { recursive: true, force: true }),
    rm(target, { recursive: true, force: true }),
  ]));
  for (const checkout of [baseline, target]) {
    execFileSync("git", ["clone", "--quiet", "file://" + root, checkout]);
  }

  const cargoCalls = [];
  const runCargo = async (checkout, args) => {
    cargoCalls.push({ checkout, args });
    const result = spawnSync("cargo", args, {
      cwd: checkout,
      encoding: "utf8",
      // This metadata-only fixture runs before CI installs compilation caches.
      env: { ...process.env, CARGO_NET_OFFLINE: "true",
        RUSTC_WRAPPER: "", RUSTC_WORKSPACE_WRAPPER: "" },
    });
    assert.equal(result.status, 0, "cargo " + args.join(" ") + " failed: " + (result.stderr || result.error));
    return result.stdout;
  };

  const prepared = await prepareProductDependencies(baseline, [target], { runCargo });
  assert.deepEqual(prepared.results.map(result => result.mode), ["resolved", "reused-baseline"]);
  assert.deepEqual(cargoCalls.filter(call => call.args[0] === "metadata").map(call => call.args), [
    ["metadata", "--locked", "--format-version", "1"],
    ["metadata", "--locked", "--format-version", "1"],
  ]);
  const baselineLock = await readFile(path.join(baseline, "Cargo.lock"), "utf8");
  assert.match(baselineLock, /name = "local-helper"/);
  assert.equal(await readFile(path.join(target, "Cargo.lock"), "utf8"), baselineLock);

  for (const checkout of [baseline, target]) execFileSync("git", ["add", "Cargo.lock"], { cwd: checkout });
  const incompleteLock = baselineLock.replace(
    /\n\[\[package\]\]\nname = "local-helper"\nversion = "0\.1\.0"\n/, "\n");
  assert.notEqual(incompleteLock, baselineLock, "fixture lock must contain a removable local package entry");
  await writeFile(path.join(target, "Cargo.lock"), incompleteLock);
  execFileSync("git", ["add", "Cargo.lock"], { cwd: target });
  const beforeRejectedValidation = cargoCalls.length;

  await assert.rejects(prepareProductDependencies(baseline, [target], { runCargo }),
    /cargo metadata --locked.*failed|lock file.*needs to be updated/i);
  assert.equal(cargoCalls.length - beforeRejectedValidation, 2,
    "the second pass should validate baseline and target without resolving again");
  assert.ok(cargoCalls.slice(beforeRejectedValidation).every(call => call.args[0] === "metadata"));
  assert.equal(await readFile(path.join(target, "Cargo.lock"), "utf8"), incompleteLock,
    "failed validation must leave the copied lock byte-for-byte unchanged");
});

test("same source/configuration round-trips without Cargo in the consumer", async (t) => {
  const { root, manifest } = await stamped(t);
  assert.deepEqual(await verify(root, env), manifest);
  assert.equal(manifest.build.profile, "dev");
  assert.ok(!manifest.files["web/pkg/.gitignore"]);
  assert.ok(manifest.files["web/ci-Cargo.lock"]);
});

test("cache identity separates dependencies, compiler, flags, and build inputs", async (t) => {
  const { root, put, git } = await fixture(t);
  const original = await prepare(root, env, compiler);
  assert.equal(original.key, (await prepare(root, env, compiler)).key);
  await put("Cargo.lock", "version = 4\n# changed resolution\n");
  const changedLock = await prepare(root, env, compiler);
  assert.notEqual(changedLock.key, original.key);
  assert.equal(changedLock.restoreKeys[1], original.restoreKeys[1]);
  assert.notEqual((await prepare(root, env, `${compiler}\ncommit-hash: different`)).restoreKeys[1], original.restoreKeys[1]);
  await put(".cargo/config.toml", '[build]\nrustflags = ["--cfg", "test_ci"]\n');
  git("add", ".cargo/config.toml");
  git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", "commit", "-qm", "change build configuration");
  assert.notEqual((await prepare(root, env, compiler)).restoreKeys[1], original.restoreKeys[1]);
  for (const name of ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_DEV_OPT_LEVEL", "RUSTUP_TOOLCHAIN", "CARGO_TARGET_DIR"]) {
    assert.throws(() => validateEnvironment({ [name]: "custom" }), /unsupported/);
  }
  await assert.rejects(prepare(root, env, compiler.replaceAll("1.98.0", "1.99.0")), /compiler does not match/);
});

test("cache base selects only a compiler seed, not the artifact source", async (t) => {
  const { root } = await fixture(t);
  const base = "b".repeat(40);
  const identity = await prepare(root, { ...env, NOON_CI_CACHE_REF: base }, compiler);
  assert.ok(identity.key.endsWith(base));
  assert.equal(identity.source, sourceSha(root, env));
  assert.notEqual(identity.source, base);
  assert.throws(() => sourceSha(root, { GITHUB_SHA: base }), /checkout differs/);
  await assert.rejects(prepare(root, { ...env, NOON_CI_CACHE_REF: "head\nkey=injected" }, compiler), /invalid cache revision/);
});

test("PR head cannot stand in for the tested synthetic merge commit", async (t) => {
  const { root, manifest, put } = await stamped(t);
  manifest.source = "c".repeat(40);
  await put("web/ci-artifact.json", JSON.stringify(manifest));
  await assert.rejects(verify(root, env), /not from this checkout/);
});

test("dev/release and feature mismatches fail closed", async (t) => {
  const { root, manifest, put } = await stamped(t);
  for (const change of [{ profile: "release" }, { features: "all" }, { skipOpt: "0" }, { debug: "2" }]) {
    await put("web/ci-artifact.json", JSON.stringify({ ...manifest, build: { ...manifest.build, ...change } }));
    await assert.rejects(verify(root, env), /configuration mismatch/);
  }
});

for (const name of ["web/pkg/noon_web_bg.wasm", "web/pkg/noon_web.js", "web/python-worker.js", `web/python/${bundle}`, "web/ci-Cargo.lock"]) {
  test(`corrupted generated file is rejected: ${name}`, async (t) => {
    const { root, put } = await stamped(t);
    const original = await readFile(path.join(root, name));
    await put(name, Buffer.concat([original, Buffer.from("corrupt")]));
    await assert.rejects(verify(root, env), /content mismatch/);
  });
}

test("missing, extra, and substituted bundles are rejected", async (t) => {
  const { root, put } = await stamped(t);
  const extra = `web/python/compat-bundle.${"d".repeat(64)}.json`;
  await put(extra, "{}");
  await assert.rejects(verify(root, env), /stale or missing/);
  await rm(path.join(root, extra));
  await rm(path.join(root, `web/python/${bundle}`));
  await assert.rejects(verify(root, env), /stale or missing/);
});

test("manifest cannot add traversal paths or omit hashes", async (t) => {
  const { root, put, manifest } = await stamped(t);
  for (const files of [{ ...manifest.files, "../../outside": "hash" }, {}]) {
    await put("web/ci-artifact.json", JSON.stringify({ ...manifest, files }));
    await assert.rejects(verify(root, env), /inventory mismatch/);
  }
});

test("symlinked package entries are rejected", async (t) => {
  const { root } = await stamped(t);
  await symlink("../ci-Cargo.lock", path.join(root, "web/pkg/injected"));
  await assert.rejects(verify(root, env), /symlink/);
});

test("changed dependency resolution cannot be stamped as the prepared build", async (t) => {
  const { root, put, identity } = await stamped(t);
  await put("Cargo.lock", "different dependency graph");
  await assert.rejects(stamp(root, identity, env), /resolution changed/);
});

test("future tracked lockfiles must match the downloaded artifact", async (t) => {
  const { root, put, git } = await fixture(t);
  git("add", "Cargo.lock");
  git("-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", "commit", "-qm", "track lock");
  await stamp(root, await prepare(root, env, compiler), env);
  await put("Cargo.lock", "changed checkout lock");
  await assert.rejects(verify(root, env), /checkout lockfile mismatch/);
});

test("only explicitly requested master push/dispatch may publish compiler caches", () => {
  for (const event of ["pull_request", "pull_request_target", "workflow_run", "schedule"]) {
    assert.throws(() => trustedWriter({ NOON_CI_PUBLISH_CACHE: "true", GITHUB_REF: "refs/heads/master", GITHUB_EVENT_NAME: event }), /trusted master/);
  }
  for (const event of ["push", "workflow_dispatch"]) {
    assert.equal(trustedWriter({ NOON_CI_PUBLISH_CACHE: "true", GITHUB_REF: "refs/heads/master", GITHUB_EVENT_NAME: event }), true);
    assert.throws(() => trustedWriter({ NOON_CI_PUBLISH_CACHE: "true", GITHUB_REF: "refs/heads/topic", GITHUB_EVENT_NAME: event }), /trusted master/);
  }
  assert.equal(trustedWriter({}), false);
});

test("measurement preserves command failure and writes evidence", async (t) => {
  const { root } = await fixture(t);
  assert.equal(await measure(root, "failing-build", [process.execPath, "-e", "process.exit(23)"]), 23);
  const result = JSON.parse(await readFile(path.join(root, "ci-artifacts/wasm-build/failing-build.json"), "utf8"));
  assert.equal(result.status, 23);
  assert.ok(result.seconds >= 0);
  await assert.rejects(measure(root, "empty", []), /missing measured command/);
});

test("invalid preflight input cannot silently skip checks", () => {
  for (const value of ["true", "false"]) validateEnvironment({ NOON_CI_PREFLIGHT: value });
  assert.throws(() => validateEnvironment({ NOON_CI_PREFLIGHT: "tru" }), /invalid preflight/);
});

test("artifact compiler evidence must match the repository pin", async (t) => {
  const { root, manifest, put } = await stamped(t);
  await put("web/ci-artifact.json", JSON.stringify({ ...manifest, compiler: compiler.replaceAll("1.98.0", "1.99.0") }));
  await assert.rejects(verify(root, env), /artifact compiler/);
});

test("Python artifact directory cannot be a symlink", async (t) => {
  const { root } = await stamped(t);
  await rm(path.join(root, "web/python"), { recursive: true });
  await symlink("pkg", path.join(root, "web/python"));
  await assert.rejects(verify(root, env), /not a real directory/);
});


test("dirty tracked source cannot be labeled as the HEAD artifact", async (t) => {
  const { root, put, identity } = await stamped(t);
  await put("crates/noon-web/src/lib.rs", "// modified after source checkout\n");
  await assert.rejects(prepare(root, env, compiler), /tracked checkout was modified/);
  await assert.rejects(stamp(root, identity, env), /tracked checkout was modified/);
  await assert.rejects(verify(root, env), /tracked checkout was modified/);
});

for (const role of ["baseline", "candidate", "candidate-fixture"]) {
  test(`${role} product package verifies the release role and producer source pair`, async (t) => {
    const { root, git, put, sources, expected } = await productFixture(t);
    if (role === "baseline") git("reset", "--hard", sources.baseline);
    expected.NOON_RENDERER_SMOKE = role === "candidate-fixture" ? "1" : "0";
    const identity = await prepareProductArtifact(root, expected, compiler, role);
    const manifest = await stampProductArtifact(root, identity, expected, role);
    assert.deepEqual(await verifyProductArtifact(root, expected, role), manifest);
    assert.deepEqual(manifest.productSources, sources);
    assert.equal(manifest.source, role === "baseline" ? sources.baseline : sources.candidate);
    assert.equal(manifest.build.profile, "release");
    // Retries consume the producer pair, not a newer event's/ref's source.
    assert.deepEqual(await verifyProductArtifact(root, { ...expected, GITHUB_SHA: "f".repeat(40) }, role), manifest);
    // Default dev artifacts and other release feature sets cannot substitute.
    const artifactEnv = { GITHUB_SHA: manifest.source };
    await assert.rejects(verify(root, artifactEnv), /configuration mismatch/);
    if (role === "candidate-fixture") {
      await assert.rejects(verify(root, artifactEnv, productConfig("candidate")), /configuration mismatch/);
    } else {
      assert.deepEqual(productConfig(role), productConfig(role === "baseline" ? "candidate" : "baseline"));
    }
    await assert.rejects(verifyProductArtifact(root, { ...expected, NOON_PRODUCT_BASE_SHA: sources.eventBase }, role),
      role === "baseline" ? /checkout differs/ : /merge parents/);
    await assert.rejects(verifyProductArtifact(root, { ...expected, NOON_PRODUCT_CANDIDATE_SHA: "f".repeat(40) }, role),
      role === "baseline" ? /source pair mismatch/ : /checkout differs/);
    await assert.rejects(verifyProductArtifact(root, { ...expected, NOON_PRODUCT_HEAD_SHA: undefined }, role), /head SHA/);
    await assert.rejects(stampProductArtifact(root, { ...identity, productSources: undefined }, expected, role),
      /source pair changed/);
    await put("web/ci-artifact.json", JSON.stringify({ ...manifest, productSources: { ...sources, eventBase: sources.baseline } }));
    await assert.rejects(verifyProductArtifact(root, expected, role), /source pair mismatch/);
    await put("web/ci-artifact.json", JSON.stringify({ ...manifest, productSources: undefined }));
    await assert.rejects(verifyProductArtifact(root, expected, role), /source pair mismatch/);
  });
}

test("product comparison uses matching optimized production builds", () => {
  const baseline = productConfig("baseline");
  const candidate = productConfig("candidate");
  const fixture = productConfig("candidate-fixture");
  assert.deepEqual(candidate, baseline);
  assert.equal(baseline.profile, "release");
  assert.equal(baseline.features, "default");
  assert.equal(baseline.skipOpt, "0");
  assert.equal(baseline.rendererSmoke, "0");
  assert.equal(baseline.binaryen, "132");
  assert.deepEqual(
    { ...fixture, features: baseline.features, rendererSmoke: baseline.rendererSmoke },
    baseline,
  );
  assert.equal(fixture.features, "default,renderer-smoke");
  assert.equal(fixture.rendererSmoke, "1");
});

test("product package role rejects build flags that would change its artifact identity", () => {
  for (const [role, smoke] of [["candidate", "0"], ["candidate-fixture", "1"]]) {
    validateProductEnvironment({
      NOON_WASM_PROFILE: "release",
      NOON_WASM_SKIP_OPT: "0",
      NOON_RENDERER_SMOKE: smoke,
    }, role);
  }
  assert.throws(() => validateProductEnvironment({
    NOON_WASM_PROFILE: "release",
    NOON_WASM_SKIP_OPT: "1",
    NOON_RENDERER_SMOKE: "0",
  }, "candidate"), /NOON_WASM_SKIP_OPT/);
  assert.throws(() => validateProductEnvironment({
    NOON_WASM_PROFILE: "release",
    NOON_WASM_SKIP_OPT: "0",
    NOON_RENDERER_SMOKE: "1",
  }, "candidate"), /NOON_RENDERER_SMOKE/);
});

test("product gate resolves the installer and verifies the downloaded package layouts", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  assert.match(workflow, /uses: \.\/candidate\/\.github\/actions\/install-wasm-opt/);
  const adjacentJob = workflow.slice(workflow.indexOf("  measure-adjacent:"), workflow.indexOf("  measure-cumulative:"));
  const compareJob = workflow.slice(workflow.indexOf("  compare:"));
  assert.match(adjacentJob, /NOON_WASM_PROFILE: "release"/);
  assert.match(adjacentJob, /NOON_WASM_SKIP_OPT: "0"/);
  assert.match(adjacentJob, /NOON_RENDERER_SMOKE: "0"/);
  assert.match(adjacentJob, /NOON_RENDERER_SMOKE: "1"[\s\S]*?renderer-init-failure-smoke\.mjs/);
  assert.match(adjacentJob, /cp -a \.\.\/candidate-fixture\/\. web\//);
  assert.match(compareJob, /needs: \\[build, measure-adjacent, measure-cumulative\\]/);
  const restoration = adjacentJob.slice(adjacentJob.indexOf("      - name: Restore candidate production package for benchmark"));
  assert.match(restoration, /artifact-ids: \$\{\{ needs\.build\.outputs\.candidate-artifact \}\}[\s\S]*?path: candidate\/web/);
});

test("product gate pins the tested merge and retries the producer's exact source pair", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const producer = workflow.slice(workflow.indexOf("  build:"), workflow.indexOf("  compare:"));
  const consumer = workflow.slice(workflow.indexOf("  compare:"));
  assert.match(producer, /ref: \$\{\{ github\.sha \}\}\n\s+fetch-depth: 2\n\s+path: candidate/);
  assert.match(producer, /product-artifact\.mjs sources candidate/);
  assert.match(producer, /NOON_PRODUCT_HEAD_SHA: \$\{\{ github\.event\.pull_request\.head\.sha \}\}/);
  assert.match(producer, /ref: \$\{\{ steps\.sources\.outputs\.baseline-sha \}\}/);
  for (const name of ["candidate", "baseline", "head", "event-base"]) {
    assert.match(producer, new RegExp(`${name}-sha: \\$\\{\\{ steps\\.sources\\.outputs\\.${name}-sha \\}\\}`));
    assert.match(consumer, new RegExp(`needs\\.build\\.outputs\\.${name}-sha`));
  }
  assert.match(consumer, /ref: \$\{\{ needs\.build\.outputs\.candidate-sha \}\}\n\s+fetch-depth: 2/);
  assert.match(consumer, /ref: \$\{\{ needs\.build\.outputs\.baseline-sha \}\}/);
  assert.doesNotMatch(consumer, /github\.event\.pull_request\.(base|head)\.sha|ref: \$\{\{ github\.sha/);
});

test("product gate resolves one matching dependency lock before its first package build", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const producer = workflow.slice(workflow.indexOf("  build:"), workflow.indexOf("  compare:"));
  const resolve = producer.indexOf("- name: Resolve product dependency locks before builds");
  const firstBuild = producer.indexOf("- name: Build baseline production package");
  assert.ok(resolve >= 0 && firstBuild > resolve, "shared lock preparation must precede the first release build");
  assert.match(producer.slice(resolve, firstBuild),
    /product-artifact\.mjs dependencies baseline anchor candidate/);
  assert.doesNotMatch(producer, /cargo generate-lockfile/,
    "independent resolutions belong in the single preparation step, not serial build steps");
  assert.match(producer, /Build candidate renderer-smoke fixture package[\s\S]*?prepare candidate-fixture candidate/);
  assert.match(producer, /Build candidate production package[\s\S]*?prepare candidate candidate/);
});

test("product gate requires PNG comparison controls after dependency setup", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const adjacentJob = workflow.slice(workflow.indexOf("  measure-adjacent:"), workflow.indexOf("  measure-cumulative:"));
  const controls = adjacentJob.indexOf("      - name: Test product comparison with PNG controls");
  assert.ok(controls > adjacentJob.indexOf("npm install --no-save --ignore-scripts"));
  const controlStep = adjacentJob.slice(controls, adjacentJob.indexOf("      - name:", controls + 10));
  assert.match(controlStep, /NOON_PRODUCT_IMAGE_TESTS: "1"/);
  assert.match(controlStep, /node --test web\/playground-product-compare-validation\.test\.mjs/);
});

test("product camera measurements reuse the restored packages and fixed alternating pairs", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const measurements = workflow.slice(workflow.indexOf("      - name: Measure three alternating product pairs"),
    workflow.indexOf("      - name: Upload product regression evidence"));
  assert.match(measurements, /product-performance-anchor.mjs cohorts/);
  assert.match(measurements, /read -r noon_example noon_directory/);
  assert.match(measurements, /for noon_pair in 1 2 3/);
  assert.match(measurements, /NOON_PRODUCT_EXAMPLE="\$noon_example"/);
  assert.match(measurements, /"\$noon_root\/\$noon_directory\/candidate" --pairs 3/);
  assert.doesNotMatch(measurements, /build-web-demo|cargo |wasm-pack/);
});

test("Pages builds use the shared optimized production WASM configuration", async () => {
  const workflow = await readFile(new URL("../workflows/pages.yml", import.meta.url), "utf8");
  const buildJob = workflow.slice(workflow.indexOf("  build:"), workflow.indexOf("\n  deploy:"));
  const installer = buildJob.indexOf("uses: ./.github/actions/install-wasm-opt");
  const browserBuild = buildJob.indexOf("- name: Build and validate playground");
  assert.ok(installer >= 0 && browserBuild > installer,
    "Pages must install the shared verified wasm-opt before building the browser package");

  const product = productConfig("candidate");
  assert.equal(product.profile, "release");
  assert.equal(product.skipOpt, "0");
  assert.equal(product.rendererSmoke, "0");
  assert.equal(product.features, "default");
  assert.match(buildJob, new RegExp(`^\\s+NOON_WASM_PROFILE: ${product.profile}$`, "m"));
  assert.match(buildJob, new RegExp(`^\\s+NOON_WASM_SKIP_OPT: "${product.skipOpt}"$`, "m"));
  assert.match(buildJob, new RegExp(`^\\s+NOON_RENDERER_SMOKE: "${product.rendererSmoke}"$`, "m"));
});

test("unknown product roles cannot select a default build", () => {
  assert.throws(() => productConfig("other"), /invalid product artifact role/);
});

test("package bytes derive from the hash-verified inventory and exclude lock provenance", async t => {
  const { root, manifest, put } = await stamped(t);
  const sizes = await packageSizes(root);
  assert.equal(sizes.compression, "none");
  assert.equal(sizes.files["web/pkg/noon_web_bg.wasm"].bytes, 4);
  assert.equal(sizes.files["web/pkg/noon_web_bg.wasm"].sha256, manifest.files["web/pkg/noon_web_bg.wasm"]);
  assert.equal(sizes.files["web/ci-Cargo.lock"], undefined);
  assert.equal(sizes.totalBytes, Object.values(sizes.files).reduce((total, file) => total + file.bytes, 0));
  assert.deepEqual(summarizePackageSizes(sizes.files), sizes);
  await put("web/pkg/noon_web_bg.wasm", "altered");
  await assert.rejects(packageSizes(root), /content mismatch/);
});

test("package size aggregation rejects impossible bytes and a forged file inventory", async t => {
  const { root } = await stamped(t);
  const { files } = await packageSizes(root);
  for (const bytes of [-1, null, Number.NaN, 2 ** 53]) {
    assert.throws(() => summarizePackageSizes({ ...files, "web/pkg/noon_web_bg.wasm": {
      ...files["web/pkg/noon_web_bg.wasm"], bytes,
    } }), /invalid package bytes/);
  }
  const oversized = Object.fromEntries(Object.entries(files).map(([name, file]) =>
    [name, { ...file, bytes: Number.MAX_SAFE_INTEGER - 1 }]));
  assert.throws(() => summarizePackageSizes(oversized), /total exceeded safe integer/);
  assert.throws(() => summarizePackageSizes({ ...files, "web/pkg/../outside": {
    sha256: "a".repeat(64), bytes: 1,
  } }), /invalid generated package size inventory/);
});

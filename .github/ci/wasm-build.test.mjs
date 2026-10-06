import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { productConfig, validateProductEnvironment, resolveProductSources,
  prepareProductArtifact, stampProductArtifact, verifyProductArtifact } from "./product-artifact.mjs";
import { measure, prepare, prepareArtifact, sourceSha, stamp, trustedWriter,
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
  const compareJob = workflow.slice(workflow.indexOf("  compare:"));
  assert.match(compareJob, /NOON_WASM_PROFILE: "release"/);
  assert.match(compareJob, /NOON_WASM_SKIP_OPT: "0"/);
  assert.match(compareJob, /NOON_RENDERER_SMOKE: "0"/);
  assert.match(compareJob, /NOON_RENDERER_SMOKE: "1"[\s\S]*?renderer-init-failure-smoke\.mjs/);
  assert.match(compareJob, /cp -a \.\.\/candidate-fixture\/\. web\//);
  const restoration = compareJob.slice(compareJob.indexOf("      - name: Restore candidate production package for benchmark"));
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

test("standalone host measurements revalidate the producer pair before opening a browser", async () => {
  const runner = await readFile(new URL("../../scripts/python-host-perf.mjs", import.meta.url), "utf8");
  assert.match(runner, /import \{ verifyProductArtifact \} from "\.\.\/\.github\/ci\/product-artifact\.mjs"/);
  const check = runner.indexOf("identities.push(await verifyProductArtifact(root, process.env, role))");
  assert.ok(check >= 0 && check < runner.indexOf("browser = await playwright.chromium.launch"));
  assert.doesNotMatch(runner, /process\.env\.GITHUB_SHA|import.*\{ verify \}/);
  // No private source resolver may compete with the upstream artifact contract.
  await assert.rejects(readFile(new URL("./product-baseline.mjs", import.meta.url)), { code: "ENOENT" });
});

test("product gate requires PNG comparison controls after dependency setup", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const compareJob = workflow.slice(workflow.indexOf("  compare:"));
  const controls = compareJob.indexOf("      - name: Test product comparison with PNG controls");
  assert.ok(controls > compareJob.indexOf("npm install --no-save --ignore-scripts"));
  const controlStep = compareJob.slice(controls, compareJob.indexOf("      - name:", controls + 10));
  assert.match(controlStep, /NOON_PRODUCT_IMAGE_TESTS: "1"/);
  assert.match(controlStep, /node --test scripts\/paired-product-metrics\.test\.mjs web\/playground-product-compare-validation\.test\.mjs/);
});

test("product camera measurements reuse the restored packages and seven fixed alternating pairs", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  const measurements = workflow.slice(workflow.indexOf("      - name: Measure seven alternating product pairs"),
    workflow.indexOf("      - name: Upload product regression evidence"));
  assert.match(measurements, /for noon_example in parity-square-and-circle showcase-camera-follows-path/);
  assert.match(measurements, /for noon_pair in 1 2 3 4 5 6 7/);
  assert.match(measurements, /noon_pair % 2 == 0/);
  assert.match(measurements, /NOON_PRODUCT_EXAMPLE="\$noon_example"/);
  assert.match(measurements, /product-gate\/camera\/candidate --pairs 7/);
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

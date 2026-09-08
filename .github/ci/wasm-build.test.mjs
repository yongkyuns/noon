import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { measure, prepare, sourceSha, stamp, trustedWriter,
  validateEnvironment, verify } from "./wasm-build.mjs";

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

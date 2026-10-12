// Exercise product provenance through runtime identity generation, not just Git ancestry.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { prepareProductArtifact, stampProductArtifact, verifyProductArtifact } from "./product-artifact.mjs";
import { observedSourceRevision, writeRuntimeBuildIdentity } from "../../scripts/build-runtime-identity.mjs";

const repository = fileURLToPath(new URL("../../", import.meta.url));
const compiler = "rustc 1.98.0\nhost: x86_64-unknown-linux-gnu\nrelease: 1.98.0";
const bundle = `compat-bundle.${"a".repeat(64)}.json`;

async function put(root, name, content) {
  await mkdir(path.dirname(path.join(root, name)), { recursive: true });
  await writeFile(path.join(root, name), content);
}

async function fixture(t) {
  const workspace = await mkdtemp(path.join(os.tmpdir(), "noon-product-runtime-"));
  t.after(() => rm(workspace, { recursive: true, force: true }));
  const root = path.join(workspace, "candidate");
  await mkdir(root);
  const git = (...args) => execFileSync("git", args, {
    cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  // Use the repository's real ignore policy. Do not hide newly added bookkeeping.
  await put(root, ".gitignore", await readFile(path.join(repository, ".gitignore")));
  await put(root, "Cargo.toml", '[workspace]\nmembers = ["crates/noon-web"]\n');
  await put(root, "rust-toolchain.toml", '[toolchain]\nchannel = "1.98.0"\n');
  await put(root, "crates/noon-web/Cargo.toml", '[package]\nname = "noon-web"\nversion = "0.1.0"\n');
  await put(root, "scripts/build-web-demo.sh", "# unit fixture; no Rust build\n");
  await put(root, ".github/actions/dev-wasm-build/action.yml", "name: fixture\n");
  await put(root, "web/runtime-build-verifier.js", "// tracked fixture verifier\n");
  git("init", "-q", "-b", "base");
  git("config", "user.name", "CI Test");
  git("config", "user.email", "ci@example.invalid");
  git("add", ".");
  git("commit", "-qm", "event base");
  const eventBase = git("rev-parse", "HEAD");
  git("checkout", "-qb", "pr");
  await put(root, "pr.txt", "candidate source\n");
  git("add", ".");
  git("commit", "-qm", "PR head");
  const head = git("rev-parse", "HEAD");
  git("checkout", "-q", "base");
  await put(root, "base.txt", "intervening target source\n");
  git("add", ".");
  git("commit", "-qm", "advance target");
  const baseline = git("rev-parse", "HEAD");
  git("merge", "--no-ff", "-qm", "tested merge", "pr");
  const candidate = git("rev-parse", "HEAD");
  const sources = { schema: 1, candidate, baseline, head, eventBase };
  return { workspace, root, git, sources };
}

async function selectSources(f, root = f.root) {
  const workflow = await readFile(path.join(repository, ".github/workflows/playground-product-gate.yml"), "utf8");
  const beforeBuild = workflow.split("      - name: Build baseline production package")[0];
  const match = beforeBuild.match(/^\s*run: node candidate\/(\S+)([^\n]*)$/m);
  assert.ok(match, "workflow must select sources before building either package");
  const args = match[2].trim().split(/\s+/).map(arg => arg === "candidate" ? root : arg);
  // Real Actions file commands live outside the checkout.
  const runnerTemp = await mkdtemp(path.join(f.workspace, "runner-temp-"));
  const environment = path.join(runnerTemp, "env");
  execFileSync(process.execPath, [path.join(repository, match[1]), ...args], {
    env: { ...process.env, GITHUB_SHA: f.sources.candidate,
      NOON_PRODUCT_CANDIDATE_SHA: f.sources.candidate, NOON_PRODUCT_BASE_SHA: f.sources.baseline,
      NOON_PRODUCT_HEAD_SHA: f.sources.head, NOON_PRODUCT_EVENT_BASE_SHA: f.sources.eventBase,
      GITHUB_ENV: environment, GITHUB_OUTPUT: path.join(runnerTemp, "output") },
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.equal(observedSourceRevision(root), f.sources.candidate,
    "source selection must not erase runtime provenance by dirtying the checkout");
  return Object.fromEntries((await readFile(environment, "utf8")).trim().split("\n")
    .map(line => line.split("=")));
}

async function generatedPackage(root, role) {
  await put(root, "Cargo.lock", "version = 4\n");
  await put(root, "web/pkg/noon_web.js", "export default function init() {}\n");
  await put(root, "web/pkg/noon_web_bg.wasm", Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  await put(root, "web/pkg/package.json", '{"type":"module"}\n');
  await put(root, "web/python-worker.js", `// ${role} unit fixture\nfetch("./python/${bundle}");\n`);
  await put(root, `web/python/${bundle}`, '{"version":1,"modules":[]}\n');
}

for (const roles of [["baseline"], ["candidate-fixture", "candidate"]]) {
  test(`${roles.join(" then ")} retains the observed revision through source selection, prepare, move, stamp and verify`, async t => {
    const f = await fixture(t);
    const selected = await selectSources(f);
    assert.equal(selected.NOON_PRODUCT_BASE_SHA, f.sources.baseline);
    assert.equal(selected.NOON_PRODUCT_CANDIDATE_SHA, f.sources.candidate);
    assert.notEqual(selected.NOON_PRODUCT_BASE_SHA, f.sources.eventBase);
    const source = roles[0] === "baseline" ? f.sources.baseline : f.sources.candidate;
    f.git("checkout", "-q", "--detach", source);
    let previousBuild = null;
    for (const role of roles) {
      const env = { ...selected, NOON_WASM_PROFILE: "release", NOON_WASM_SKIP_OPT: "0",
        NOON_RENDERER_SMOKE: role === "candidate-fixture" ? "1" : "0" };
      await generatedPackage(f.root, role);
      const prepared = await prepareProductArtifact(f.root, env, compiler, role);
      await put(f.root, "ci-artifacts/product-build.json", JSON.stringify(prepared));
      const moved = path.join(f.workspace, ".product-source");
      await rename(f.root, moved);
      try {
        const runtime = await writeRuntimeBuildIdentity(moved);
        assert.equal(runtime.sourceRevision, source);
        const manifest = await stampProductArtifact(moved, prepared, env, role);
        assert.equal(manifest.source, runtime.sourceRevision);
        assert.deepEqual(manifest.productSources, f.sources);
        assert.deepEqual(await verifyProductArtifact(moved, env, role), manifest);
        assert.equal(observedSourceRevision(moved), source);
        // A second build must not lose its revision because of the first stamp.
        assert.deepEqual(await writeRuntimeBuildIdentity(moved), runtime);
        if (previousBuild) assert.notEqual(runtime.buildId, previousBuild);
        previousBuild = runtime.buildId;
      } finally {
        await rename(moved, f.root);
      }
    }
    assert.equal(f.git("status", "--porcelain", "--untracked-files=all"), "");
  });
}

test("untracked pre-build evidence makes provenance unavailable instead of inventing a revision", async t => {
  const f = await fixture(t);
  await selectSources(f);
  await generatedPackage(f.root, "candidate");
  const report = "browser-smoke-artifacts/product-gate/source-pair.json";
  await put(f.root, report, JSON.stringify(f.sources));
  assert.match(f.git("status", "--porcelain", "--untracked-files=all"), /source-pair\.json/);
  const dirty = await writeRuntimeBuildIdentity(f.root);
  assert.equal(dirty.sourceRevision, null);
  await rm(path.join(f.root, report));
  const clean = await writeRuntimeBuildIdentity(f.root);
  assert.equal(clean.sourceRevision, f.sources.candidate);
  assert.notEqual(clean.buildId, dirty.buildId);
});

for (const name of ["pr.txt", "web/python/untracked_module.py"]) {
  test(`actual dirty source still has no claimed revision: ${name}`, async t => {
    const f = await fixture(t);
    await selectSources(f);
    await generatedPackage(f.root, "candidate");
    await put(f.root, name, "changed source\n");
    assert.equal((await writeRuntimeBuildIdentity(f.root)).sourceRevision, null);
  });
}

for (const depth of [1, 2]) {
  test(`source-to-runtime provenance handles a real depth-${depth} checkout`, async t => {
    const f = await fixture(t);
    const shallow = path.join(f.workspace, "shallow");
    f.git("clone", "-q", "--depth", String(depth), pathToFileURL(f.root).href, shallow);
    if (depth === 1) {
      await assert.rejects(selectSources(f, shallow), error => /fetched parents/.test(String(error.stderr)));
    } else {
      await selectSources(f, shallow);
      await generatedPackage(shallow, "candidate");
      assert.equal((await writeRuntimeBuildIdentity(shallow)).sourceRevision, f.sources.candidate);
    }
  });
}

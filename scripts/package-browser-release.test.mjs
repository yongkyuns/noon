import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdtemp, mkdir, readFile, readdir, rm, symlink, utimes, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { optimizedConfig } from "../.github/ci/optimized-replay-artifact.mjs";
import { prepareArtifact, stamp } from "../.github/ci/wasm-build.mjs";
import { writeRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { packageBrowserRelease } from "./package-browser-release.mjs";
import { extractBrowserRelease, qualifyBrowserRelease, verifyBrowserRelease } from "./check-browser-release.mjs";

const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const longAsset = `web/python/examples/${"long_directory_".repeat(9)}/lesson.py`;

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-browser-candidate-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const put = async (name, value) => {
    await mkdir(path.dirname(path.join(root, name)), { recursive: true });
    await writeFile(path.join(root, name), value);
  };
  const git = (...args) => execFileSync("git", args, {
    cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  const source = {
    ".gitignore": "/web/pkg\n/web/python-worker.js\n/web/runtime-build-identity.json\n/web/ci-artifact.json\n/web/ci-Cargo.lock\n/web/python/compat-bundle.*.json\n/dist/\n__pycache__/\nnode_modules/\n*.pyc\n",
    "LICENSE": "MIT fixture license\n",
    "licenses/typst-assets-0.15.1/LICENSE": "Apache-2.0 fixture license\n",
    "licenses/typst-assets-0.15.1/NOTICE": "Individual bundled font fixture terms\n",
    "licenses/typst-assets-0.15.1/README.md": "Exact provider fixture provenance\n",
    "crates/noon/src/code_display/THIRD_PARTY_NOTICES.md": "Code highlighting fixture terms\n",
    "Cargo.toml": "[workspace]\nmembers = []\n",
    "Cargo.lock": "version = 4\n",
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.98.1"\n',
    "crates/noon-web/Cargo.toml": '[package]\nname = "noon-web"\nversion = "0.1.0"\n',
    "scripts/build-web-demo.sh": "#!/bin/sh\n",
    "scripts/playground-product-e2e.mjs": "console.error('fixture browser launch failure'); process.exit(23);\n",
    ".github/actions/dev-wasm-build/action.yml": "name: fixture\n",
    "web/index.html": '<script type="module" src="./main.js"></script>\n',
    "web/main.js": 'import "./execution-render-worker.js";\n',
    "web/execution-render-worker.js": "export const worker = true;\n",
    "web/authoring-render-worker.js": "export const authoring = true;\n",
    "web/python-editor-bootstrap.js": 'import "./python-editor.js";\n',
    "web/python-editor.js": 'import "./pkg/python-editor-bundle.js";\n',
    "web/python-compat-modules.js": "export const PYTHON_COMPAT_MODULES = [];\n",
    "web/python/noon.py": '__version__ = "0.1.0"\n',
    "web/python/examples/first_lesson.py": "class FirstLesson: pass\n",
    "web/python/examples/showcase_first_scene.py": "class FirstScene: pass\n",
    "web/runtime-build-verifier.js": "export const verifier = true;\n",
    "web/.nojekyll": "",
    [longAsset]: "# A long retained source path exercises deterministic PAX headers.\n",
  };
  for (const [name, bytes] of Object.entries(source)) await put(name, bytes);
  git("init", "-q");
  const commit = () => {
    git("add", ".");
    git("-c", "user.name=Noon Test", "-c", "user.email=noon@example.invalid", "commit", "-qm", "fixture");
  };
  commit();
  const contentHash = digest("fixture compatibility modules");
  for (const [name, bytes] of Object.entries({
    "web/python-worker.js": `const bundle = "./python/compat-bundle.${contentHash}.json";\n`,
    [`web/python/compat-bundle.${contentHash}.json`]: JSON.stringify({ contentHash, modules: [] }),
    "web/pkg/noon_web.js": "export default function init() {}\n",
    "web/pkg/noon_web.d.ts": "export default function init(): void;\n",
    "web/pkg/noon_web_bg.wasm": Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]),
    "web/pkg/package.json": '{"name":"noon-web","version":"0.1.0"}\n',
    "web/pkg/python-editor-bundle.js": "export const editor = true;\n",
    "web/pkg/python-editor-licenses.txt": "MIT editor fixture notice\n",
  })) await put(name, bytes);
  const rebuild = async () => {
    const identity = await writeRuntimeBuildIdentity(root);
    const build = optimizedConfig("production");
    await stamp(root, await prepareArtifact(root, {}, "release: 1.98.1\n", build), {}, build);
    return identity;
  };
  await rebuild();
  const pack = output => packageBrowserRelease({ root, version: "0.1.0-alpha.1", output, env: {} });
  return { root, put, git, commit, rebuild, pack, source };
}

test("complete browser candidate preserves identity, checks every payload, and is reproducible", async t => {
  const { root, put, pack } = await fixture(t);
  await put("web/python/__pycache__/noon.pyc", "excluded cache");
  await put("web/node_modules/unused/index.js", "excluded dependency cache");
  const identity = await readFile(path.join(root, "web/runtime-build-identity.json"));
  const first = await pack();
  const extract = path.join(root, "dist/extracted");
  await mkdir(extract);
  execFileSync("tar", ["-xzf", first.archive, "-C", extract]);
  const candidate = path.join(extract, path.basename(first.archive, ".tar.gz"));
  assert.equal(first.manifest.version, "0.1.0-alpha.1");
  assert.equal(first.manifest.buildId, JSON.parse(identity).buildId);
  assert.deepEqual(await readFile(path.join(candidate, "web/runtime-build-identity.json")), identity);
  for (const name of ["web/authoring-render-worker.js", "web/python/noon.py",
    "web/python/examples/first_lesson.py", "web/pkg/python-editor-bundle.js",
    "web/pkg/python-editor-licenses.txt", "web/.nojekyll", longAsset,
    "licenses/typst-assets-0.15.1/NOTICE", "licenses/typst-assets-0.15.1/LICENSE",
    "licenses/typst-assets-0.15.1/README.md", "crates/noon/src/code_display/THIRD_PARTY_NOTICES.md"]) {
    assert.ok(name in first.manifest.files, `complete tree omitted ${name}`);
  }
  assert.ok(!Object.keys(first.manifest.files).some(name => /node_modules|__pycache__/.test(name)));
  for (const [name, recorded] of Object.entries(first.manifest.files)) {
    const bytes = await readFile(path.join(candidate, name));
    assert.equal(bytes.length, recorded.bytes);
    assert.equal(digest(bytes), recorded.sha256);
  }
  for (const line of (await readFile(path.join(candidate, "SHA256SUMS"), "utf8")).trim().split("\n")) {
    const [hash, name] = line.split("  ");
    assert.equal(digest(await readFile(path.join(candidate, name))), hash);
  }
  assert.deepEqual(JSON.parse(await readFile(first.manifestPath, "utf8")), first.manifest);
  assert.equal(digest(await readFile(first.archive)), first.archiveSha256);
  assert.equal(await readFile(first.checksum, "utf8"), `${first.archiveSha256}  ${path.basename(first.archive)}\n`);

  await utimes(path.join(root, "web/index.html"), 12345, 12345);
  await utimes(path.join(root, "web/pkg/noon_web_bg.wasm"), 54321, 54321);
  const second = await pack(path.join(root, "dist/second"));
  assert.deepEqual(await readFile(second.archive), await readFile(first.archive));
  assert.equal((await pack()).archiveSha256, first.archiveSha256, "same candidate is safely reusable");
  await writeFile(first.archive, "different published bytes");
  await assert.rejects(pack(), /refusing to overwrite a different release artifact/);
  assert.equal(await readFile(first.archive, "utf8"), "different published bytes");
});

test("source drift, stale runtime hashes and a forged buildId cannot become a candidate", async t => {
  const { root, put, pack, source } = await fixture(t);
  await put("web/main.js", "changed source\n");
  await assert.rejects(pack(), /modified|clean source/);
  await put("web/main.js", source["web/main.js"]);
  const worker = await readFile(path.join(root, "web/python-worker.js"), "utf8");
  await put("web/python-worker.js", `${worker}// stale worker\n`);
  await assert.rejects(pack(), /artifact inventory mismatch|artifact content mismatch/);
  // Restore the exact original worker before independently corrupting identity.
  await put("web/python-worker.js", worker);
  const identity = JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8"));
  identity.buildId = "0".repeat(64);
  await put("web/runtime-build-identity.json", JSON.stringify(identity));
  await assert.rejects(pack(), /digest does not match/);
});

test("dev metadata and replay exports are rejected even with internally consistent hashes", async t => {
  const { root, put, pack, rebuild } = await fixture(t);
  const provenance = JSON.parse(await readFile(path.join(root, "web/ci-artifact.json"), "utf8"));
  provenance.build.profile = "dev";
  await put("web/ci-artifact.json", JSON.stringify(provenance));
  await assert.rejects(pack(), /configuration mismatch/);
  await put("web/pkg/noon_web.js", "export function verifyDirectExecutionReplay() {}\n");
  await rebuild();
  await assert.rejects(pack(), /replay fixture export must be absent/);
});

test("missing static editor input is rejected beyond the generated WASM package gate", async t => {
  const { root, commit, rebuild, pack } = await fixture(t);
  await rm(path.join(root, "web/python-editor.js"));
  commit();
  await rebuild();
  await assert.rejects(pack(), /missing browser release asset: web\/python-editor\.js/);
});

test("a tracked link cannot pull outside files into the complete site bundle", async t => {
  const { root, commit, rebuild, pack } = await fixture(t);
  await symlink("../LICENSE", path.join(root, "web/link.txt"));
  commit();
  await rebuild();
  await assert.rejects(pack(), /symlink in browser release/);
});

test("version and output paths cannot change the archive layout", async t => {
  const { root } = await fixture(t);
  await assert.rejects(packageBrowserRelease({ root, version: "../../escape" }), /explicit SemVer/);
  await assert.rejects(packageBrowserRelease({ root, version: "0.1.0\n" }), /explicit SemVer/);
  await assert.rejects(packageBrowserRelease({ root, version: "0.1.0", output: path.join(root, "web/releases") }), /outside web/);
});

async function extractedFixture(t) {
  const original = await fixture(t);
  const candidate = await original.pack();
  const directory = await mkdtemp(path.join(os.tmpdir(), "noon-browser-release-consumer-test-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const extracted = await extractBrowserRelease(candidate.archive, directory);
  return { ...original, candidate, directory, extracted,
    verify: () => verifyBrowserRelease(extracted.root, { sourceRoot: original.root, env: {} }) };
}

async function refreshReleaseChecksums(root, edit = () => {}) {
  const manifest = JSON.parse(await readFile(path.join(root, "release-manifest.json"), "utf8"));
  edit(manifest);
  for (const name of Object.keys(manifest.files)) {
    const bytes = await readFile(path.join(root, name));
    manifest.files[name] = { sha256: digest(bytes), bytes: bytes.length };
  }
  const bytes = `${JSON.stringify(manifest, null, 2)}\n`;
  await writeFile(path.join(root, "release-manifest.json"), bytes);
  const sums = { ...manifest.files, "release-manifest.json": { sha256: digest(bytes) } };
  await writeFile(path.join(root, "SHA256SUMS"), Object.keys(sums).sort()
    .map(name => `${sums[name].sha256}  ${name}\n`).join(""));
}

test("release consumer verifies the actual extracted production archive outside the checkout", async t => {
  const { root, candidate, extracted, verify } = await extractedFixture(t);
  assert.ok(!extracted.root.startsWith(`${root}${path.sep}`));
  assert.equal(extracted.archiveSha256, candidate.archiveSha256);
  const report = await verify();
  assert.equal(report.sourceRevision, candidate.manifest.sourceRevision);
  assert.equal(report.buildId, candidate.manifest.buildId);
  assert.equal(report.payloads, Object.keys(candidate.manifest.files).length);
  assert.equal(report.files, report.payloads + 2);
  assert.equal(report.runtimePayloads, 4);
  assert.equal(report.fixture.path, "python/examples/showcase_first_scene.py");
  assert.deepEqual(await verify(), report, "read-only qualification must leave the complete candidate unchanged");
});

test("release consumer rejects changed, missing and extra static payloads", async t => {
  const { extracted, verify } = await extractedFixture(t);
  const file = path.join(extracted.root, "web/main.js");
  const original = await readFile(file);
  await writeFile(file, "broken static entrypoint\n");
  await assert.rejects(verify(), /length mismatch|checksum mismatch/);
  await rm(file);
  await assert.rejects(verify(), /file inventory differs/);
  await writeFile(file, original);
  await writeFile(path.join(extracted.root, "web/unlisted.js"), "unlisted payload\n");
  await assert.rejects(verify(), /file inventory differs/);
});

test("release consumer rejects altered checksum inventories and symlinks", async t => {
  const { extracted, verify } = await extractedFixture(t);
  const sums = path.join(extracted.root, "SHA256SUMS");
  const original = await readFile(sums, "utf8");
  await writeFile(sums, original + original.split("\n")[0] + "\n");
  await assert.rejects(verify(), /checksum inventory differs/);
  await writeFile(sums, original);
  const file = path.join(extracted.root, "web/main.js");
  await rm(file);
  await symlink("../LICENSE", file);
  await assert.rejects(verify(), /symlink in extracted candidate/);
});

test("consistent release hashes cannot relabel a development package as optimized production", async t => {
  const { extracted, verify } = await extractedFixture(t);
  const file = path.join(extracted.root, "web/ci-artifact.json");
  const provenance = JSON.parse(await readFile(file, "utf8"));
  provenance.build.profile = "dev";
  await writeFile(file, JSON.stringify(provenance));
  await refreshReleaseChecksums(extracted.root);
  await assert.rejects(verify(), /optimized production configuration/);
});

test("consistent release hashes cannot forge the canonical runtime identity", async t => {
  const { extracted, verify } = await extractedFixture(t);
  const file = path.join(extracted.root, "web/runtime-build-identity.json");
  const identity = JSON.parse(await readFile(file, "utf8"));
  identity.buildId = "0".repeat(64);
  await writeFile(file, JSON.stringify(identity));
  await refreshReleaseChecksums(extracted.root, manifest => { manifest.buildId = identity.buildId; });
  await assert.rejects(verify(), /digest does not match/);
});

test("release consumer requires the exact clean source and packaged authored fixture", async t => {
  const { root, put, source, extracted, verify } = await extractedFixture(t);
  await assert.rejects(verifyBrowserRelease(extracted.root, { sourceRoot: root,
    env: { GITHUB_SHA: "0".repeat(40) } }), /checkout differs from event commit/);
  await put("web/main.js", "uncommitted source\n");
  await assert.rejects(verify(), /clean source checkout/);
  await put("web/main.js", source["web/main.js"]);
  await writeFile(path.join(extracted.root, "web/python/examples/showcase_first_scene.py"), "substituted fixture\n");
  await refreshReleaseChecksums(extracted.root);
  await assert.rejects(verify(), /product fixture differs from the packaged source/);
});

test("release extraction rejects an archive digest or manifest sidecar mismatch", async t => {
  const { candidate, directory } = await extractedFixture(t);
  const empty = path.join(directory, "next");
  await mkdir(empty);
  await writeFile(candidate.checksum, `${"0".repeat(64)}  ${path.basename(candidate.archive)}\n`);
  await assert.rejects(extractBrowserRelease(candidate.archive, empty), /archive checksum mismatch/);
  await writeFile(candidate.checksum, `${candidate.archiveSha256}  ${path.basename(candidate.archive)}\n`);
  await writeFile(candidate.manifestPath, "{}\n");
  await assert.rejects(extractBrowserRelease(candidate.archive, empty), /manifest differs from producer sidecar/);
});

test("a failed product harness retains its evidence and cannot qualify an unchanged archive", async t => {
  const { root, pack } = await fixture(t);
  const candidate = await pack();
  const output = path.join(root, "dist/failed-consumer");
  const report = await qualifyBrowserRelease({ archive: candidate.archive, output, sourceRoot: root, env: {} });
  assert.equal(report.status, "failed");
  assert.equal(report.browserExecution.exitCode, 23);
  assert.deepEqual(report.after, report.before, "failed browser setup must still reverify every candidate byte");
  assert.match(await readFile(path.join(output, "product.stderr.log"), "utf8"), /fixture browser launch failure/);
  assert.deepEqual(JSON.parse(await readFile(path.join(output, "report.json"), "utf8")), report);
  await assert.rejects(lstat(report.extractedRoot), { code: "ENOENT" });
});

for (const failure of ["traversal", "symlink", "hardlink", "duplicate"]) {
  test(`release extraction rejects ${failure} before writing any payload`, async t => {
    const directory = await mkdtemp(path.join(os.tmpdir(), "noon-browser-bad-archive-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    const label = `noon-browser-0.1.0-${"1".repeat(12)}`;
    const archive = path.join(directory, `${label}.tar.gz`);
    const empty = path.join(directory, "extracted");
    await mkdir(empty);
    execFileSync("python3", ["-c", String.raw`
import io, sys, tarfile
archive, label, failure = sys.argv[1:]
with tarfile.open(archive, "w:gz") as output:
    first = tarfile.TarInfo(label + "/safe.txt")
    first.size = 4
    output.addfile(first, io.BytesIO(b"safe"))
    bad = tarfile.TarInfo(label + ("/../escaped.txt" if failure == "traversal" else "/safe.txt" if failure == "duplicate" else "/link"))
    if failure in ("symlink", "hardlink"):
        bad.type = tarfile.SYMTYPE if failure == "symlink" else tarfile.LNKTYPE
        bad.linkname = "../escaped.txt"
    output.addfile(bad)
`, archive, label, failure]);
    await writeFile(`${archive}.sha256`, `${digest(await readFile(archive))}  ${path.basename(archive)}\n`);
    const optimization = process.env.PYTHONOPTIMIZE;
    try {
      // Caller settings must not compile away the extraction assertions.
      if (failure === "traversal") process.env.PYTHONOPTIMIZE = "2";
      await assert.rejects(extractBrowserRelease(archive, empty), /unsafe archive path|links and special entries|duplicate archive path/);
    } finally {
      if (optimization === undefined) delete process.env.PYTHONOPTIMIZE;
      else process.env.PYTHONOPTIMIZE = optimization;
    }
    assert.deepEqual(await readdir(empty), [], "invalid headers must fail before extraction begins");
  });
}

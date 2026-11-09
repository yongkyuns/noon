import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, symlink, utimes, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { optimizedConfig } from "../.github/ci/optimized-replay-artifact.mjs";
import { prepareArtifact, stamp } from "../.github/ci/wasm-build.mjs";
import { writeRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { packageBrowserRelease } from "./package-browser-release.mjs";

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

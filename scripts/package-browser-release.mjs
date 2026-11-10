// Produce a versioned static-site candidate from the already qualified optimized
// production build. This command never builds, deploys, or publishes anything.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { constants, createReadStream, createWriteStream } from "node:fs";
import { copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";
import { createGzip } from "node:zlib";

import { optimizedConfig, assertReplaySurface } from "../.github/ci/optimized-replay-artifact.mjs";
import { fileDigest, verify } from "../.github/ci/wasm-build.mjs";
import { validateRuntimeBuildIdentity } from "../web/runtime-build-verifier.js";
import { observedSourceRevision } from "./build-runtime-identity.mjs";

const noticeFiles = [
  "LICENSE", "licenses/typst-assets-0.15.1/LICENSE", "licenses/typst-assets-0.15.1/NOTICE",
  "licenses/typst-assets-0.15.1/README.md", "crates/noon/src/code_display/THIRD_PARTY_NOTICES.md",
];
const requiredFiles = [
  ...noticeFiles, "web/index.html", "web/main.js", "web/python-editor-bootstrap.js",
  "web/python-editor.js", "web/python-compat-modules.js", "web/python/noon.py",
  "web/python/examples/first_lesson.py", "web/python-worker.js",
  "web/runtime-build-identity.json", "web/runtime-build-verifier.js",
  "web/pkg/noon_web.js", "web/pkg/noon_web.d.ts", "web/pkg/noon_web_bg.wasm",
  "web/pkg/package.json", "web/pkg/python-editor-bundle.js", "web/pkg/python-editor-licenses.txt",
  "web/ci-artifact.json", "web/ci-Cargo.lock",
];
const omittedDirectories = new Set([".git", "node_modules", "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"]);
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => `${JSON.stringify(value, null, 2)}\n`;

function command(root, program, args) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, `${program} failed: ${result.stderr || result.error}`);
  return result.stdout;
}

async function inventory(root) {
  const names = [...noticeFiles];
  async function visit(name) {
    assert.ok(!/[\x00-\x1f\x7f\\]/.test(name), `unsupported browser asset path: ${name}`);
    const info = await lstat(path.join(root, name));
    assert.ok(!info.isSymbolicLink(), `symlink in browser release: ${name}`);
    if (info.isDirectory()) {
      for (const child of (await readdir(path.join(root, name))).sort()) {
        if (omittedDirectories.has(child) || child.endsWith(".egg-info") ||
            child === ".DS_Store" || child === ".gitignore" || /\.py[co]$/.test(child)) continue;
        await visit(`${name}/${child}`);
      }
    } else {
      assert.ok(info.isFile(), `unsupported browser release entry: ${name}`);
      names.push(name);
    }
  }
  await visit("web");
  for (const name of requiredFiles) {
    assert.ok(names.includes(name), `missing browser release asset: ${name}`);
    const info = await lstat(path.join(root, name));
    assert.ok(info.isFile() && !info.isSymbolicLink() && info.size > 0,
      `browser release asset must be a nonempty regular file: ${name}`);
  }
  return names.sort();
}

async function writeOnce(filename, bytes) {
  try { await writeFile(filename, bytes, { flag: "wx" }); }
  catch (error) {
    if (error.code !== "EEXIST") throw error;
    assert.deepEqual(await readFile(filename), Buffer.from(bytes), `refusing to overwrite a different release artifact: ${filename}`);
  }
}

export async function packageBrowserRelease({ root = process.cwd(), version, output, env = process.env }) {
  assert.ok(typeof version === "string" && version === version.trim() && version.length <= 100 &&
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(version),
  "version must be an explicit SemVer candidate such as 0.1.0-alpha.1");
  root = path.resolve(root);
  output = path.resolve(output ?? path.join(root, "dist/browser"));
  const web = path.join(root, "web");
  assert.ok(output !== web && !output.startsWith(`${web}${path.sep}`), "release output must be outside web");

  // Reuse source/configuration/lockfile/package attestation, including the exact
  // production profile. A dev or replay package cannot be relabeled as a release.
  const provenance = await verify(root, env, optimizedConfig("production"));
  assert.equal(observedSourceRevision(root), provenance.source, "browser release requires a clean source checkout");
  const provenanceBytes = await readFile(path.join(root, "web/ci-artifact.json"));
  assert.deepEqual(JSON.parse(provenanceBytes), provenance, "build provenance changed while packaging");
  const identityBytes = await readFile(path.join(root, "web/runtime-build-identity.json"));
  const identity = await validateRuntimeBuildIdentity(JSON.parse(identityBytes));
  assert.equal(identity.sourceRevision, provenance.source, "runtime identity is not from this source checkout");
  for (const descriptor of Object.values(identity.files)) {
    const name = `web/${descriptor.path.slice(2)}`;
    assert.equal(await fileDigest(root, name), descriptor.sha256, `runtime identity mismatch: ${name}`);
  }
  assertReplaySurface(
    await readFile(path.join(root, "web/pkg/noon_web.js"), "utf8"),
    await readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8"), "production");

  const names = await inventory(root);
  const label = `noon-browser-${version}-${provenance.source.slice(0, 12)}`;
  const temporary = await mkdtemp(path.join(os.tmpdir(), "noon-browser-release-"));
  const staged = path.join(temporary, label);
  try {
    const files = {};
    for (const name of names) {
      const bytes = await readFile(path.join(root, name));
      files[name] = { sha256: digest(bytes), bytes: bytes.length };
      const destination = path.join(staged, name);
      await mkdir(path.dirname(destination), { recursive: true });
      await writeFile(destination, bytes);
    }
    // Validate the copied bytes too, before constructing the final immutable
    // candidate. Never rewrite the runtime identity to bless a different build.
    for (const [name, expected] of Object.entries(provenance.files)) {
      assert.equal(files[name]?.sha256, expected, `build artifact changed while packaging: ${name}`);
    }
    for (const descriptor of Object.values(identity.files)) {
      const name = `web/${descriptor.path.slice(2)}`;
      assert.equal(files[name]?.sha256, descriptor.sha256, `runtime changed while packaging: ${name}`);
    }
    assert.equal(files["web/ci-artifact.json"].sha256, digest(provenanceBytes), "build provenance changed while packaging");
    assert.equal(files["web/runtime-build-identity.json"].sha256, digest(identityBytes), "runtime identity changed while packaging");
    assert.equal(observedSourceRevision(root), provenance.source, "source changed while packaging");
    const manifest = {
      schema: 1, version, sourceRevision: provenance.source, buildId: identity.buildId,
      entrypoint: "web/index.html", serveDirectory: "web",
      runtimeIdentity: "web/runtime-build-identity.json",
      buildProvenance: "web/ci-artifact.json", files,
    };
    const manifestBytes = json(manifest);
    await writeFile(path.join(staged, "release-manifest.json"), manifestBytes);
    const checksums = { ...files, "release-manifest.json": { sha256: digest(manifestBytes) } };
    await writeFile(path.join(staged, "SHA256SUMS"), Object.keys(checksums).sort()
      .map(name => `${checksums[name].sha256}  ${name}\n`).join(""));

    // GNU tar is available in the Linux release job. Normalize order, ownership,
    // modes and all timestamps; fixed PAX names avoid embedding the archiver PID.
    assert.match(command(root, "tar", ["--version"]), /GNU tar/, "browser release packaging requires GNU tar");
    const tar = path.join(temporary, "browser.tar");
    command(root, "tar", ["--create", "--file", tar, "--format=posix", "--sort=name",
      "--mtime=@0", "--owner=0", "--group=0", "--numeric-owner", "--mode=u=rwX,go=rX",
      "--pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime", "--directory", temporary, label]);
    const compressed = path.join(temporary, `${label}.tar.gz`);
    await pipeline(createReadStream(tar), createGzip({ level: 9 }), createWriteStream(compressed));
    const archiveSha256 = await fileDigest(temporary, `${label}.tar.gz`);
    await mkdir(output, { recursive: true });
    const archive = path.join(output, `${label}.tar.gz`);
    try { await copyFile(compressed, archive, constants.COPYFILE_EXCL); }
    catch (error) {
      if (error.code !== "EEXIST") throw error;
      assert.equal(await fileDigest(output, `${label}.tar.gz`), archiveSha256,
        `refusing to overwrite a different release artifact: ${archive}`);
    }
    const checksum = `${archive}.sha256`;
    const manifestPath = path.join(output, `${label}.manifest.json`);
    await writeOnce(checksum, `${archiveSha256}  ${path.basename(archive)}\n`);
    await writeOnce(manifestPath, manifestBytes);
    return { archive, checksum, manifestPath, archiveSha256, manifest };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function main() {
  const options = {};
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    assert.ok(["--version", "--output", "--root"].includes(key) && args[index + 1],
      "usage: node scripts/package-browser-release.mjs --version VERSION [--output DIR] [--root CHECKOUT]");
    assert.ok(!(key.slice(2) in options), `duplicate option: ${key}`);
    options[key.slice(2)] = args[index + 1];
  }
  const result = await packageBrowserRelease(options);
  console.log(json({ archive: result.archive, checksum: result.checksum, manifest: result.manifestPath,
    version: result.manifest.version, sourceRevision: result.manifest.sourceRevision,
    buildId: result.manifest.buildId, archiveSha256: result.archiveSha256 }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}

// Qualify the shipped production archive, without rebuilding or modifying it.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { assertReplaySurface, optimizedConfig } from "../.github/ci/optimized-replay-artifact.mjs";
import { configuration, fileDigest, packageSizes, sourceSha } from "../.github/ci/wasm-build.mjs";
import { validateRuntimeBuildIdentity } from "../web/runtime-build-verifier.js";
import { observedSourceRevision } from "./build-runtime-identity.mjs";
import { productMeasurement } from "./playground-product-fps.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exampleId = "showcase-first-scene";
const fixturePath = `web/${productMeasurement(exampleId).sourcePath}`;
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const record = value => value !== null && typeof value === "object" && !Array.isArray(value);

function relativePath(name) {
  assert.ok(typeof name === "string" && name.length > 0 && !/[\x00-\x1f\x7f\\]/.test(name) &&
    name.split("/").every(part => part !== "" && part !== "." && part !== ".."),
  `invalid release path: ${name}`);
  return name;
}

async function fileInventory(root) {
  const files = [];
  async function visit(name) {
    const entry = await lstat(path.join(root, name));
    assert.ok(!entry.isSymbolicLink(), `symlink in extracted candidate: ${name}`);
    if (entry.isDirectory()) {
      for (const child of (await readdir(path.join(root, name))).sort()) {
        await visit(relativePath(name ? `${name}/${child}` : child));
      }
    } else {
      assert.ok(entry.isFile(), `non-regular release payload: ${name}`);
      files.push(name);
    }
  }
  await visit("");
  return files.sort();
}

// Validate all headers before writing any payload. Links, duplicate names and
// traversal are not part of the producer's static-file archive contract.
export async function extractBrowserRelease(archive, destination) {
  assert.match(archive, /\.tar\.gz$/, "expected a browser .tar.gz candidate");
  const archiveSha256 = await fileDigest(path.dirname(archive), path.basename(archive));
  assert.equal(await readFile(`${archive}.sha256`, "utf8"),
    `${archiveSha256}  ${path.basename(archive)}\n`, "browser archive checksum mismatch");
  const label = path.basename(archive, ".tar.gz");
  const result = spawnSync("python3", ["-I", "-c", String.raw`
import pathlib, shutil, sys, tarfile
archive, destination, label = sys.argv[1:]
target = pathlib.Path(destination)
assert target.is_dir() and not any(target.iterdir()), "extraction destination must be empty"
with tarfile.open(archive, "r:gz") as source:
    members = source.getmembers()
    seen = set()
    for item in members:
        name = item.name.rstrip("/")
        parts = name.split("/")
        assert name and all(p not in ("", ".", "..") for p in parts), "unsafe archive path"
        assert "\\" not in name and all(ord(c) >= 32 and ord(c) != 127 for c in name), "unsafe archive path"
        assert parts[0] == label, "archive root differs from candidate filename"
        assert name not in seen, "duplicate archive path"
        seen.add(name)
        assert item.isdir() or item.isfile(), "archive links and special entries are forbidden"
    for item in members:
        output = target.joinpath(*item.name.rstrip("/").split("/"))
        if item.isdir():
            output.mkdir(parents=True, exist_ok=True)
        else:
            output.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(item) as incoming, output.open("xb") as outgoing:
                shutil.copyfileobj(incoming, outgoing)
`, archive, destination, label], { encoding: "utf8", timeout: 60_000 });
  assert.equal(result.status, 0, `browser archive extraction failed: ${result.stderr || result.error}`);
  const root = path.join(destination, label);
  assert.deepEqual(await readFile(path.join(root, "release-manifest.json")),
    await readFile(archive.slice(0, -7) + ".manifest.json"), "archived manifest differs from producer sidecar");
  return { root, archiveSha256, archiveBytes: (await lstat(archive)).size };
}

export async function verifyBrowserRelease(root, { sourceRoot = repoRoot, env = process.env } = {}) {
  const source = sourceSha(sourceRoot, env);
  assert.equal(observedSourceRevision(sourceRoot), source, "release consumer requires a clean source checkout");
  const bytes = await readFile(path.join(root, "release-manifest.json"));
  const manifest = JSON.parse(bytes);
  assert.ok(record(manifest) && manifest.schema === 1 && record(manifest.files), "invalid release manifest");
  assert.deepEqual(Object.keys(manifest).sort(), ["schema", "version", "sourceRevision", "buildId", "entrypoint",
    "serveDirectory", "runtimeIdentity", "buildProvenance", "files"].sort(), "unexpected release manifest envelope");
  assert.equal(manifest.sourceRevision, source, "release candidate is not from this checkout");
  assert.ok(typeof manifest.version === "string" && /^[0-9A-Za-z.+-]+$/.test(manifest.version), "invalid release version");
  assert.equal(path.basename(root), `noon-browser-${manifest.version}-${source.slice(0, 12)}`, "release label mismatch");
  assert.equal(manifest.entrypoint, "web/index.html");
  assert.equal(manifest.serveDirectory, "web");
  assert.equal(manifest.runtimeIdentity, "web/runtime-build-identity.json");
  assert.equal(manifest.buildProvenance, "web/ci-artifact.json");
  const names = Object.keys(manifest.files).map(relativePath).sort();
  assert.ok(names.length > 0 && names.includes(manifest.entrypoint) && names.includes(fixturePath),
    "release entrypoint or product fixture is missing");
  assert.deepEqual(await fileInventory(root), [...names, "release-manifest.json", "SHA256SUMS"].sort(),
    "extracted release file inventory differs");
  for (const name of names) {
    const expected = manifest.files[name];
    assert.ok(record(expected) && Object.keys(expected).sort().join(",") === "bytes,sha256" &&
      /^[0-9a-f]{64}$/.test(expected.sha256) && Number.isSafeInteger(expected.bytes) && expected.bytes >= 0,
    `invalid release file descriptor: ${name}`);
    assert.equal((await lstat(path.join(root, name))).size, expected.bytes, `release file length mismatch: ${name}`);
    assert.equal(await fileDigest(root, name), expected.sha256, `release payload checksum mismatch: ${name}`);
  }
  const sums = { ...manifest.files, "release-manifest.json": { sha256: digest(bytes) } };
  assert.equal(await readFile(path.join(root, "SHA256SUMS"), "utf8"),
    Object.keys(sums).sort().map(name => `${sums[name].sha256}  ${name}\n`).join(""),
    "release checksum inventory differs");

  const provenance = JSON.parse(await readFile(path.join(root, manifest.buildProvenance), "utf8"));
  assert.equal(provenance.source, source, "build provenance source differs");
  assert.deepEqual(provenance.build, await configuration(sourceRoot, optimizedConfig("production")),
    "release is not the expected optimized production configuration");
  assert.match(provenance.compiler, new RegExp(`^release: ${provenance.build.toolchain.replaceAll(".", "\\.")}$`, "m"));
  assert.equal(provenance.dependencyLock, await fileDigest(sourceRoot, "Cargo.lock"), "reviewed dependency lock differs");
  // This existing verifier derives the generated-package inventory itself and
  // checks every retained package/worker/bundle byte against build provenance.
  const sizes = await packageSizes(root);
  assert.equal(provenance.files["web/ci-Cargo.lock"], provenance.dependencyLock, "packaged dependency lock differs");
  assertReplaySurface(await readFile(path.join(root, "web/pkg/noon_web.js"), "utf8"),
    await readFile(path.join(root, "web/pkg/noon_web.d.ts"), "utf8"), "production");
  const identity = await validateRuntimeBuildIdentity(JSON.parse(await readFile(path.join(root, manifest.runtimeIdentity), "utf8")));
  assert.equal(identity.sourceRevision, source);
  assert.equal(identity.buildId, manifest.buildId);
  for (const descriptor of Object.values(identity.files)) {
    assert.equal(await fileDigest(root, `web/${descriptor.path.slice(2)}`), descriptor.sha256, "runtime payload differs");
  }
  // The existing product harness reads its authored fixture from the checkout.
  // Require those exact bytes to be present in this archive before using it.
  assert.equal(await fileDigest(sourceRoot, fixturePath), manifest.files[fixturePath].sha256,
    "product fixture differs from the packaged source");
  return { sourceRevision: source, version: manifest.version, buildId: identity.buildId,
    manifestSha256: digest(bytes), files: names.length + 2, payloads: names.length,
    provenancePayloads: Object.keys(provenance.files).length, runtimePayloads: Object.keys(identity.files).length,
    fixture: { path: fixturePath.slice(4), sha256: manifest.files[fixturePath].sha256 },
    runtimeIdentity: identity, packageSizes: sizes };
}

export async function qualifyBrowserRelease({ archive, output, sourceRoot = repoRoot, env = process.env }) {
  archive = path.resolve(archive);
  output = path.resolve(output);
  await mkdir(output, { recursive: true });
  assert.equal((await readdir(output)).length, 0, "browser qualification output must be empty");
  const temporary = await mkdtemp(path.join(os.tmpdir(), "noon-browser-consumer-"));
  const report = { schema: 1, status: "running", archive, output, browserExecution: "not-run",
    physicalDeviceQualified: false, node: process.version, startedAt: new Date().toISOString() };
  try {
    assert.ok(!temporary.startsWith(`${path.resolve(sourceRoot)}${path.sep}`), "release extraction is inside checkout");
    const extracted = await extractBrowserRelease(archive, temporary);
    report.archiveSha256 = extracted.archiveSha256;
    report.archiveBytes = extracted.archiveBytes;
    report.extractedRoot = extracted.root;
    report.before = await verifyBrowserRelease(extracted.root, { sourceRoot, env });
    const harness = path.join(sourceRoot, "scripts/playground-product-e2e.mjs");
    report.harnessSha256 = digest(await readFile(harness));
    report.command = [process.execPath, harness];
    const started = performance.now();
    const processResult = spawnSync(process.execPath, [harness], { cwd: sourceRoot, encoding: "utf8",
      detached: true, timeout: 240_000, maxBuffer: 16 * 1024 * 1024,
      env: { ...env, NOON_PRODUCT_SITE_ROOT: extracted.root, NOON_PRODUCT_LABEL: "candidate",
        NOON_PRODUCT_EXAMPLE: exampleId, NOON_PRODUCT_PORT: "4205", NOON_PRODUCT_ARTIFACT_DIR: path.join(output, "product") } });
    // The harness normally closes its browser and HTTP server. Retire this
    // owned process group too if its bounded invocation failed or timed out.
    if (processResult.pid) {
      try { process.kill(-processResult.pid, "SIGKILL"); }
      catch (error) { if (error.code !== "ESRCH") throw error; }
    }
    await writeFile(path.join(output, "product.stdout.log"), processResult.stdout ?? "");
    await writeFile(path.join(output, "product.stderr.log"), processResult.stderr ?? "");
    report.browserExecution = { exitCode: processResult.status, signal: processResult.signal,
      error: processResult.error?.message ?? null, seconds: (performance.now() - started) / 1000 };
    // Check after failures as well; browser execution must not alter the bytes
    // which will later be deployed or archived as this candidate.
    report.after = await verifyBrowserRelease(extracted.root, { sourceRoot, env });
    assert.deepEqual(report.after, report.before, "browser execution changed the release candidate");
    assert.equal(processResult.status, 0, `production browser qualification failed: ${processResult.error ?? processResult.stderr}`);
    const product = JSON.parse(await readFile(path.join(output, "product/report.json"), "utf8"));
    assert.equal(product.siteRoot, extracted.root);
    assert.equal(product.exampleId, exampleId);
    assert.deepEqual(product.runtimeIdentity, report.before.runtimeIdentity, "browser report used another runtime identity");
    assert.deepEqual(product.source, report.before.fixture, "browser report used another authored fixture");
    assert.deepEqual(product.pageErrors, []);
    assert.deepEqual(product.consoleErrors, []);
    report.productReport = "product/report.json";
    report.productReportSha256 = digest(await readFile(path.join(output, report.productReport)));
    report.status = "passed";
  } catch (error) {
    report.status = "failed";
    report.error = error?.stack ?? String(error);
  } finally {
    report.finishedAt = new Date().toISOString();
    await writeFile(path.join(output, "report.json"), json(report));
    await rm(temporary, { recursive: true, force: true });
  }
  return report;
}

async function main() {
  const args = process.argv.slice(2);
  assert.ok(args.length === 4 && args[0] === "--archive" && args[2] === "--output",
    "usage: node scripts/check-browser-release.mjs --archive CANDIDATE.tar.gz --output EMPTY_DIRECTORY");
  const result = await qualifyBrowserRelease({ archive: args[1], output: args[3] });
  console.log(json(result));
  if (result.status !== "passed") process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { lstat, mkdir, readFile, readdir, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const SOURCE_SHA = "60e5708b4a235f68c8b0788fd57fb219fdc4a2949aa61573d1aa0cef267ff84b";
const EXPECTED_BROWSER = "151.0.7922.34";
const VARIANTS = {
  anchor: { checkout: "anchor-source", source: "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5", role: "baseline" },
  d7: { checkout: "d7-source", source: "f3def9bc39afe95f5ba2207d906714bae15d60f7", role: "candidate" },
  current: { checkout: "current-source", source: "be814def71f9d34d9d28d106631544ca5a8eb3f8", role: "candidate" },
};
const ORDERS = [
  ["anchor", "d7", "current"],
  ["d7", "current", "anchor"],
  ["current", "anchor", "d7"],
];
const PRODUCER_VERIFIERS = {
  anchor: { checkout: "current-source", source: "be814def71f9d34d9d28d106631544ca5a8eb3f8" },
  current: { checkout: "current-source", source: "be814def71f9d34d9d28d106631544ca5a8eb3f8" },
  d7: { checkout: "d7-source", source: "f3def9bc39afe95f5ba2207d906714bae15d60f7" },
};
const root = path.resolve(process.env.GITHUB_WORKSPACE);
const studyRoot = path.resolve(process.env.STUDY_ROOT);
const harness = path.join(root, "harness");
const e2ePath = path.join(harness, "scripts/playground-product-e2e.mjs");
const preparedPath = path.join(studyRoot, "prepared-packages.json");
const studyPath = path.join(studyRoot, "study.json");
const pins = JSON.parse(await readFile(preparedPath, "utf8"));

function sha256(data) {
  return createHash("sha256").update(data).digest("hex");
}

async function assertRegular(file) {
  const info = await lstat(file);
  assert.ok(info.isFile() && !info.isSymbolicLink(), `expected regular file: ${file}`);
  return info;
}

async function assertDirectory(dir) {
  const info = await lstat(dir);
  assert.ok(info.isDirectory() && !info.isSymbolicLink(), `expected real directory: ${dir}`);
  return info;
}

async function ensureDirectory(dir) {
  try {
    await lstat(dir);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    await mkdir(dir);
  }
  await assertDirectory(dir);
}

async function assertNoSymlinks(dir) {
  await assertDirectory(dir);
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const child = path.join(dir, entry.name);
    const info = await lstat(child);
    assert.ok(!info.isSymbolicLink(), `symlink in study output: ${child}`);
    if (info.isDirectory()) await assertNoSymlinks(child);
    else assert.ok(info.isFile(), `non-regular study output: ${child}`);
  }
}

function git(rootDir, ...args) {
  const result = spawnSync("git", args, { cwd: rootDir, encoding: "utf8" });
  assert.equal(result.status, 0, `git ${args.join(" ")} failed: ${result.stderr || result.error}`);
  return result.stdout.trim();
}

function assertTrackedClean(rootDir) {
  assert.equal(git(rootDir, "diff", "--name-only", "HEAD", "--"), "",
    `tracked source changed under ${rootDir}`);
}

async function atomicWrite(file, value) {
  const temp = `${file}.pending`;
  try {
    await lstat(temp);
    throw new Error(`refusing existing temporary output ${temp}`);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  await writeFile(temp, `${JSON.stringify(value, null, 2)}\n`, { flag: "wx" });
  await assertRegular(temp);
  await rename(temp, file);
  await assertRegular(file);
}

async function packageSnapshot(name) {
  const expected = VARIANTS[name];
  const checkout = path.join(root, expected.checkout);
  await assertDirectory(checkout);
  assert.equal(git(checkout, "rev-parse", "HEAD"), expected.source,
    `${name} source checkout changed`);
  assertTrackedClean(checkout);
  const sourceBytes = await readFile(path.join(checkout, "web/python/examples/showcase_bezier_paths.py"));
  assert.equal(sha256(sourceBytes), SOURCE_SHA, `${name} source fixture differs`);
  const manifestPath = path.join(checkout, "web/ci-artifact.json");
  const identityPath = path.join(checkout, "web/runtime-build-identity.json");
  await assertRegular(manifestPath);
  await assertRegular(identityPath);
  const artifact = JSON.parse(await readFile(manifestPath, "utf8"));
  const identity = JSON.parse(await readFile(identityPath, "utf8"));
  assert.equal(artifact.source, expected.source, `${name} artifact source mismatch`);
  assert.equal(identity.sourceRevision, expected.source, `${name} runtime identity source mismatch`);
  for (const file of Object.values(identity.files ?? {})) {
    const relative = file.path.replace(/^\.\//, "");
    const target = path.join(checkout, "web", relative);
    await assertRegular(target);
    assert.equal(sha256(await readFile(target)), file.sha256, `${name} runtime payload mismatch: ${relative}`);
  }
  assert.equal(pins.variants[name].sourceRevision, expected.source);

  const verifier = PRODUCER_VERIFIERS[name];
  const verifierRoot = path.join(root, verifier.checkout);
  assert.equal(git(verifierRoot, "rev-parse", "HEAD"), verifier.source,
    `${name} producer verifier source checkout changed`);
  const verifierPath = path.join(verifierRoot, ".github/ci/product-artifact.mjs");
  await assertRegular(verifierPath);
  const verifierSha256 = sha256(await readFile(verifierPath));
  const artifactModule = await import(pathToFileURL(verifierPath));
  const identityBuilderPath = path.join(verifierRoot, "scripts/build-runtime-identity.mjs");
  await assertRegular(identityBuilderPath);
  const identityBuilderSha256 = sha256(await readFile(identityBuilderPath));
  const { runtimeBuildIdentityCore } = await import(pathToFileURL(identityBuilderPath));
  const identityCore = runtimeBuildIdentityCore({ sourceRevision: identity.sourceRevision,
    hashes: Object.fromEntries(Object.entries(identity.files).map(([key, file]) => [key, file.sha256])) });
  assert.equal(identity.buildId, sha256(JSON.stringify(identityCore)), `${name} runtime identity digest mismatch`);
  const sources = artifact.productSources;
  assert.ok(sources && sources.baseline && sources.candidate && sources.head && sources.eventBase,
    `${name} package lacks product source provenance`);
  const sourceForRole = expected.role === "baseline" ? sources.baseline : sources.candidate;
  assert.equal(sourceForRole, expected.source, `${name} product source role mismatch`);
  const env = {
    ...process.env,
    GITHUB_SHA: sourceForRole,
    NOON_PRODUCT_BASE_SHA: sources.baseline,
    NOON_PRODUCT_CANDIDATE_SHA: sources.candidate,
    NOON_PRODUCT_HEAD_SHA: sources.head,
    NOON_PRODUCT_EVENT_BASE_SHA: sources.eventBase,
    NOON_WASM_PROFILE: "release",
    NOON_WASM_SKIP_OPT: "0",
    NOON_RENDERER_SMOKE: "0",
  };
  const verified = await artifactModule.verifyProductArtifact(checkout, env, expected.role);
  assert.equal(verified.source, expected.source);
  return {
    sourceRevision: expected.source,
    tree: git(checkout, "rev-parse", "HEAD^{tree}"),
    sourceFixtureSha256: sha256(sourceBytes),
    runtimeIdentity: identity,
    producerVerifier: { checkout: verifier.checkout, source: verifier.source, sha256: verifierSha256,
      identityBuilderSha256 },
    productSources: sources,
    packageFiles: artifact.files,
    packageManifestSha256: sha256(await readFile(manifestPath)),
  };
}

await assertDirectory(studyRoot);
await assertDirectory(harness);
await assertRegular(process.argv[1]);
await assertRegular(path.join(root, "scripts/prepare_three_variant_packages.py"));
try {
  await lstat(studyPath);
  throw new Error(`refusing existing study output ${studyPath}`);
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
for (const file of [
  e2ePath,
  path.join(harness, "scripts/playground-product-fps.mjs"),
  path.join(harness, "scripts/showcase-live-review.mjs"),
  path.join(harness, "scripts/playground-browser-failure-evidence.mjs"),
  path.join(harness, ".github/ci/wasm-build.mjs"),
  path.join(harness, "web/python/examples/showcase_bezier_paths.py"),
]) await assertRegular(file);
assert.equal(git(harness, "rev-parse", "HEAD"), "8ca4f6afd3bc1a7a41de254a0750dcbe392d6a20",
  "canonical 8ca product E2E checkout changed");
assertTrackedClean(harness);
assert.equal(process.versions.node, "24.22.1", "diagnostic Node version changed");

const measurementsModule = await import(pathToFileURL(path.join(harness, "scripts/playground-product-fps.mjs")));
const measurement = measurementsModule.productMeasurement("showcase-bezier-paths");
assert.equal(measurement.windowStartSeconds, 1.4);
assert.equal(measurement.windowEndSeconds, 3.4);
const authoredSource = await readFile(path.join(harness, "web/python/examples/showcase_bezier_paths.py"));
assert.equal(sha256(authoredSource), SOURCE_SHA, "canonical harness fixture changed");
const e2eSha256 = sha256(await readFile(e2ePath));

const requireHarness = createRequire(path.join(harness, "package.json"));
const { chromium } = requireHarness("playwright");
const browser = await chromium.launch({
  channel: "chromium",
  headless: true,
  args: [
    "--disable-features=WebGPU", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist",
    "--use-gl=angle", "--use-angle=swiftshader", "--disable-gpu-sandbox", "--disable-dev-shm-usage",
  ],
});
const browserVersion = browser.version();
await browser.close();
assert.equal(browserVersion, EXPECTED_BROWSER, "installed Chromium version changed");

const before = {};
for (const name of Object.keys(VARIANTS)) before[name] = await packageSnapshot(name);
const order = ORDERS.flatMap((variants, round) => variants.map((variant, position) => ({
  id: `round-${round + 1}-${variant}`,
  round: round + 1,
  position: position + 1,
  variant,
  status: "not-started",
})));
const study = {
  schemaVersion: 1,
  status: "running",
  diagnosticOnly: true,
  acceptanceComparator: null,
  sourceRevision: git(harness, "rev-parse", "HEAD"),
  sourceTree: git(harness, "rev-parse", "HEAD^{tree}"),
  harnessPath: "scripts/playground-product-e2e.mjs",
  harnessSha256: e2eSha256,
  sourcePath: measurement.sourcePath,
  sourceSha256: SOURCE_SHA,
  measurement,
  browserVersion,
  nodeVersion: process.versions.node,
  runtimeConfig: {
    backendExpected: "WebGL2", gpuModeExpected: "software-WebGL", viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1, executionMode: "semantic", sourceRunsPerVariant: 3,
  },
  latinSquareOrder: ORDERS,
  packagesBefore: before,
  plannedRuns: order,
};

async function update() {
  await atomicWrite(studyPath, study);
}

await update();
let failed = false;
for (const run of order) {
  const variant = VARIANTS[run.variant];
  const siteRoot = path.join(root, variant.checkout);
  const runDir = path.join(studyRoot, "runs", run.id);
  const logDir = path.join(studyRoot, "logs");
  await ensureDirectory(path.join(studyRoot, "runs"));
  await ensureDirectory(logDir);
  try {
    await lstat(runDir);
    throw new Error(`refusing existing run artifact path ${runDir}`);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  run.status = "running";
  run.startedAt = new Date().toISOString();
  await update();
  const env = { ...process.env };
  delete env.NOON_PRODUCT_PAIR_INDEX;
  delete env.NOON_PRODUCT_PAIR_POSITION;
  env.NOON_PRODUCT_SITE_ROOT = siteRoot;
  env.NOON_PRODUCT_PORT = "4205";
  env.NOON_PRODUCT_EXAMPLE = "showcase-bezier-paths";
  env.NOON_PRODUCT_LABEL = variant.role;
  env.NOON_PRODUCT_ARTIFACT_DIR = runDir;
  const result = spawnSync(process.execPath, [e2ePath], {
    cwd: harness, env, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
  });
  const stdoutPath = path.join(logDir, `${run.id}.stdout.log`);
  const stderrPath = path.join(logDir, `${run.id}.stderr.log`);
  await writeFile(stdoutPath, result.stdout ?? "", { flag: "wx" });
  await writeFile(stderrPath, result.stderr ?? "", { flag: "wx" });
  await assertRegular(stdoutPath);
  await assertRegular(stderrPath);
  run.finishedAt = new Date().toISOString();
  run.exitCode = result.status;
  run.signal = result.signal;
  run.error = result.error ? String(result.error) : null;
  run.stdoutPath = path.relative(studyRoot, stdoutPath);
  run.stderrPath = path.relative(studyRoot, stderrPath);
  run.artifactDir = path.relative(studyRoot, runDir);
  run.status = result.status === 0 && result.error === undefined ? "passed" : "failed";
  try {
    await assertNoSymlinks(runDir);
    const reportPath = path.join(runDir, "report.json");
    await assertRegular(reportPath);
    const report = JSON.parse(await readFile(reportPath, "utf8"));
    assert.equal(report.label, variant.role);
    assert.equal(report.exampleId, "showcase-bezier-paths");
    assert.equal(report.source.path, measurement.sourcePath);
    assert.equal(report.source.sha256, SOURCE_SHA);
    assert.deepEqual(report.measurement, measurement);
    assert.equal(report.runtime.backend, "WebGL2");
    assert.equal(report.runtime.gpuMode, "software-WebGL");
    assert.equal(report.runtime.browserVersion, browserVersion);
    assert.deepEqual(report.runtime.viewport, { width: 1280, height: 800 });
    assert.equal(report.runtime.deviceScaleFactor, 1);
    assert.equal(report.runtime.executionMode, "semantic");
    assert.equal(report.runtimeIdentity.sourceRevision, variant.source);
    assert.equal(report.runtimeIdentity.buildId, before[run.variant].runtimeIdentity.buildId);
    assert.equal(report.pair, null, "the diagnostic must not reuse the two-arm comparator protocol");
    run.report = {
      reportPath: path.relative(studyRoot, reportPath),
      source: report.source,
      runtimeIdentity: report.runtimeIdentity,
      runtime: report.runtime,
      fps: report.fps,
      rendererCosts: report.rendererCosts,
      presentationGaps: report.presentationGaps,
      coldRunMs: report.coldRunMs,
      warmRunMs: report.warmRunMs,
      editRunMs: report.editRunMs,
    };
  } catch (error) {
    run.reportValidationError = String(error);
    run.status = "failed";
  }
  if (run.status !== "passed") failed = true;
  await update();
  console.log(`${run.id} ${run.variant}: ${run.status} exit=${run.exitCode} fps=${run.report?.fps?.effectiveFps ?? "unavailable"}`);
}

study.packagesAfter = {};
for (const name of Object.keys(VARIANTS)) {
  try {
    study.packagesAfter[name] = await packageSnapshot(name);
    assert.deepEqual(study.packagesAfter[name], study.packagesBefore[name],
      `${name} source/package identity changed during the cohort`);
  } catch (error) {
    study.postcheckError ??= {};
    study.postcheckError[name] = String(error);
    failed = true;
  }
}
const harnessAfter = {
  sourceRevision: git(harness, "rev-parse", "HEAD"),
  sourceTree: git(harness, "rev-parse", "HEAD^{tree}"),
  harnessSha256: sha256(await readFile(e2ePath)),
  sourceSha256: sha256(await readFile(path.join(harness, "web/python/examples/showcase_bezier_paths.py"))),
};
study.harnessAfter = harnessAfter;
if (harnessAfter.sourceRevision !== study.sourceRevision || harnessAfter.sourceTree !== study.sourceTree ||
    harnessAfter.harnessSha256 !== e2eSha256 || harnessAfter.sourceSha256 !== SOURCE_SHA) {
  study.postcheckError = { ...(study.postcheckError ?? {}), harness: "canonical harness/source changed during study" };
  failed = true;
}
study.status = failed ? "incomplete" : "observed";
study.finishedAt = new Date().toISOString();
await update();
if (failed) process.exitCode = 1;

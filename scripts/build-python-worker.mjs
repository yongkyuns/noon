import { createHash } from "node:crypto";
import { readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const webRoot = path.join(repoRoot, "web");
const pythonRoot = path.join(webRoot, "python");
const sourcePath = path.join(webRoot, "python-worker.source.js");
const outputPath = path.join(webRoot, "python-worker.js");
const stableBundleUrl = "./python/compat-bundle.json";

const modules = await Promise.all(
  PYTHON_COMPAT_MODULES.map(async ({ sourcePath: modulePath, runtimePath, label }) => ({
    runtimePath,
    label,
    source: await readFile(path.join(webRoot, modulePath), "utf8"),
  })),
);
const bundlePayload = { version: 1, modules };
const contentHash = createHash("sha256")
  .update(JSON.stringify(bundlePayload))
  .digest("hex");
const bundleFileName = `compat-bundle.${contentHash}.json`;
const bundleUrl = `./python/${bundleFileName}`;
const bundle = { ...bundlePayload, contentHash };

for (const entry of await readdir(pythonRoot)) {
  if (/^compat-bundle\.[0-9a-f]{64}\.json$/.test(entry) && entry !== bundleFileName) {
    await rm(path.join(pythonRoot, entry));
  }
}
await writeFile(path.join(pythonRoot, bundleFileName), `${JSON.stringify(bundle)}\n`, "utf8");

const source = await readFile(sourcePath, "utf8");
const fetchBlock = [
  `  const response = await fetch(new URL("${stableBundleUrl}", import.meta.url));`,
  "  if (!response.ok) {",
  '    throw new Error(`Unable to load Noon Python compatibility bundle: HTTP ${response.status}`);',
  "  }",
  "  const bundle = await response.json();",
].join("\n");
if (source.indexOf(fetchBlock) === -1 || source.indexOf(fetchBlock) !== source.lastIndexOf(fetchBlock)) {
  throw new Error("Python worker compatibility loading boundary changed; update the generator explicitly");
}
const generatedFetchBlock = [
  `  const expectedContentHash = "${contentHash}";`,
  `  const response = await fetch(new URL("${bundleUrl}", import.meta.url));`,
  "  if (!response.ok) {",
  '    throw new Error(`Unable to load Noon Python compatibility bundle: HTTP ${response.status}`);',
  "  }",
  "  const bundle = await response.json();",
  "  if (bundle?.contentHash !== expectedContentHash) {",
  '    throw new Error("Noon Python compatibility bundle content hash does not match this worker");',
  "  }",
].join("\n");

const promiseBlock = "const pyodidePromise = initializePyodide();";
const generatedPromiseBlock = [
  "const runtimeBuildPromise = loadRuntimeBuild();",
  promiseBlock,
].join("\n");
const readyBlock = [
  "pyodidePromise",
  '  .then(() => post("ready"))',
  "  .catch(failAuthoringWorker);",
].join("\n");
const generatedReadyBlock = [
  "Promise.all([pyodidePromise, runtimeBuildPromise])",
  '  .then(([, runtimeBuild]) => post("ready", { buildIdentity: runtimeBuild.identity }))',
  "  .catch(failAuthoringWorker);",
].join("\n");
const wasmInitBlock = '  const noonWebReady = measureStartupTask(resourceDurations, "noonWebInitMs", () => initNoonWeb());';
const generatedWasmInitBlock = [
  '  const noonWebReady = measureStartupTask(resourceDurations, "noonWebInitMs", async () => {',
  "    const runtimeBuild = await runtimeBuildPromise;",
  "    return initNoonWeb({ module_or_path: runtimeBuild.wasmBytes });",
  "  });",
].join("\n");
for (const [label, block] of [["runtime promise", promiseBlock], ["ready", readyBlock], ["WASM init", wasmInitBlock]]) {
  if (source.indexOf(block) === -1 || source.indexOf(block) !== source.lastIndexOf(block)) {
    throw new Error(`Python worker ${label} boundary changed; update the generator explicitly`);
  }
}

const runtimeBuildHelpers = `
async function loadRuntimeBuild() {
  const response = await fetch(new URL("./runtime-build-identity.json", import.meta.url), { redirect: "error" });
  if (!response.ok) {
    throw new Error(\`Unable to load Noon runtime build identity: HTTP \${response.status}\`);
  }
  const payload = await response.json();
  const identity = await validateRuntimeBuildIdentity(payload);
  const [workerBytes, wasmBytes, glueBytes] = await Promise.all([
    fetchVerifiedRuntimeBytes(identity.files.worker),
    fetchVerifiedRuntimeBytes(identity.files.wasm),
    fetchVerifiedRuntimeBytes(identity.files.glue),
  ]);
  // workerBytes and glueBytes are deliberately read: their hashes prove the
  // served module graph agrees with this identity. WASM bytes are additionally
  // passed directly into wasm-bindgen below so the engine cannot fetch a
  // different module after verification.
  void workerBytes;
  void glueBytes;
  return Object.freeze({ identity, wasmBytes });
}

async function validateRuntimeBuildIdentity(payload) {
  if (!isRecord(payload) || !hasExactKeys(payload, ["schema", "sourceRevision", "files", "buildId"]) ||
      payload.schema !== 1 || !/^[0-9a-f]{64}$/.test(payload.buildId) ||
      (payload.sourceRevision !== null && !/^[0-9a-f]{40}$/.test(payload.sourceRevision)) ||
      !isRecord(payload.files) || !hasExactKeys(payload.files, ["worker", "wasm", "glue"])) {
    throw new Error("Noon runtime build identity has an invalid envelope");
  }
  const expectedPaths = {
    worker: "./python-worker.js",
    wasm: "./pkg/noon_web_bg.wasm",
    glue: "./pkg/noon_web.js",
  };
  const files = {};
  for (const key of ["worker", "wasm", "glue"]) {
    const descriptor = payload.files[key];
    if (!isRecord(descriptor) || !hasExactKeys(descriptor, ["path", "sha256"]) ||
        descriptor.path !== expectedPaths[key] || !/^[0-9a-f]{64}$/.test(descriptor.sha256)) {
      throw new Error(\`Noon runtime build identity has invalid \${key} provenance\`);
    }
    files[key] = Object.freeze({ path: descriptor.path, sha256: descriptor.sha256 });
  }
  const core = {
    schema: 1,
    sourceRevision: payload.sourceRevision,
    files: { worker: files.worker, wasm: files.wasm, glue: files.glue },
  };
  const expectedBuildId = await sha256Hex(new TextEncoder().encode(JSON.stringify(core)));
  if (expectedBuildId !== payload.buildId) {
    throw new Error("Noon runtime build identity digest does not match its contents");
  }
  Object.freeze(core.files);
  return Object.freeze({ ...core, buildId: payload.buildId });
}

async function fetchVerifiedRuntimeBytes(descriptor) {
  const response = await fetch(new URL(descriptor.path, import.meta.url), { redirect: "error" });
  if (!response.ok) {
    throw new Error(\`Unable to load Noon runtime file \${descriptor.path}: HTTP \${response.status}\`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  const actual = await sha256Hex(bytes);
  if (actual !== descriptor.sha256) {
    throw new Error(\`Noon runtime file hash does not match build identity: \${descriptor.path}\`);
  }
  return bytes;
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(digest, (value) => value.toString(16).padStart(2, "0")).join("");
}

function hasExactKeys(value, expected) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}
`;

const generatedSource = source
  .replace(fetchBlock, generatedFetchBlock)
  .replace(promiseBlock, generatedPromiseBlock)
  .replace(readyBlock, generatedReadyBlock)
  .replace(wasmInitBlock, generatedWasmInitBlock);
const generated = [
  "// Generated by scripts/build-python-worker.mjs. Do not edit directly.",
  generatedSource,
  runtimeBuildHelpers,
].join("\n");

if (generated.includes(stableBundleUrl)) {
  throw new Error("generated Python worker still references the mutable compatibility bundle URL");
}
if (generated.includes("() => initNoonWeb()")) {
  throw new Error("generated Python worker can still fetch unverified WASM implicitly");
}

await writeFile(outputPath, `${generated}\n`, "utf8");
console.log(
  `✓ built python-worker.js with ${modules.length} modules at ${bundleFileName}`,
);

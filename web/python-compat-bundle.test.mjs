import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";

// Coverage runs web/*.test.mjs directly, so materialize the ignored runtime
// worker and content-addressed bundle here instead of relying on build order.
await import("../scripts/build-python-worker.mjs");

const generatedWorker = await readFile(new URL("./python-worker.js", import.meta.url), "utf8");
const sourceWorker = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");

// The verified module can be handed to the renderer after Noon initializes,
// while Pyodide and its compatibility bundle are still preparing.
const runtimeStartup = new Function("loadCompiledRuntimeBuild", "initNoonWeb", `${generatedWorker.slice(
  generatedWorker.indexOf("const runtimeBuildPromise ="),
  generatedWorker.indexOf("const pyodidePromise ="),
)}; return runtimeBuildPromise;`);

test("worker startup instantiates and exposes the same verified module", async () => {
  const wasmModule = new WebAssembly.Module(Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0));
  const build = Object.freeze({ identity: { buildId: "verified-build" }, wasmModule });
  const initialized = Promise.withResolvers();
  let calls = 0;
  let settled = false;
  const ready = runtimeStartup(async () => build, options => {
    calls += 1;
    assert.equal(options.module_or_path, wasmModule);
    return initialized.promise;
  });
  void ready.then(() => { settled = true; });
  await Promise.resolve();
  assert.equal(calls, 1);
  assert.equal(settled, false, "readiness must wait for actual WASM initialization");
  initialized.resolve();
  assert.equal(await ready, build);
});

for (const stage of ["verification", "initialization"]) {
  test(`${stage} failure preserves the original startup rejection`, async () => {
    const failure = new Error(stage);
    let initializationCalls = 0;
    const ready = runtimeStartup(
      async () => {
        if (stage === "verification") throw failure;
        return { identity: {}, wasmModule: {} };
      },
      async () => { initializationCalls += 1; throw failure; },
    );
    await assert.rejects(ready, error => error === failure);
    assert.equal(initializationCalls, stage === "verification" ? 0 : 1);
  });
}

const bundleMatch = generatedWorker.match(
  /\.\/python\/(compat-bundle\.([0-9a-f]{64})\.json)/,
);
assert.ok(bundleMatch, "generated worker must reference one content-addressed compatibility bundle");
assert.equal(
  generatedWorker.match(/compat-bundle\.[0-9a-f]{64}\.json/g)?.length,
  1,
  "generated worker must reference exactly one compatibility bundle generation",
);

const [, bundleFileName, expectedHash] = bundleMatch;
const generated = JSON.parse(
  await readFile(new URL(`./python/${bundleFileName}`, import.meta.url), "utf8"),
);
const { contentHash, ...bundlePayload } = generated;
const actualHash = createHash("sha256")
  .update(JSON.stringify(bundlePayload))
  .digest("hex");

// Every public lazy import must be available in the deployed interpreter.
const publicSource = await readFile(new URL("./python/noon.py", import.meta.url), "utf8");
const exportsMatch = publicSource.match(/_PUBLIC_EXPORTS = \{([\s\S]*?)\n\}/);
assert.ok(exportsMatch, "public lazy module mapping must remain inspectable");
const lazyModules = [...exportsMatch[1].matchAll(/^\s*"[^"]+":\s*"([^"]+)"/gm)].map(match => match[1]);
assert.ok(lazyModules.length > 0, "public lazy export inventory must not be empty");
for (const module of new Set(lazyModules)) {
  assert.ok(PYTHON_COMPAT_MODULES.some(entry => entry.runtimePath === `/tmp/${module}.py`),
    `${module}: public lazy import missing from worker compatibility manifest`);
}

const moduleCount = PYTHON_COMPAT_MODULES.length;
assert.ok(moduleCount >= 19, "compatibility manifest must retain the supported bootstrap surface");
assert.equal(new Set(PYTHON_COMPAT_MODULES.map((module) => module.sourcePath)).size, moduleCount);
assert.equal(new Set(PYTHON_COMPAT_MODULES.map((module) => module.runtimePath)).size, moduleCount);
assert.equal(generated.version, 1);
assert.equal(generated.modules.length, PYTHON_COMPAT_MODULES.length);
assert.equal(contentHash, expectedHash, "bundle envelope hash must match its immutable filename");
assert.equal(actualHash, expectedHash, "bundle filename must be derived from its exact payload");

for (const [index, expected] of PYTHON_COMPAT_MODULES.entries()) {
  const actual = generated.modules[index];
  assert.equal(actual.runtimePath, expected.runtimePath, `${expected.sourcePath}: runtime path drift`);
  assert.equal(actual.label, expected.label, `${expected.sourcePath}: label drift`);
  assert.equal(
    actual.source,
    await readFile(new URL(`./${expected.sourcePath}`, import.meta.url), "utf8"),
    `${expected.sourcePath}: generated bundle is stale`,
  );
}

assert.match(
  sourceWorker,
  /fetch\(new URL\("\.\/python\/compat-bundle\.json", import\.meta\.url\)\)/,
  "worker template must retain the explicit generator replacement boundary",
);
assert.doesNotMatch(
  generatedWorker,
  /new URL\("\.\/python\/compat-bundle\.json"/,
  "deployed Python worker must not request the mutable bundle URL",
);
assert.match(
  generatedWorker,
  new RegExp(`expectedContentHash = "${expectedHash}"`),
  "worker and compatibility bundle must share the same content identity",
);
assert.match(
  generatedWorker,
  /Generated by scripts\/build-python-worker\.mjs/,
  "deployed Python worker must identify its generator",
);
assert.match(
  generatedWorker,
  /import \{ loadCompiledRuntimeBuild \} from "\.\/runtime-build-verifier\.js";/,
  "deployed Python worker must use the checked-in runtime byte verifier",
);
assert.match(
  generatedWorker,
  /initNoonWeb\(\{ module_or_path: build\.wasmModule \}\)/,
  "deployed Python worker must initialize wasm-bindgen from the verified compiled module",
);
assert.doesNotMatch(
  generatedWorker,
  /\(\) => initNoonWeb\(\)/,
  "deployed Python worker must not retain implicit WASM fetching",
);
assert.match(
  generatedWorker,
  /PYTHON_COMPAT_MODULES/,
  "generated worker must retain manifest validation",
);
assert.ok(
  generatedWorker.length < sourceWorker.length + 1024,
  "Python sources must stay out of the worker critical path instead of being embedded",
);

console.log("✓ Python compatibility bootstrap is parallel, content-addressed, and runtime-provenanced");

// Actual Pyodide -> generated WASM binding qualification without a browser/GPU.
// NOON_PYODIDE_ROOT must contain the pinned official Pyodide distribution and
// NumPy wheel. Uses the same source fixture as shared-authoring-smoke.mjs.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import initNoonWeb, * as wasm from "../web/pkg/noon_web.js";
import { resolveAnimationOptionsPlain } from "../web/animation-options.js";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = fileURLToPath(new URL("../", import.meta.url));
const runtimeRoot = process.env.NOON_PYODIDE_ROOT;
if (!runtimeRoot) {
  throw new Error("NOON_PYODIDE_ROOT is required; this test must not silently skip");
}
const hash = (value) => createHash("sha256").update(value).digest("hex");
const report = {
  host: "Node/Pyodide with actual Rust WASM authoring",
  node: process.version,
  browser: false,
  gpu: false,
  sourceContinuationMode: "direct construct with synchronous Rust ordinaryWait",
};
try {
  const pinnedWorker = fs.readFileSync(path.join(root, "web/python-worker.source.js"), "utf8");
  const pinnedVersion = pinnedWorker.match(/pyodide\/v([^/]+)\/full\/pyodide\.mjs/)?.[1];
  assert.ok(pinnedVersion, "worker must pin the interpreter version");
  const { loadPyodide } = await import(pathToFileURL(path.resolve(runtimeRoot, "pyodide.mjs")));
  const module = fs.readFileSync(path.join(root, "web/pkg/noon_web_bg.wasm"));
  report.wasmSha256 = hash(module);
  report.bindingsSha256 = hash(fs.readFileSync(path.join(root, "web/pkg/noon_web.js")));
  await initNoonWeb({ module_or_path: module });
  const pyodide = await loadPyodide({ indexURL: path.resolve(runtimeRoot) + path.sep });
  assert.equal(pyodide.version, pinnedVersion, "test interpreter must match the worker pin");
  report.pyodideVersion = pyodide.version;
  await pyodide.loadPackage(["numpy"]);
  const authoringStore = new wasm.WasmAuthoringStore();
  // Real typed factories, equivalent to the worker's bindings for this fixture.
  // No monkeypatched Python setters, mocked scene state, or effect interpreter.
  Object.assign(globalThis, {
    noonCreateCanonicalAuthoringSceneContext: () => authoringStore.createSceneContext(),
    noonAuthoringGeometryOptions: wasm.WasmManimGeometryOptions,
    noonAuthoringVectorPath: () => new wasm.WasmAuthoringVectorPath(),
    noonCreateAuthoringGeometryHandle: (options) => authoringStore.createManimGeometry(options),
    noonAuthoringMembershipBatch: (kind) => new wasm.WasmSceneMembershipBatch(kind),
    noonCreateAuthoringFamilyHandle: (batch, zIndex) => authoringStore.createFamily(batch, zIndex),
    noonGlowUpdate: (...args) => new wasm.WasmGlowUpdate(...args),
    noonGlow: (update) => new wasm.WasmGlow(update),
    noonResolveAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveAnimationOptions, ...args),
    noonResolveTransformAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveTransformAnimationOptions, ...args),
  });
  for (const descriptor of PYTHON_COMPAT_MODULES) {
    pyodide.FS.writeFile(descriptor.runtimePath,
      fs.readFileSync(path.join(root, "web", descriptor.sourcePath), "utf8"));
  }
  pyodide.runPython('import sys\nsys.path.insert(0, "/tmp")\nimport noon');
  const fixture = fs.readFileSync(path.join(root, "web/python/examples/effect_authoring_contract.py"), "utf8");
  report.fixtureSha256 = hash(fixture);
  // Direct construction intentionally tests the existing synchronous context
  // path, not JSPI, worker continuation/handoff, browser rendering or seeking.
  pyodide.runPython(fixture + `
import json
import _manim_reactive
import _manim_scene
scene = EffectAuthoringContract()
token = _manim_reactive._enter_authoring_scene(scene)
try:
    scene.setup()
    scene.construct()
    scene.tear_down()
finally:
    _manim_reactive._leave_authoring_scene(token)
_context = _manim_scene.execution_context(scene)
_result_json = json.dumps({"object_count": len(scene.mobjects), "time": scene.time,
    "ownership": str(_context.liveExecutionOwnership()),
    "center": list(scene.mobjects[0].get_center())})
`);
  report.observed = JSON.parse(pyodide.globals.get("_result_json"));
  assert.equal(report.observed.object_count, 1);
  assert.ok(Math.abs(report.observed.time - 0.1) < 1e-9);
  assert.equal(report.observed.ownership, "active");
  assert.deepEqual(report.observed.center, [0, 0]);
  report.result = "PASS";
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  report.result = "FAIL";
  report.failure = String(error.stack ?? error);
  console.error(error);
  process.exitCode = 1;
} finally {
  if (process.env.NOON_EFFECT_REPORT) {
    fs.writeFileSync(process.env.NOON_EFFECT_REPORT, JSON.stringify(report, null, 2) + "\n");
  }
}

// Direct Pyodide -> real generated WASM bindings, without browser/GPU mocks.
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
assert.ok(runtimeRoot, "NOON_PYODIDE_ROOT must contain the pinned official distribution and NumPy wheel");
const hash = value => createHash("sha256").update(value).digest("hex");
const report = { host: "Node/Pyodide with actual Rust WASM", node: process.version, browser: false, gpu: false };
try {
  const worker = fs.readFileSync(path.join(root, "web/python-worker.source.js"), "utf8");
  const pinnedVersion = worker.match(/pyodide\/v([^/]+)\/full\/pyodide\.mjs/)?.[1];
  const bytes = fs.readFileSync(path.join(root, "web/pkg/noon_web_bg.wasm"));
  report.wasmSha256 = hash(bytes);
  await initNoonWeb({ module_or_path: bytes });
  for (const index of [-1, 0.5, 2 ** 32, NaN, Infinity]) {
    assert.throws(() => new wasm.WasmCodeOptions("python", undefined, "vim", index, true, 1, "rectangle"),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  for (const index of [0.5, 2 ** 31, -(2 ** 31) - 1, NaN, Infinity]) {
    assert.throws(() => new wasm.WasmCodeOptions("python", undefined, "vim", 4, true, index, "rectangle"),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  const { loadPyodide } = await import(pathToFileURL(path.resolve(runtimeRoot, "pyodide.mjs")));
  const pyodide = await loadPyodide({ indexURL: path.resolve(runtimeRoot) + path.sep });
  assert.equal(pyodide.version, pinnedVersion);
  report.pyodideVersion = pyodide.version;
  await pyodide.loadPackage("numpy");
  const authoringStore = new wasm.WasmAuthoringStore();
  Object.assign(globalThis, {
    noonCreateCanonicalAuthoringSceneContext: () => authoringStore.createSceneContext(),
    noonAuthoringGeometryOptions: wasm.WasmManimGeometryOptions,
    noonAuthoringVectorPath: () => new wasm.WasmAuthoringVectorPath(),
    noonCreateAuthoringGeometryHandle: options => authoringStore.createManimGeometry(options),
    noonAuthoringMembershipBatch: kind => new wasm.WasmSceneMembershipBatch(kind),
    noonCreateAuthoringFamilyHandle: (batch, zIndex) => authoringStore.createFamily(batch, zIndex),
    noonNativeFontFace: (...args) => new wasm.WasmNativeFontFace(...args),
    noonBundledNativeFontFace: family => wasm.WasmNativeFontFace.bundled(family),
    noonTextColorBatch: () => new wasm.WasmTextColorBatch(),
    noonCreateAuthoringTextHandle: (...args) => authoringStore.createManimText(...args),
    noonResolveAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveAnimationOptions, ...args),
    noonResolveCreateAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveCreateAnimationOptions, ...args),
    noonResolveTransformAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveTransformAnimationOptions, ...args),
    noonCodeOptions: (...args) => new wasm.WasmCodeOptions(...args),
    noonCodeStylesList: () => wasm.noonCodeStylesList(),
    noonCodeLanguagesList: () => wasm.noonCodeLanguagesList(),
    noonCodeFromFamily: family => family.asCode(),
    noonCreateAuthoringCodeHandle: (source, options, context) => context == null
      ? authoringStore.createCode(source, options) : context.liveCreateCode(source, options),
  });
  for (const descriptor of PYTHON_COMPAT_MODULES) {
    pyodide.FS.writeFile(descriptor.runtimePath,
      fs.readFileSync(path.join(root, "web", descriptor.sourcePath), "utf8"));
  }
  pyodide.runPython('import sys\nsys.path.insert(0, "/tmp")');
  const fixture = fs.readFileSync(new URL("./code-authoring-contract.py", import.meta.url), "utf8");
  report.fixtureSha256 = hash(fixture);
  pyodide.runPython(fixture + `
import json
import _manim_reactive
import _manim_scene
scene = CodeAuthoringContract()
token = _manim_reactive._enter_authoring_scene(scene)
try:
    scene.construct()
finally:
    _manim_reactive._leave_authoring_scene(token)
context = _manim_scene.execution_context(scene)
_code_result = json.dumps({"families": len(scene.mobjects), "time": scene.time,
                          "ownership": str(context.liveExecutionOwnership())})
`);
  report.observed = JSON.parse(pyodide.globals.get("_code_result"));
  assert.equal(report.observed.families, 3);
  assert.ok(Math.abs(report.observed.time - 0.1) < 1e-9);
  assert.equal(report.observed.ownership, "active");
  report.result = "PASS";
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  report.result = "FAIL";
  report.failure = String(error.stack ?? error);
  console.error(error);
  process.exitCode = 1;
} finally {
  if (process.env.NOON_CODE_REPORT) fs.writeFileSync(process.env.NOON_CODE_REPORT, JSON.stringify(report, null, 2) + "\n");
}

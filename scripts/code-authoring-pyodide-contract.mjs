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
  const boundaryStore = new wasm.WasmAuthoringStore();
  const numberOptions = new wasm.WasmCodeOptions("text", undefined, "vim", 4, true, -1, "rectangle");
  for (const digits of [true, false, "2", 2n, -1, 0.5, 33, 2 ** 32, NaN, Infinity]) {
    assert.throws(() => numberOptions.setLineNumberLayout(11, digits),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  numberOptions.setLineNumberLayout(11, 2);
  const numberedCode = boundaryStore.createCode("x\nx\nx", numberOptions);
  assert.deepEqual(numberedCode.lineNumberSources(), ["-01", "00", "01"]);
  report.lineNumberBoundaryChecks = "typed bounded padding, signed numbering, independent gutter size";
  const boundaryCode = boundaryStore.createCode("x\n\nz", new wasm.WasmCodeOptions(
    "python", undefined, "vim", 4, true, 9, "rectangle"));
  const invalidIndices = [true, false, "1", 1n, -1, 0.5, 2 ** 32, NaN, Infinity];
  for (const index of [...invalidIndices, null, undefined]) {
    assert.throws(() => boundaryCode.getHighlightedLines(index, undefined, new wasm.WasmCodeHighlightOptions(false)),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  for (const index of invalidIndices) {
    assert.throws(() => boundaryCode.highlightLines(0, index, new wasm.WasmCodeHighlightOptions(false)),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  for (const [start, stop] of [[0, 0], [2, 1], [0, 4]]) {
    assert.throws(() => boundaryCode.highlightLines(start, stop, new wasm.WasmCodeHighlightOptions(false)),
      error => error.category === "invalid_input" && error.code === "code.invalid_line_range");
  }
  assert.equal(boundaryCode.highlights().memberKeys().length, 0);
  boundaryCode.highlightLines(1, undefined, new wasm.WasmCodeHighlightOptions(false));
  assert.equal(boundaryCode.highlights().memberKeys().length, 1);
  report.lineRegionBoundaryChecks = "typed indices, atomic range rejection, cold attachment";
  for (const index of [...invalidIndices, null, undefined]) {
    assert.throws(() => boundaryCode.replaceLine(index, "z"),
      error => error.category === "invalid_input" && error.code === "code.invalid_option");
  }
  const sourceBefore = boundaryCode.lineSources();
  const membersBefore = boundaryCode.codeLines().memberKeys();
  for (const [index, source] of [[3, "z"], [0, "a\nb"], [0, "overflow".repeat(100)]]) {
    assert.throws(() => boundaryCode.replaceLine(index, source),
      error => error.category === "invalid_input");
  }
  assert.deepEqual(boundaryCode.lineSources(), sourceBefore);
  assert.deepEqual(boundaryCode.codeLines().memberKeys(), membersBefore);
  const replacement = boundaryCode.replaceLine(0, "z");
  assert.equal(replacement.previous().textSource(), "x");
  assert.equal(replacement.current().textSource(), "z");
  replacement.free();
  report.sourceReplacementBoundaryChecks = "typed indices, atomic fit/range rejection, ordinary row handles";
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
    noonCreateAuthoringFamilyHandle: (batch, zIndex, drawEnabled, ...color) => color.length
      ? authoringStore.createFamilyWithColor(batch, zIndex, ...color, drawEnabled)
      : authoringStore.createFamily(batch, zIndex, drawEnabled),
    noonNativeFontFace: (...args) => new wasm.WasmNativeFontFace(...args),
    noonBundledNativeFontFace: family => wasm.WasmNativeFontFace.bundled(family),
    noonTextColorBatch: () => new wasm.WasmTextColorBatch(),
    noonCreateAuthoringTextHandle: (...args) => authoringStore.createManimText(...args),
    noonResolveAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveAnimationOptions, ...args),
    noonResolveCreateAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveCreateAnimationOptions, ...args),
    noonResolveGlowPulseAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveGlowPulseAnimationOptions, ...args),
    noonResolveTransformAnimationOptions: (...args) => resolveAnimationOptionsPlain(wasm.resolveTransformAnimationOptions, ...args),
    noonCodeOptions: (...args) => new wasm.WasmCodeOptions(...args),
    noonCodeHighlightOptions: (...args) => new wasm.WasmCodeHighlightOptions(...args),
    noonCodeStylesList: () => wasm.noonCodeStylesList(),
    noonCodeLanguagesList: () => wasm.noonCodeLanguagesList(),
    noonCodeFromFamily: family => family.asCode(),
    noonCreateAuthoringCodeHandle: (source, options, context) => context == null
      ? authoringStore.createCode(source, options) : context.liveCreateCode(source, options),
    noonGetHighlightedCodeLines: (context, code, start, stop, options) => context == null
      ? code.getHighlightedLines(start, stop, options)
      : context.liveGetHighlightedCodeLines(code, start, stop, options),
    noonHighlightCodeLines: (context, code, start, stop, options) => context == null
      ? code.highlightLines(start, stop, options)
      : context.liveHighlightCodeLines(code, start, stop, options),
    noonReplaceCodeLine: (context, code, index, source) => context == null
      ? code.replaceLine(index, source) : context.liveReplaceCodeLine(code, index, source),
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
  const sourceFixture = fs.readFileSync(new URL("./code-source-authoring-contract.py", import.meta.url), "utf8");
  report.sourceFixtureSha256 = hash(sourceFixture);
  pyodide.runPython(sourceFixture + `
source_scene = CodeSourceAuthoringContract()
token = _manim_reactive._enter_authoring_scene(source_scene)
try:
    source_scene.construct()
finally:
    _manim_reactive._leave_authoring_scene(token)
source_context = _manim_scene.execution_context(source_scene)
_code_source_result = json.dumps({"families": len(source_scene.mobjects), "time": source_scene.time,
    "ownership": str(source_context.liveExecutionOwnership()), **source_scene.source_contract_observed})
`);
  report.sourceObserved = JSON.parse(pyodide.globals.get("_code_source_result"));
  assert.equal(report.sourceObserved.families, 2);
  assert.ok(Math.abs(report.sourceObserved.time - 0.8) < 1e-9);
  assert.equal(report.sourceObserved.ownership, "active");
  assert.equal(report.sourceObserved.unchanged_rows, 5);
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

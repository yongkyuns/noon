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
  for (const index of [-1, 0.5, 2 ** 32, NaN, Infinity]) {
    assert.throws(() => new wasm.WasmNativeFontFace("invalid", new Uint8Array(), index),
      error => error.category === "invalid_input" && error.code === "text.invalid_font_index");
  }
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
    noonNativeFontFace: (...args) => new wasm.WasmNativeFontFace(...args),
    noonBundledNativeFontFace: (family) => wasm.WasmNativeFontFace.bundled(family),
    noonTextColorBatch: () => new wasm.WasmTextColorBatch(),
    noonCreateAuthoringTextHandle: (...args) => authoringStore.createManimText(...args),
    noonCreateAuthoringMarkupTextHandle: (...args) => authoringStore.createManimMarkupText(...args),
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
  // Actual optional owned WASM arguments must retain the font wrapper for reuse
  // across cold/live plain and markup text, copies, and constructor defaults.
  pyodide.runPython(`
import copy
import json
from noon import NativeFontFace, Text, MarkupText, Scene, RED
from _noon_errors import NoonValueError, NoonMissingResourceError
import _manim_reactive
face = NativeFontFace.bundled("DejaVu Sans Mono")
assert face.family == "DejaVu Sans Mono" and face.face_index == 0
assert copy.copy(face) is face and copy.deepcopy(face) is face
implicit = Text("Same face")
explicit = Text("Same face", font=face)
assert abs(implicit.width - explicit.width) < 1e-9
assert abs(implicit.height - explicit.height) < 1e-9
assert abs(explicit.copy().width - explicit.width) < 1e-9
assert abs(MarkupText("Same face", font=face).width - explicit.width) < 1e-9
colored = Text("Same face", font=face, t2c={"Same": RED})
assert abs(colored.width - explicit.width) < 1e-9
Text.set_default(font=face)
try:
    assert abs(Text("Same face").width - explicit.width) < 1e-9
finally:
    Text.set_default()
font_scene = Scene()
font_token = _manim_reactive._enter_authoring_scene(font_scene)
try:
    live = Text("Same face", font=face)
    live_markup = MarkupText("Same face", font=face)
    live_colored = Text("Same face", font=face, t2c={"Same": RED})
    assert abs(live.width - explicit.width) < 1e-9
    assert abs(live_markup.width - explicit.width) < 1e-9
    assert abs(live_colored.width - explicit.width) < 1e-9
    font_scene.add(live, live_markup, live_colored)
finally:
    _manim_reactive._leave_authoring_scene(font_token)
assert len(font_scene.mobjects) == 3
for invalid_index in (True, -1, 2**32):
    try:
        NativeFontFace("invalid", b"bad bytes", invalid_index)
    except (TypeError, ValueError):
        pass
    else:
        raise AssertionError("invalid face index accepted")
try:
    NativeFontFace("invalid", b"not an OpenType face")
except NoonValueError as error:
    assert error.code == "text.invalid_font_face"
else:
    raise AssertionError("invalid font bytes accepted")
try:
    NativeFontFace.bundled("not an embedded Noon font")
except NoonMissingResourceError as error:
    assert error.code == "text.font_unavailable"
else:
    raise AssertionError("unavailable font accepted")
_font_result_json = json.dumps({"family": face.family, "faceIndex": face.face_index,
    "width": explicit.width, "liveObjectCount": len(font_scene.mobjects)})
`);
  report.nativeFontInput = JSON.parse(pyodide.globals.get("_font_result_json"));
  // A long text leaf must receive the flat play override just like its moving
  // sibling. Otherwise root rescaling accelerates the sibling to finish early.
  // Exercise the production Python route and generated bindings, without a GPU.
  pyodide.runPython(`
from noon import Square, VGroup, Write, Unwrite, Create, Uncreate, AnimationGroup, linear
import _manim_scene
_timing_rows = []
for operation in (Write, Unwrite, Create, Uncreate):
    for family in (False, True):
        timing_scene = Scene()
        timing_token = _manim_reactive._enter_authoring_scene(timing_scene)
        try:
            moving = Square()
            timing_scene.add(moving)
            target = VGroup(Text("FIRST"), Text("SECOND")) if family else Text("WRITE")
            if operation in (Unwrite, Uncreate):
                timing_scene.add(target)
            candidate, *_ = _manim_scene._build_canonical_composition_candidate(
                timing_scene, "parallel",
                (moving.animate.shift([2, 0, 0]), operation(target, run_time=3)),
                None, {"run_time": 2, "rate_func": linear})
            context = _manim_scene.execution_context(timing_scene)
            assert context.ordinaryCanPlayComposition(candidate)
            endpoint = context.beginOrdinaryComposition(candidate)
            context.liveAdvanceSegmentTo(0.5)
            x = moving.get_center()[0]
            assert abs(endpoint - 2) < 1e-9
            assert abs(x - 0.5) < 1e-6, (operation.__name__, family, x)
            context.liveAdvanceSegmentTo(endpoint)
            context.liveCompleteSegment()
            _timing_rows.append({"operation": operation.__name__, "family": family,
                "endpoint": endpoint, "siblingXAtHalfSecond": x})
        finally:
            _manim_reactive._leave_authoring_scene(timing_token)
# An explicit group keeps authored child durations and rescales the group.
# Its moving leaf lasts 2/3 of the 2-second group, so x(0.5) is 0.75.
timing_scene = Scene()
timing_token = _manim_reactive._enter_authoring_scene(timing_scene)
try:
    moving = Square()
    timing_scene.add(moving)
    target = Text("GROUP")
    group = AnimationGroup(moving.animate(run_time=2, rate_func=linear).shift([2, 0, 0]),
        Write(target, run_time=3))
    candidate, *_ = _manim_scene._build_canonical_composition_candidate(
        timing_scene, "parallel", tuple(group.animations), group, {"run_time": 2})
    context = _manim_scene.execution_context(timing_scene)
    endpoint = context.beginOrdinaryComposition(candidate)
    context.liveAdvanceSegmentTo(0.5)
    assert abs(endpoint - 2) < 1e-9
    assert abs(moving.get_center()[0] - 0.75) < 1e-6
    context.liveAdvanceSegmentTo(endpoint)
    context.liveCompleteSegment()
finally:
    _manim_reactive._leave_authoring_scene(timing_token)
_timing_result_json = json.dumps(_timing_rows)
`);
  report.flatTextPlayTiming = JSON.parse(pyodide.globals.get("_timing_result_json"));
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

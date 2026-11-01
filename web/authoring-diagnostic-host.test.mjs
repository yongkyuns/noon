import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { withAuthoringDiagnostics, diagnosedHostCode } from "./authoring-diagnostic-runtime.js";

// Execute the production worker's exact fixed host script in CPython with the
// real portable source compiler. Only engine bindings are doubled. This is
// host-language proof, not Pyodide/renderer qualification; the existing browser
// authoring-recovery smoke owns that end-to-end path.
const worker = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const start = worker.indexOf("async function runAuthoringSource(");
const end = worker.indexOf("async function runCanonicalCallbackPhase(", start);
assert.ok(start >= 0 && end > start, "production source entrypoint must exist");
const run = new Function("withAuthoringDiagnostics", "diagnosedHostCode", "prepareAuthoringDependencies",
  `const activeAuthoringRun = {}; let authoringLibrariesPristine = true;
  ${worker.slice(start, end)}; return runAuthoringSource;`,
)(withAuthoringDiagnostics, diagnosedHostCode, async () => {});

const scripts = {};
const globals = new Map();
globals.destroy = () => {};
const constructor = () => globals;
constructor.destroy = () => {};
await run({
  globals: { get: () => constructor },
  runPython(code) {
    if (code.includes("source_is_scene_only(__noon_source)")) return false;
    scripts.compile = code;
  },
  runPythonAsync(code) { scripts.execute = code; return "fixture"; },
}, "from noon import Scene\nresult=Scene()\n", {
  playground: { selection_generation: 1, run_generation: 7 },
}, 0);
assert.equal(typeof scripts.compile, "string");
assert.equal(typeof scripts.execute, "string");

const cases = [
  ["scene result", "from noon import Scene\nresult=Scene()\n", null, null],
  ["construct once", "from noon import Scene\ncontext['events']=[]\nclass Demo(Scene):\n    def construct(self):\n        context['events'].append('once')\n", null, null],
  ["named syntax error", "label='😀'; result=(1+)\n", "SyntaxError", 1],
  ["module source line", "label='😀'\nmissing\n", "NameError", 2],
  ["construct source line", "from noon import Scene\nclass Demo(Scene):\n    def construct(self):\n        missing\n", "NameError", 4],
  ["source after await", "from noon import Scene\nimport asyncio\nclass Demo(Scene):\n    async def construct(self):\n        await asyncio.sleep(0)\n        missing\n", "NameError", 6],
  ["result validation", "result=42\n", "TypeError", null],
  ["fresh corrected run", "from noon import Scene\nresult=Scene()\n", null, null],
];
const python = String.raw`
import ast, asyncio, inspect, json, sys
from types import ModuleType
from unittest.mock import patch
from _manim_source_execution import bind_portable_construct
payload = json.load(sys.stdin)
class Scene: pass
class Context:
    def liveHandoffDuration(self): return 0.0
async def construct(scene, portable_constructs):
    method = bind_portable_construct(scene.construct, portable_constructs) or scene.construct
    result = method()
    if inspect.isawaitable(result): await result
async def barrier(*args, **kwargs):
    raise AssertionError("fixture does not qualify animation barriers")
def module(name, **values):
    result = ModuleType(name)
    vars(result).update(values)
    return result
stubs = {
    "noon": module("noon", Scene=Scene),
    "_manim_scene": module("_manim_scene", execute_construct=construct,
        await_source_barrier=barrier, await_module_source_barrier=barrier,
        execution_context=lambda scene: Context()),
    "_manim_updaters": module("_manim_updaters", canonical_callback_session_id=lambda scene: None),
    "js": module("js", noonRegisterSemanticExecution=lambda context: "fixture-context",
        noonSemanticContinuationGeneration=lambda context: None),
}
async def main():
    results = []
    for label, source, _, _ in payload["cases"]:
        emitted = []
        namespace = {"__noon_source": source, "__noon_filename": "noon-s1-r7.py",
                     "__noon_revision": "s1-r7", "__noon_context_json": "{}",
                     "__noon_capture_diagnostic": emitted.append}
        failure = None
        try:
            exec(compile(payload["scripts"]["compile"], "<host-compile>", "exec"), namespace)
            with patch.dict(sys.modules, stubs):
                code = compile(payload["scripts"]["execute"], "<host-execute>", "exec",
                               flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT)
                result = eval(code, namespace)
                if inspect.isawaitable(result): await result
        except BaseException as error:
            failure = {"type": type(error).__name__, "message": str(error)}
        results.append({"label": label, "failure": failure,
            "diagnostics": [json.loads(value) for value in emitted],
            "result": namespace.get("__noon_result_json"),
            "events": namespace.get("__noon_namespace", {}).get("context", {}).get("events")})
    print(json.dumps(results))
asyncio.run(main())
`;
const output = spawnSync("python3", ["-c", python], {
  encoding: "utf8", timeout: 15_000, maxBuffer: 1024 * 1024,
  input: JSON.stringify({ scripts, cases }),
  env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1",
    PYTHONPATH: fileURLToPath(new URL("./python/", import.meta.url)) },
});
assert.equal(output.error, undefined, String(output.error));
assert.equal(output.status, 0, output.stderr);
const results = JSON.parse(output.stdout);
assert.equal(results.length, cases.length);
for (const [index, [label, , expectedType, line]] of cases.entries()) {
  test(`production host script: ${label}`, () => {
    const result = results[index];
    assert.equal(result.label, label);
    assert.equal(result.failure?.type ?? null, expectedType);
    if (expectedType === null) {
      assert.deepEqual(result.diagnostics, []);
      assert.deepEqual(JSON.parse(result.result), {
        kind: "semantic_scene", duration: 0,
        semantic_execution: { context_id: "fixture-context", callback_session_id: null },
      });
      if (label === "construct once") assert.deepEqual(result.events, ["once"]);
    } else {
      assert.equal(result.diagnostics.length, 1);
      const diagnostic = result.diagnostics[0];
      assert.equal(diagnostic.exception.type, expectedType);
      assert.equal(diagnostic.exception.location?.line ?? null, line);
      assert.deepEqual(diagnostic.document, { filename: "noon-s1-r7.py", revision: "s1-r7" });
      assert.equal(diagnostic.phase, expectedType === "SyntaxError" ? "compile" : "construct");
      if (label === "result validation") {
        assert.equal(result.failure.message, "Python authoring result must be a noon.Scene");
      }
    }
  });
}

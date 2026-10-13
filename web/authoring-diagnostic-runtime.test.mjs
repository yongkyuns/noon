import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  authoringSourceDocument, diagnosedHostCode, withAuthoringDiagnostics, authoringDiagnosticPayload,
} from "./authoring-diagnostic-runtime.js";

const produced = spawnSync("python3", ["-c", `
import json
from _noon_diagnostics import capture_exception
source = "label = '😀한글'; missing\\n"
try:
    exec(compile(source, "noon-s1-r7.py", "exec"), {})
except Exception as error:
    print(json.dumps(capture_exception(error, source=source, filename="noon-s1-r7.py",
                                     revision="s1-r7", phase="construct")))
`], { encoding: "utf8", timeout: 15000,
  env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1",
    PYTHONPATH: fileURLToPath(new URL("./python", import.meta.url)) } });
assert.equal(produced.status, 0, produced.stderr);
const fixture = JSON.parse(produced.stdout);
const context = { playground: { selection_generation: 1, run_generation: 7 } };

function fakePyodide(onCompile = () => {}) {
  const calls = [];
  const created = [];
  const pyodide = { globals: { get(name) {
    assert.equal(name, "dict");
    const construct = () => {
      const globals = new Map();
      globals.destroyed = false;
      globals.destroy = () => { globals.destroyed = true; calls.push("globals.destroy"); };
      created.push(globals);
      return globals;
    };
    construct.destroy = () => calls.push("dict.destroy");
    return construct;
  } }, runPython(code, { globals }) {
    calls.push("compile");
    assert.match(code, /compile_authoring_source\([\s\S]*filename=__noon_filename/);
    assert.match(code, /phase="compile"/);
    onCompile(globals, code);
  } };
  return { pyodide, calls, created };
}
const emit = (globals, value = fixture) => globals.get("__noon_capture_diagnostic")(JSON.stringify(value));

test("UI and worker derive the same immutable source document", () => {
  const ui = authoringSourceDocument(context);
  const worker = authoringSourceDocument(context, 24);
  assert.deepEqual(ui, worker);
  assert.deepEqual(worker, fixture.document);
  assert.ok(Object.isFrozen(worker));
});

test("non-playground callers use a bounded request identity", () => {
  assert.deepEqual(authoringSourceDocument({}, 2), { filename: "noon-request-2.py", revision: "request-2" });
});

for (const requestId of [undefined, -1, true, Infinity, "7"]) {
  test(`invalid request identity ${String(requestId)} is rejected`, () => {
    assert.throws(() => authoringSourceDocument({}, requestId), TypeError);
  });
}

test("successful return is unchanged; owned proxies are released exactly once", async () => {
  const { pyodide, calls, created } = fakePyodide();
  const result = {};
  assert.equal(await withAuthoringDiagnostics(pyodide, "result = 42", context, 2, async globals => {
    assert.equal(globals.get("__noon_filename"), fixture.document.filename);
    assert.equal(globals.get("__noon_context_json"), JSON.stringify(context));
    assert.equal(globals.destroyed, false);
    return result;
  }), result);
  assert.deepEqual(calls, ["dict.destroy", "compile", "globals.destroy"]);
  assert.equal(created[0].destroyed, true);
});

test("compile failure keeps original Error and never executes source", async () => {
  const failure = new Error("original traceback");
  const { pyodide, calls } = fakePyodide(globals => { emit(globals); throw failure; });
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 2,
    () => assert.fail("execute on compile failure")), error => error === failure);
  assert.deepEqual(failure.authoringDiagnostic, fixture);
  assert.ok(Object.isFrozen(failure.authoringDiagnostic));
  assert.equal(failure.message, "original traceback");
  assert.deepEqual(calls, ["dict.destroy", "compile", "globals.destroy"]);
});

test("construction failure keeps snapshot across an await", async () => {
  const failure = new Error("Python failed");
  const { pyodide } = fakePyodide();
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 2, async globals => {
    await Promise.resolve();
    emit(globals);
    throw failure;
  }), error => error === failure);
  assert.deepEqual(failure.authoringDiagnostic, fixture);
});

test("dependency/JS failures remain raw instead of receiving a stale Python error", async () => {
  const { pyodide } = fakePyodide();
  const first = new Error("first");
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 1, async globals => {
    emit(globals); throw first;
  }));
  const second = new Error("network offline");
  await assert.rejects(withAuthoringDiagnostics(pyodide, "pass", context, 2, async () => {
    throw second;
  }), error => error === second);
  assert.equal(second.authoringDiagnostic, undefined);
});

test("simultaneous request sinks are independent", async () => {
  const { pyodide } = fakePyodide();
  const errors = [new Error("a"), new Error("b")];
  await Promise.all(errors.map((failure, index) => assert.rejects(
    withAuthoringDiagnostics(pyodide, "missing", context, index, async globals => {
      const diagnostic = structuredClone(fixture);
      diagnostic.exception.message = String(index);
      emit(globals, diagnostic);
      await Promise.resolve();
      throw failure;
    }), error => error === failure,
  )));
  assert.deepEqual(errors.map(error => error.authoringDiagnostic.exception.message), ["0", "1"]);
});

test("source identity mismatch cannot be attached to another run", async () => {
  const { pyodide } = fakePyodide();
  const failure = new Error("original");
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", {}, 2, async globals => {
    emit(globals); throw failure;
  }));
  assert.equal(failure.authoringDiagnostic, undefined);
});

for (const bad of ["bad json", "null", '{"version":7}', JSON.stringify({ ...fixture, version: 2 })]) {
  test(`malformed capture falls back without masking original (${bad.slice(0, 12)})`, async () => {
    const { pyodide } = fakePyodide();
    const failure = new Error("original");
    await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 2, async globals => {
      globals.get("__noon_capture_diagnostic")(bad); throw failure;
    }), error => error === failure);
    assert.equal(failure.authoringDiagnostic, undefined);
  });
}

test("a frozen WASM trap is not converted to a normal Python failure", async () => {
  const failure = Object.freeze(new WebAssembly.RuntimeError("unreachable"));
  const { pyodide } = fakePyodide();
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 2, async globals => {
    emit(globals); throw failure;
  }), error => error === failure && error instanceof WebAssembly.RuntimeError);
});

test("primitive rejection is propagated exactly", async () => {
  const { pyodide } = fakePyodide();
  await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", context, 2, async globals => {
    emit(globals); throw "original rejection";
  }), error => error === "original rejection");
});

test("payload extension is absent for an ordinary error", () => {
  assert.deepEqual(authoringDiagnosticPayload(new Error("raw")), {});
});

test("payload extension validates and copies the existing snapshot", () => {
  const payload = authoringDiagnosticPayload({ authoringDiagnostic: fixture });
  assert.deepEqual(payload.authoringDiagnostic, fixture);
  assert.notEqual(payload.authoringDiagnostic, fixture);
  assert.ok(Object.isFrozen(payload.authoringDiagnostic));
});

test("host wrapper returns explicit result and never interpolates user source", async () => {
  const code = diagnosedHostCode('value = "safe"\n__noon_result_json = value');
  assert.match(code, /phase="construct"/);
  assert.ok(code.endsWith("\n__noon_result_json"));
  const { pyodide } = fakePyodide();
  await withAuthoringDiagnostics(pyodide, "` ${arbitrary} \"'", context, 2, async globals => {
    assert.equal(globals.get("__noon_source"), "` ${arbitrary} \"'");
  });
});

test("cleanup failure does not replace an in-flight original exception", async () => {
  const failure = new Error("original");
  const { pyodide } = fakePyodide(globals => {
    globals.destroy = () => { throw new Error("cleanup"); };
  });
  await assert.rejects(withAuthoringDiagnostics(pyodide, "pass", context, 1,
    async () => { throw failure; }), error => error === failure);
});

test("diagnostic getters cannot break the generic error channel", () => {
  const failure = { get authoringDiagnostic() { throw new Error("getter"); } };
  assert.deepEqual(authoringDiagnosticPayload(failure), {});
});

test("reused error objects cannot retain a previous run's source diagnostic", async () => {
  const { pyodide } = fakePyodide();
  const failure = new Error("reused host failure");
  for (const generation of [7, 8]) {
    const nextContext = { playground: { selection_generation: 1, run_generation: generation } };
    const diagnostic = structuredClone(fixture);
    diagnostic.document = authoringSourceDocument(nextContext);
    await assert.rejects(withAuthoringDiagnostics(pyodide, "missing", nextContext, generation,
      async globals => { emit(globals, diagnostic); throw failure; }), error => error === failure);
    assert.deepEqual(failure.authoringDiagnostic.document, diagnostic.document);
  }
  await assert.rejects(withAuthoringDiagnostics(pyodide, "pass", context, 9,
    async () => { throw failure; }), error => error === failure);
  assert.equal(failure.authoringDiagnostic, undefined);
  assert.deepEqual(authoringDiagnosticPayload(failure), {});
});

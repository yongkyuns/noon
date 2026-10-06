import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  qualifyUnsupportedJspi, unsupportedJspiSource, verifyUnsupportedJspiSource,
} from "../scripts/shared-authoring-jspi.mjs";

const expected = "RuntimeError: ordinary synchronous canonical play/wait requires Pyodide JS Promise Integration";
function fixture({ wrapped = true, rejection = { error: expected, terminated: false },
  recovery = { semanticExecution: true, terminated: false }, recoveryError = null, pageError = null } = {}) {
  const events = [];
  let reportPageError;
  const page = {
    on(event, handler) {
      assert.equal(event, "pageerror");
      reportPageError = handler;
    },
    async goto(url, options) {
      events.push(["goto", url, options]);
    },
    async evaluate(_function, source) {
      if (source === undefined) {
        events.push(["initialize"]);
        return wrapped;
      }
      if (source === unsupportedJspiSource) {
        events.push(["reject"]);
        return rejection;
      }
      assert.equal(source, verifyUnsupportedJspiSource);
      events.push(["verify"]);
      if (recoveryError) throw recoveryError;
      if (pageError) reportPageError(pageError);
      return recovery;
    },
  };
  const context = {
    async addInitScript(source) {
      assert.equal(typeof source, "function");
      events.push(["init-script"]);
    },
    async route(pattern, handler) {
      assert.equal(pattern, "**/python-worker-no-jspi-test.js");
      events.push(["route"]);
      await handler({
        async fulfill(response) {
          assert.deepEqual(response.headers, {
            "Cross-Origin-Embedder-Policy": "require-corp",
            "Cross-Origin-Resource-Policy": "same-origin",
          });
          assert.match(response.body, /delete WebAssembly\.promising;/);
          assert.match(response.body, /delete WebAssembly\.Suspending;/);
          assert.match(response.body, /await import\('\.\/python-worker\.js'\)/);
        },
      });
    },
    async newPage() { events.push(["new-page"]); return page; },
    async close() { events.push(["close"]); },
  };
  const browser = {
    async newContext(options) {
      assert.deepEqual(options, { viewport: { width: 800, height: 500 } });
      events.push(["new-context"]);
      return context;
    },
  };
  return { browser, events };
}

test("no-JSPI rejection uses an isolated production worker and verifies recovery", async () => {
  const { browser, events } = fixture();
  await qualifyUnsupportedJspi(browser, "http://test.invalid:4191");
  assert.deepEqual(events.map(([name]) => name), [
    "new-context", "init-script", "route", "new-page", "goto",
    "initialize", "reject", "verify", "close",
  ]);
  assert.deepEqual(events.find(([name]) => name === "goto"),
    ["goto", "http://test.invalid:4191/web/execution-worker-smoke.html", { waitUntil: "load" }]);
});

test("a missing worker interception fails before executing Python", async () => {
  const { browser, events } = fixture({ wrapped: false });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /did not wrap the production worker/);
  assert.equal(events.some(([name]) => name === "reject"), false);
  assert.equal(events.at(-1)[0], "close");
});

test("a wrong original rejection is not masked by a second authoring request", async () => {
  const { browser, events } = fixture({
    rejection: { error: "Python source suspended without a semantic continuation consumer", terminated: true },
  });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /did not fail at the JSPI admission gate/);
  assert.equal(events.some(([name]) => name === "verify"), false);
  assert.equal(events.at(-1)[0], "close");
});

test("an unexpected successful synchronous segment is not a capability pass", async () => {
  const { browser, events } = fixture({ rejection: { error: null, terminated: false } });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /did not fail at the JSPI admission gate/);
  assert.equal(events.some(([name]) => name === "verify"), false);
  assert.equal(events.at(-1)[0], "close");
});

test("an admission error cannot pass after killing its authoring worker", async () => {
  const { browser, events } = fixture({ rejection: { error: expected, terminated: true } });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /leave its Python authoring worker usable/);
  assert.equal(events.some(([name]) => name === "verify"), false);
  assert.equal(events.at(-1)[0], "close");
});

test("post-rejection state failure remains visible and closes the isolated context", async () => {
  const error = new Error("assert context.authoredDuration() == 0 failed");
  const { browser, events } = fixture({ recoveryError: error });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    failure => failure === error);
  assert.equal(events.at(-1)[0], "close");
});

test("a malformed recovery cannot pass", async () => {
  const { browser } = fixture({ recovery: { semanticExecution: false, terminated: false } });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /changed scene state or prevented another source invocation/);
});

test("an unexpected page error cannot hide behind correct source rejection", async () => {
  const { browser, events } = fixture({ pageError: new Error("page owner failed") });
  await assert.rejects(qualifyUnsupportedJspi(browser, "http://test.invalid"),
    /unexpected no-JSPI page error/);
  assert.equal(events.at(-1)[0], "close");
});

test("the real source compiler retains the no-JSPI test's synchronous helper stack", () => {
  const python = process.env.PYTHON ?? "python3";
  const probe = `
import ast, inspect, json, sys
sys.path.insert(0, sys.argv[1])
from _manim_source_execution import compile_authoring_source
sources = json.load(sys.stdin)
code, constructs = compile_authoring_source(sources[0], "<no-jspi-rejection>", portable=True)
assert not constructs, "no-JSPI test was lowered to portable async"
assert not code.co_flags & inspect.CO_COROUTINE, "test module must keep its synchronous stack"
for source in sources:
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if isinstance(node, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            for target in targets:
                assert not (isinstance(target, ast.Attribute) and target.attr in {"can_run_sync", "can_wait_sync"}), "late capability monkeypatch"
compile_authoring_source(sources[1], "<no-jspi-recovery>", portable=True)
print("original synchronous helper; no post-import capability mutation")
`;
  const result = spawnSync(python, [
    "-I", "-S", "-c", probe, fileURLToPath(new URL("../web/python", import.meta.url)),
  ], { input: JSON.stringify([unsupportedJspiSource, verifyUnsupportedJspiSource]), encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr ?? String(result.error));
  assert.match(result.stdout, /original synchronous helper/);
});

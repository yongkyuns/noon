import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const start = source.indexOf("async function runAuthoringSource(");
const end = source.indexOf("\nasync function runCanonicalCallbackPhase(", start);
assert.ok(start >= 0 && end > start, "production source-run boundary must exist");
// Exercise the production function without initializing WASM, a worker or Pyodide.
// Only the interpreter interface is mocked; no duplicate source runner is tested.
const runAuthoringSource = vm.runInNewContext(`(${source.slice(start, end)})`);

function interpreter({ load, loadPackage: loadOptional, execute, implicitPackages } = {}) {
  const events = [];
  const namespaces = [];
  const loadedPackages = new Set();
  let currentSource = "";
  let requestedPackages = [];
  const pyodide = {
    async loadPackagesFromImports(text) {
      currentSource = text;
      events.push(["packages", text]);
      await load?.(text);
    },
    async loadPackage(packages) {
      const requested = [...packages];
      events.push(["load-package", requested]);
      await loadOptional?.(requested);
      for (const packageName of requested) loadedPackages.add(packageName);
    },
    runPython(code) {
      if (code.includes("required_packages_json")) {
        requestedPackages = [...(implicitPackages?.(currentSource) ?? [])];
        return JSON.stringify(requestedPackages);
      }
      if (code.includes("missing_packages_json")) {
        return JSON.stringify(
          requestedPackages.filter(packageName => !loadedPackages.has(packageName)),
        );
      }
      if (code.includes("bind_loaded_packages_json")) {
        events.push(["bind-packages", [...requestedPackages]]);
        return null;
      }
      throw new Error(`unexpected synchronous Python source: ${code}`);
    },
    globals: {
      get(name) {
        assert.equal(name, "dict");
        events.push(["dict"]);
        const constructor = () => {
          const values = new Map();
          const namespace = { values, destroyed: false,
            set: (key, value) => values.set(key, value),
            destroy() { this.destroyed = true; events.push(["destroy"]); },
          };
          namespaces.push(namespace);
          return namespace;
        };
        constructor.destroy = () => events.push(["constructor-destroy"]);
        return constructor;
      },
    },
    async runPythonAsync(bootstrap, { globals }) {
      events.push(["execute", globals.values.get("__noon_source")]);
      assert.match(bootstrap, /await execute_authoring_module\(__noon_code, __noon_namespace\)/);
      return execute ? execute(bootstrap, globals) : "source-result";
    },
  };
  return { pyodide, events, namespaces, loadedPackages };
}

const numpyScene = "import numpy as np\nfrom noon import *\nclass Demo(Scene):\n    def construct(self):\n        self.add(Dot([np.sin(1), 0, 0]))\n";
const implicitNumpyScene = "from noon import *\nclass Demo(Scene):\n    def construct(self):\n        self.add(Dot([np.sin(1), 0, 0]))\n";

test("source imports finish loading before namespace allocation or authored effects", async () => {
  let release;
  const loading = new Promise(resolve => { release = resolve; });
  const state = interpreter({ load: () => loading });
  const pending = runAuthoringSource(state.pyodide, numpyScene, { selected: "Demo" });
  await Promise.resolve();
  assert.deepEqual(state.events, [["packages", numpyScene]]);
  assert.equal(state.namespaces.length, 0);
  release();
  assert.equal(await pending, "source-result");
  assert.deepEqual(state.events.map(event => event[0]),
    ["packages", "dict", "constructor-destroy", "execute", "destroy"]);
  assert.equal(state.namespaces[0].values.get("__noon_source"), numpyScene);
  assert.equal(state.namespaces[0].values.get("__noon_context_json"), '{"selected":"Demo"}');
  assert.equal(state.namespaces[0].destroyed, true);
});

test("package-load failure preserves its error and never executes user source", async () => {
  const failure = new Error("package download failed");
  const state = interpreter({ load: async () => { throw failure; } });
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), error => error === failure);
  assert.deepEqual(state.events, [["packages", numpyScene]]);
  assert.equal(state.namespaces.length, 0);
});

test("a failed package load does not poison a later source run", async () => {
  let attempts = 0;
  const state = interpreter({ load: () => {
    if (++attempts === 1) throw new Error("temporary package failure");
  } });
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), /temporary package failure/);
  assert.equal(await runAuthoringSource(state.pyodide, numpyScene, {}), "source-result");
  assert.equal(attempts, 2);
  assert.equal(state.events.filter(([kind]) => kind === "execute").length, 1);
  assert.equal(state.namespaces.length, 1);
  assert.equal(state.namespaces[0].destroyed, true);
});

test("synchronous loader errors also stop before authoring", async () => {
  const state = interpreter();
  const failure = new Error("invalid import syntax");
  state.pyodide.loadPackagesFromImports = () => { throw failure; };
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), error => error === failure);
  assert.deepEqual(state.events, []);
});

test("source execution failure still destroys its namespace exactly once", async () => {
  const failure = new Error("authored exception");
  const state = interpreter({ execute: () => { throw failure; } });
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), error => error === failure);
  assert.equal(state.namespaces[0].destroyed, true);
  assert.equal(state.events.filter(([kind]) => kind === "destroy").length, 1);
});

test("each run delegates its original imports without installing a fixed package set", async () => {
  const state = interpreter();
  const sources = ["from noon import *\nresult = Scene()\n", numpyScene,
    "from numpy import sin\nfrom noon import *\nresult = Scene()\n"];
  for (const text of sources) await runAuthoringSource(state.pyodide, text, {});
  assert.deepEqual(state.events.filter(([kind]) => kind === "packages").map(([, text]) => text), sources);
  assert.deepEqual(state.events.filter(([kind]) => kind === "execute").map(([, text]) => text), sources);
  assert.equal(state.namespaces.length, sources.length);
});

test("implicit Manim namespace packages load once before authoring and bind the real module", async () => {
  const state = interpreter({
    implicitPackages: () => ["numpy"],
  });
  assert.equal(await runAuthoringSource(state.pyodide, implicitNumpyScene, {}), "source-result");
  assert.deepEqual(state.events.filter(([kind]) => kind === "load-package"), [
    ["load-package", ["numpy"]],
  ]);
  const bindIndex = state.events.findIndex(([kind]) => kind === "bind-packages");
  const executeIndex = state.events.findIndex(([kind]) => kind === "execute");
  assert.ok(bindIndex >= 0 && bindIndex < executeIndex, "implicit module binds before user source");

  assert.equal(await runAuthoringSource(state.pyodide, implicitNumpyScene, {}), "source-result");
  assert.equal(
    state.events.filter(([kind]) => kind === "load-package").length,
    1,
    "the interpreter package cache prevents a second optional download",
  );
  assert.equal(state.events.filter(([kind]) => kind === "bind-packages").length, 2);
});

test("implicit optional package failure preserves its error and never executes user source", async () => {
  const failure = new Error("implicit package download failed");
  const state = interpreter({
    implicitPackages: () => ["numpy"],
    loadPackage: async () => { throw failure; },
  });
  await assert.rejects(
    runAuthoringSource(state.pyodide, implicitNumpyScene, {}),
    error => error === failure,
  );
  assert.equal(state.events.filter(([kind]) => kind === "execute").length, 0);
  assert.equal(state.namespaces.length, 0);
});

test("package discovery is source-startup work, never worker initialization or callbacks", () => {
  assert.equal(source.slice(0, start).includes("loadPackagesFromImports"), false);
  assert.equal(source.slice(end).includes("loadPackagesFromImports"), false);
  assert.equal(source.match(/loadPackagesFromImports\(/g)?.length, 1);
  assert.equal(source.match(/loadPackage\(/g)?.length, 1);
  assert.equal(source.includes('loadPackage("numpy")'), false);
});

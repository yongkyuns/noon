import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const start = source.indexOf("async function prepareAuthoringDependencies(");
const end = source.indexOf("\nasync function runCanonicalCallbackPhase(", start);
assert.ok(start >= 0 && end > start, "production source-startup boundary must exist");
// Exercise the production dependency preparation + source runner without
// initializing WASM, a worker or Pyodide. Only the interpreter interface is mocked.
const { prepareAuthoringDependencies, runAuthoringSource } = vm.runInNewContext(
  `(() => {
${source.slice(start, end)}
return { prepareAuthoringDependencies, runAuthoringSource };
})()`,
);

function interpreter({ load, execute, requiredPackages = [], missingPackages } = {}) {
  const events = [];
  const namespaces = [];
  const pyodide = {
    async loadPackagesFromImports(text) {
      events.push(["packages", text]);
      await load?.(text);
    },
    async loadPackage(packages) {
      events.push(["load-package", Array.from(packages)]);
    },
    globals: {
      get(name) {
        assert.equal(name, "dict");
        events.push(["dict"]);
        const constructor = () => {
          const values = new Map();
          const namespace = {
            values,
            destroyed: false,
            set: (key, value) => values.set(key, value),
            destroy() {
              this.destroyed = true;
              events.push(["destroy"]);
            },
          };
          namespaces.push(namespace);
          return namespace;
        };
        constructor.destroy = () => events.push(["constructor-destroy"]);
        return constructor;
      },
    },
    runPython(script, { globals }) {
      if (script.includes("_manim_namespace.required_packages_json(")) {
        events.push(["implicit-required", globals.values.get("__noon_dependency_source")]);
        return JSON.stringify(requiredPackages);
      }
      if (script.includes("_manim_namespace.missing_packages_json(")) {
        const requested = JSON.parse(globals.values.get("__noon_required_packages_json"));
        events.push(["implicit-missing", requested]);
        return JSON.stringify(missingPackages ?? requested);
      }
      if (script.includes("_manim_namespace.bind_loaded_packages_json(")) {
        const requested = JSON.parse(globals.values.get("__noon_required_packages_json"));
        events.push(["implicit-bind", requested]);
        return undefined;
      }
      throw new Error("unexpected runPython call in source-startup test");
    },
    async runPythonAsync(bootstrap, { globals }) {
      events.push(["execute", globals.values.get("__noon_source")]);
      // Both Python hosts use the common source lifecycle. This worker owns
      // only package readiness and its bridge namespace, not another compiler.
      assert.match(bootstrap, /from _noon_source import execute_source/);
      assert.match(bootstrap, /await execute_source\(\s*__noon_source, json\.loads\(__noon_context_json\)\s*\)/);
      assert.equal(bootstrap.match(/await execute_source\(/g)?.length, 1);
      assert.doesNotMatch(bootstrap, /compile_authoring_source|execute_authoring_module|execute_construct/);
      return execute ? execute(bootstrap, globals) : "source-result";
    },
  };
  return { pyodide, events, namespaces };
}

const numpyScene =
  "import numpy as np\nfrom noon import *\nclass Demo(Scene):\n" +
  "    def construct(self):\n        self.add(Dot([np.sin(1), 0, 0]))\n";
const implicitNumpyScene =
  "from noon import *\nclass Demo(Scene):\n" +
  "    def construct(self):\n        self.add(Dot([np.sin(1), 0, 0]))\n";

function authoringNamespaces(state) {
  return state.namespaces.filter(namespace => namespace.values.has("__noon_source"));
}

test("explicit source imports finish loading before dependency inspection or authored effects", async () => {
  let release;
  const loading = new Promise(resolve => { release = resolve; });
  const state = interpreter({ load: () => loading });
  const pending = runAuthoringSource(state.pyodide, numpyScene, { selected: "Demo" });
  await Promise.resolve();
  assert.deepEqual(state.events, [["packages", numpyScene]]);
  assert.equal(state.namespaces.length, 0);
  release();
  assert.equal(await pending, "source-result");

  const kinds = state.events.map(event => event[0]);
  assert.deepEqual(kinds, [
    "packages",
    "dict",
    "constructor-destroy",
    "implicit-required",
    "destroy",
    "dict",
    "constructor-destroy",
    "execute",
    "destroy",
  ]);
  const [namespace] = authoringNamespaces(state);
  assert.equal(namespace.values.get("__noon_source"), numpyScene);
  assert.equal(namespace.values.get("__noon_context_json"), '{"selected":"Demo"}');
  assert.equal(namespace.destroyed, true);
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
  const state = interpreter({
    load: () => {
      if (++attempts === 1) throw new Error("temporary package failure");
    },
  });
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), /temporary package failure/);
  assert.equal(await runAuthoringSource(state.pyodide, numpyScene, {}), "source-result");
  assert.equal(attempts, 2);
  assert.equal(state.events.filter(([kind]) => kind === "execute").length, 1);
  assert.equal(authoringNamespaces(state).length, 1);
  assert.equal(authoringNamespaces(state)[0].destroyed, true);
});

test("synchronous loader errors also stop before dependency inspection and authoring", async () => {
  const state = interpreter();
  const failure = new Error("invalid import syntax");
  state.pyodide.loadPackagesFromImports = () => { throw failure; };
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), error => error === failure);
  assert.deepEqual(state.events, []);
});

test("source execution failure still destroys its authoring namespace exactly once", async () => {
  const failure = new Error("authored exception");
  const state = interpreter({ execute: () => { throw failure; } });
  await assert.rejects(runAuthoringSource(state.pyodide, numpyScene, {}), error => error === failure);
  const [namespace] = authoringNamespaces(state);
  assert.equal(namespace.destroyed, true);
  // One temporary dependency namespace and one authoring namespace are each destroyed.
  assert.equal(state.events.filter(([kind]) => kind === "destroy").length, 2);
});

test("each run delegates its original explicit imports without installing a fixed package set", async () => {
  const state = interpreter();
  const sources = [
    "from noon import *\nresult = Scene()\n",
    numpyScene,
    "from numpy import sin\nfrom noon import *\nresult = Scene()\n",
  ];
  for (const text of sources) await runAuthoringSource(state.pyodide, text, {});
  assert.deepEqual(
    state.events.filter(([kind]) => kind === "packages").map(([, text]) => text),
    sources,
  );
  assert.deepEqual(
    state.events.filter(([kind]) => kind === "execute").map(([, text]) => text),
    sources,
  );
  assert.equal(authoringNamespaces(state).length, sources.length);
});

test("implicit Manim aliases load and bind the real package before user execution", async () => {
  const state = interpreter({
    requiredPackages: ["numpy"],
    missingPackages: ["numpy"],
  });
  assert.equal(
    await runAuthoringSource(state.pyodide, implicitNumpyScene, {}),
    "source-result",
  );
  const kinds = state.events.map(event => event[0]);
  assert.ok(kinds.indexOf("packages") < kinds.indexOf("implicit-required"));
  assert.ok(kinds.indexOf("implicit-required") < kinds.indexOf("implicit-missing"));
  assert.ok(kinds.indexOf("implicit-missing") < kinds.indexOf("load-package"));
  assert.ok(kinds.indexOf("load-package") < kinds.indexOf("implicit-bind"));
  assert.ok(kinds.indexOf("implicit-bind") < kinds.indexOf("execute"));
  assert.deepEqual(
    state.events.find(([kind]) => kind === "load-package"),
    ["load-package", ["numpy"]],
  );
});

test("already-loaded implicit packages are bound without requesting another download", async () => {
  const state = interpreter({
    requiredPackages: ["numpy"],
    missingPackages: [],
  });
  await prepareAuthoringDependencies(state.pyodide, implicitNumpyScene);
  assert.equal(state.events.some(([kind]) => kind === "load-package"), false);
  assert.deepEqual(
    state.events.find(([kind]) => kind === "implicit-bind"),
    ["implicit-bind", ["numpy"]],
  );
});

test("package discovery remains source-startup work, never worker initialization or callbacks", () => {
  assert.equal(source.slice(0, start).includes("loadPackagesFromImports"), false);
  assert.equal(source.slice(end).includes("loadPackagesFromImports"), false);
  assert.equal(source.match(/loadPackagesFromImports\(/g)?.length, 1);
  assert.equal(source.includes('loadPackage("numpy")'), false);
});

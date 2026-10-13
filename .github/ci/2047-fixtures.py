"""Keep the original worker test invariants with compile-before-load execution.

Temporary application helper for #2047 only; never part of the product commit.
"""
from pathlib import Path
import hashlib

EXPECTED = {
    "web/python-worker-packages.test.mjs": "725eb36a3b93c88e9b2d2f4c16496d61e0f203de",
    "web/python-worker-source.test.mjs": "3a9a8df1786ac5ccf91197fb5e8c42aea518ef85",
}
for name, expected in EXPECTED.items():
    data = Path(name).read_bytes()
    actual = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    assert actual == expected, f"{name} changed: {actual}; refusing overwrite"


def replace(source, old, new):
    assert source.count(old) == 1, f"Expected unique anchor: {old[:120]}"
    return source.replace(old, new)


path = Path("web/python-worker-packages.test.mjs")
s = path.read_text()
s = replace(s, 'import vm from "node:vm";', 'import vm from "node:vm";\nimport { attachAuthoringDiagnostic } from "./authoring-error.js";')
s = replace(s, '})()`,\n);', '})()`,\n  { attachAuthoringDiagnostic },\n);')
s = replace(s, '            set: (key, value) => values.set(key, value),', '            set: (key, value) => values.set(key, value),\n            get: (key) => values.get(key),')
s = replace(s, '    async runPythonAsync(bootstrap, { globals }) {', '''    async runPythonAsync(bootstrap, { globals }) {
      // Model the production coroutine's dependency await. Actual Python
      // compilation/capture is exercised in test_noon_worker_diagnostics.py.
      events.push(["compile"]);
      assert.ok(bootstrap.indexOf("compile_authoring_source(") < bootstrap.indexOf("await __noon_prepare_dependencies()"));
      assert.ok(bootstrap.indexOf("await __noon_prepare_dependencies()") < bootstrap.indexOf("await execute_authoring_module("));
      await globals.values.get("__noon_prepare_dependencies")();''')
s = replace(s, '''  assert.deepEqual(state.events, [["packages", numpyScene]]);
  assert.equal(state.namespaces.length, 0);
  release();''', '''  assert.deepEqual(state.events, [["dict"], ["constructor-destroy"], ["compile"], ["packages", numpyScene]]);
  assert.equal(authoringNamespaces(state).length, 1);
  assert.equal(authoringNamespaces(state)[0].destroyed, false);
  release();''')
s = replace(s, '''    "packages",
    "dict",
    "constructor-destroy",
    "implicit-required",
    "destroy",
    "dict",
    "constructor-destroy",
    "execute",
    "destroy",''', '''    "dict", "constructor-destroy", "compile", "packages",
    "dict", "constructor-destroy", "implicit-required", "destroy",
    "execute", "destroy",''')
s = replace(s, '''  assert.deepEqual(state.events, [["packages", numpyScene]]);
  assert.equal(state.namespaces.length, 0);''', '''  assert.deepEqual(state.events, [["dict"], ["constructor-destroy"], ["compile"], ["packages", numpyScene], ["destroy"]]);
  assert.equal(authoringNamespaces(state).length, 1);
  assert.equal(authoringNamespaces(state)[0].destroyed, true);''')
s = replace(s, '''  assert.equal(authoringNamespaces(state).length, 1);
  assert.equal(authoringNamespaces(state)[0].destroyed, true);
});

test("synchronous''', '''  assert.equal(authoringNamespaces(state).length, 2);
  assert.ok(authoringNamespaces(state).every(namespace => namespace.destroyed));
});

test("synchronous''')
s = replace(s, '  assert.deepEqual(state.events, []);', '  assert.deepEqual(state.events, [["dict"], ["constructor-destroy"], ["compile"], ["destroy"]]);')
path.write_text(s)

path = Path("web/python-worker-source.test.mjs")
s = path.read_text()
s = replace(s, r'  assert.match(source, /compile_authoring_source\(\s*__noon_source\s*\)/);', r'''  assert.match(source, /compile_authoring_source\(\s*__noon_source,\s*filename=__noon_source_document\["filename"\]\s*\)/);
  assert.equal(source.match(/compile_authoring_source\(/g)?.length, 1);''')
path.write_text(s)
print("Updated dependency and compiler fixtures without removing their ordering, cleanup or single-execution assertions.")

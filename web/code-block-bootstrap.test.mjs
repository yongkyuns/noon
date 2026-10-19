import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const bindings = source.match(/  self\.noonCodeOptions[\s\S]*?(?=  self\.noonTableFromFamily)/)?.[0];
assert.ok(bindings, "Code must be wired into the ordinary Python worker");
const calls = [];
const self = {};
vm.runInNewContext(bindings, {
  self,
  WasmCodeOptions: class { constructor(...args) { this.args = args; } },
  noonCodeStylesList: () => ["vim"],
  noonCodeLanguagesList: () => ["Python"],
  authoringStore: { createCode: (...args) => calls.push(["store", ...args]) },
});
const options = self.noonCodeOptions("python", undefined, "vim", 4, true, 1, "rectangle");
assert.equal(options.args[0], "python");
self.noonCreateAuthoringCodeHandle("raw\n\tsource", options, null);
assert.deepEqual(calls[0], ["store", "raw\n\tsource", options]);
const context = { liveCreateCode: (...args) => calls.push(["live", ...args]) };
self.noonCreateAuthoringCodeHandle("raw\n\tsource", options, context);
assert.deepEqual(calls[1], ["live", "raw\n\tsource", options]);
const family = { asCode: () => "retained code handle" };
assert.equal(self.noonCodeFromFamily(family), "retained code handle");
assert.deepEqual(self.noonCodeStylesList(), ["vim"]);
assert.deepEqual(self.noonCodeLanguagesList(), ["Python"]);
console.log("✓ Code worker forwards typed construction and retained family queries");

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
  WasmCodeHighlightOptions: class { constructor(...args) { this.args = args; } },
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
const regionOptions = self.noonCodeHighlightOptions(true);
assert.deepEqual(Array.from(regionOptions.args), [true]);
const code = {
  getHighlightedLines: (...args) => calls.push(["detached", ...args]),
  highlightLines: (...args) => calls.push(["attached", ...args]),
};
context.liveGetHighlightedCodeLines = (...args) => calls.push(["live detached", ...args]);
context.liveHighlightCodeLines = (...args) => calls.push(["live attached", ...args]);
self.noonGetHighlightedCodeLines(null, code, 1, undefined, regionOptions);
self.noonHighlightCodeLines(null, code, 1, 3, regionOptions);
self.noonGetHighlightedCodeLines(context, code, 1, undefined, regionOptions);
self.noonHighlightCodeLines(context, code, 1, 3, regionOptions);
assert.deepEqual(calls.slice(2), [
  ["detached", 1, undefined, regionOptions],
  ["attached", 1, 3, regionOptions],
  ["live detached", code, 1, undefined, regionOptions],
  ["live attached", code, 1, 3, regionOptions],
]);
console.log("✓ Code worker forwards construction, retained queries and cold/live line regions");

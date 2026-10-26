import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");
const binding = source.match(/  self\.noonTheme = [^\n]+;/)?.[0];
assert.ok(binding, "Theme must be available in the ordinary Python worker");
const calls = [];
const self = {};
class WasmTheme {
  constructor(preset) { calls.push(preset); }
}
vm.runInNewContext(binding, { self, WasmTheme });
assert.ok(self.noonTheme("light") instanceof WasmTheme);
assert.deepEqual(calls, ["light"]);
assert.ok(PYTHON_COMPAT_MODULES.some(module => module.sourcePath === "python/_noon_presentation.py" &&
  module.runtimePath === "/tmp/_noon_presentation.py"));

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const module = PYTHON_COMPAT_MODULES.find(
  ({ sourcePath }) => sourcePath === "python/_manim_complex_plane.py",
);
assert.ok(module, "ComplexPlane must be included in the Python worker bundle");
assert.equal(module.runtimePath, "/tmp/_manim_complex_plane.py");

const source = await readFile(path.join(root, "web/python/_manim_complex_plane.py"), "utf8");
assert.match(source, /from _manim_number_plane import NumberPlane/);
const example = await readFile(path.join(root, "web/python/examples/complex_plane.py"), "utf8");
assert.match(example, /ComplexPlane\(/);
assert.match(example, /n2p\(1 - 0\.65j\)/);
assert.match(example, /p2n\(point\)/);

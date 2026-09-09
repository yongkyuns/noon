import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("geometry methods are declared on Mobject without startup patching", async () => {
  const [base, compat, shared, modules] = await Promise.all([
    read("./python/noon.py"),
    read("./python/_manim_compat.py"),
    read("./python/_manim_shared_geometry.py"),
    read("./python-compat-modules.js"),
  ]);
  for (const name of ["set_coord", "match_coord", "match_x", "match_y", "rotate_about_origin"]) {
    assert.match(base, new RegExp(`    def ${name}\\(`));
    for (const source of [compat, shared]) {
      assert.doesNotMatch(source, new RegExp(`(?:_BaseMobject|_base\\.Mobject)\\.${name}\\s*=`));
    }
  }
  assert.doesNotMatch(modules, /_manim_mobject_transforms/);
});

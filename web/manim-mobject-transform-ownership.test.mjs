import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";

const adapterUrl = new URL("./python/_manim_mobject_transforms.py", import.meta.url);
const modulesUrl = new URL("./python-compat-modules.js", import.meta.url);

test("Mobject placement and origin rotation compose through shared semantics", async () => {
  const [adapter, modules] = await Promise.all([
    readFile(fileURLToPath(adapterUrl), "utf8"),
    readFile(fileURLToPath(modulesUrl), "utf8"),
  ]);

  assert.match(adapter, /def _center\(/);
  assert.match(adapter, /return self\.move_to\(_base\.ORIGIN\)/);
  assert.match(adapter, /_base\.Mobject\.center = _center/);
  assert.match(adapter, /_compat\.Group\.center = _center/);

  assert.match(adapter, /def _rotate_about_origin\(/);
  assert.match(
    adapter,
    /self\.rotate\(angle, axis=axis, about_point=_base\.ORIGIN, \*\*kwargs\)/,
  );
  assert.match(
    adapter,
    /_base\.Mobject\.rotate_about_origin = _rotate_about_origin/,
  );
  assert.match(
    adapter,
    /_compat\.Group\.rotate_about_origin = _rotate_about_origin/,
  );

  // These adapters are syntax-only: layout and transform math remain owned by
  // the shared semantic handle paths behind move_to/rotate.
  assert.doesNotMatch(adapter, /math\.(?:sin|cos)/);
  assert.doesNotMatch(adapter, /transform\s*\[/);
  assert.doesNotMatch(adapter, /get_center\(\)/);

  const semanticIndex = modules.indexOf("python/_manim_semantic_handles.py");
  const transformIndex = modules.indexOf("python/_manim_mobject_transforms.py");
  assert.ok(semanticIndex >= 0, "semantic handle module must be registered");
  assert.ok(
    transformIndex > semanticIndex,
    "transform adapters must load after shared semantic handles",
  );
});

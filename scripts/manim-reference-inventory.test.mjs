import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import {
  buildReferenceInventory,
  extractManimDirectives,
} from "./manim-reference-inventory.mjs";

test("extracts directive options and source from Python docstrings", () => {
  const input = `class Arc:\n    \"\"\"Example.\n\n    .. manim:: ArcExample\n        :save_last_frame:\n        :quality: low_quality\n\n        class ArcExample(Scene):\n            def construct(self):\n                self.add(Arc(angle=PI))\n    \"\"\"\n`;
  const examples = extractManimDirectives(input, "manim/mobject/geometry/arc.py");
  assert.equal(examples.length, 1);
  assert.deepEqual(examples[0], {
    name: "ArcExample",
    source_path: "manim/mobject/geometry/arc.py",
    directive_line: 4,
    options: { save_last_frame: true, quality: "low_quality" },
    source: "class ArcExample(Scene):\n    def construct(self):\n        self.add(Arc(angle=PI))",
    source_sha256: examples[0].source_sha256,
  });
  assert.match(examples[0].source_sha256, /^[0-9a-f]{64}$/u);
});

test("supports different directive indentation and multiple examples", () => {
  const input = `.. manim:: First\n    :save_last_frame:\n\n    class First(Scene):\n        pass\n\nText between directives.\n\n    .. manim:: Second\n      class Second(Scene):\n          pass\n`;
  const examples = extractManimDirectives(input, "docs/source/example.rst");
  assert.deepEqual(
    examples.map((example) => example.name),
    ["First", "Second"],
  );
  assert.equal(examples[1].source, "class Second(Scene):\n    pass");
});

test("builds a deterministic code-point-ordered inventory across documentation sources", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "noon-manim-inventory-"));
  await mkdir(path.join(root, "manim", "z"), { recursive: true });
  await mkdir(path.join(root, "docs", "source"), { recursive: true });
  await writeFile(
    path.join(root, "manim", "z", "later.py"),
    `\"\"\"Examples\n\n.. manim:: Later\n    class Later(Scene):\n        pass\n\"\"\"\n`,
  );
  await writeFile(
    path.join(root, "docs", "source", "B.rst"),
    `.. manim:: UppercaseFirst\n\n    class UppercaseFirst(Scene):\n        pass\n`,
  );
  await writeFile(
    path.join(root, "docs", "source", "a.rst"),
    `.. manim:: LowercaseSecond\n\n    class LowercaseSecond(Scene):\n        pass\n`,
  );
  await writeFile(
    path.join(root, "docs", "source", "ignored.txt"),
    ".. manim:: Ignored\n",
  );

  const inventory = await buildReferenceInventory(root);
  assert.equal(inventory.schema_version, 1);
  assert.deepEqual(inventory.scanned_roots, ["docs/source", "manim"]);
  assert.deepEqual(
    inventory.examples.map((example) => [example.source_path, example.name]),
    [
      ["docs/source/B.rst", "UppercaseFirst"],
      ["docs/source/a.rst", "LowercaseSecond"],
      ["manim/z/later.py", "Later"],
    ],
  );
});

import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  extractFileExamples,
  extractManimReferenceExamples,
} from "./extract-manim-reference-examples.mjs";

test("extracts named and inline-class manim directives deterministically", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-manim-reference-"));
  await mkdir(path.join(root, "geometry"), { recursive: true });
  await writeFile(
    path.join(root, "zeta.rst"),
    `Heading
=======

.. manim:: ExplicitScene
    :save_last_frame:
    :ref_classes: Circle Square

    class ExplicitScene(Scene):
        pass
`,
    "utf8",
  );
  await writeFile(
    path.join(root, "geometry", "alpha.rst"),
    `Alpha
=====

.. manim::
    :quality: low_quality
    :ref_classes: Arc

    class InferredScene(Scene):
        def construct(self):
            pass
`,
    "utf8",
  );

  const first = await extractManimReferenceExamples(root);
  const second = await extractManimReferenceExamples(root);
  assert.deepEqual(second, first);
  assert.deepEqual(
    first.examples.map(({ source_path, source_line, scene, ref_classes }) => ({
      source_path,
      source_line,
      scene,
      ref_classes,
    })),
    [
      {
        source_path: "geometry/alpha.rst",
        source_line: 4,
        scene: "InferredScene",
        ref_classes: ["Arc"],
      },
      {
        source_path: "zeta.rst",
        source_line: 4,
        scene: "ExplicitScene",
        ref_classes: ["Circle", "Square"],
      },
    ],
  );
  assert.match(first.examples[0].source_sha256, /^[0-9a-f]{64}$/);
  assert.notEqual(first.examples[0].source_sha256, first.examples[1].source_sha256);
  assert.deepEqual(first.examples[0].directive_options, {
    quality: "low_quality",
    ref_classes: "Arc",
  });
  assert.equal(first.upstream.version, "v0.21.0");
  assert.equal(first.upstream.scope, "docs/source/reference");
});

test("source hash changes when the directive body changes", () => {
  const before = extractFileExamples(
    `.. manim:: Example\n\n    class Example(Scene):\n        value = 1\n`,
    "module.rst",
  );
  const after = extractFileExamples(
    `.. manim:: Example\n\n    class Example(Scene):\n        value = 2\n`,
    "module.rst",
  );
  assert.notEqual(before[0].source_sha256, after[0].source_sha256);
});

test("rejects anonymous directives without a scene class", () => {
  assert.throws(
    () => extractFileExamples(`.. manim::\n    :save_last_frame:\n`, "broken.rst"),
    /broken\.rst:1: manim directive has no scene name or class/,
  );
});

test("rejects a missing reference root with a useful diagnostic", async () => {
  await assert.rejects(
    extractManimReferenceExamples(path.join(os.tmpdir(), "noon-reference-root-that-does-not-exist")),
    /reference root does not exist/,
  );
});

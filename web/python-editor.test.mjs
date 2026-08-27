import assert from "node:assert/strict";
import test from "node:test";

import { PYTHON_RUFF_SETTINGS } from "./python-editor.js";

test("demo Ruff keeps useful Pyflakes coverage but ignores star-import compatibility noise", () => {
  assert.deepEqual(PYTHON_RUFF_SETTINGS.lint.select, ["E4", "E7", "E9", "F"]);
  assert.deepEqual(PYTHON_RUFF_SETTINGS.lint.ignore, ["F403", "F405"]);
  assert.equal(
    PYTHON_RUFF_SETTINGS.lint.ignore.includes("F821"),
    false,
    "undefined local names must remain lintable",
  );
  assert.equal(
    PYTHON_RUFF_SETTINGS.lint.select.includes("E9"),
    true,
    "syntax-error diagnostics must remain enabled",
  );
});

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  PINNED_PYODIDE_VERSION,
  PYODIDE_CDN_PREFIX,
  pinnedPyodideRelativePath,
} from "../src/preview-pyodide.mjs";

test("isolated preview pins the same Pyodide release as the authored worker", () => {
  assert.equal(PINNED_PYODIDE_VERSION, "314.0.5");
  assert.equal(PYODIDE_CDN_PREFIX, "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/");
});

test("only the pinned full-distribution URL maps into the image runtime", () => {
  assert.equal(pinnedPyodideRelativePath(`${PYODIDE_CDN_PREFIX}pyodide.mjs`), "pyodide.mjs");
  assert.equal(pinnedPyodideRelativePath(`${PYODIDE_CDN_PREFIX}python_stdlib.zip?cache=1`), "python_stdlib.zip");
  assert.equal(pinnedPyodideRelativePath("https://cdn.jsdelivr.net/pyodide/v314.0.6/full/pyodide.mjs"), null);
  assert.equal(pinnedPyodideRelativePath("https://example.com/pyodide/v314.0.5/full/pyodide.mjs"), null);
  assert.equal(pinnedPyodideRelativePath(`${PYODIDE_CDN_PREFIX}%2Fetc%2Fpasswd`), null);
  assert.equal(pinnedPyodideRelativePath(PYODIDE_CDN_PREFIX), null);
});

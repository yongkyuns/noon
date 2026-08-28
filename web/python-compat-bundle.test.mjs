import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";

// The coverage workflow runs web/*.test.mjs directly instead of through
// build-web-demo.sh. Generate the ignored artifact here as part of the test so
// this contract is deterministic and independent of test invocation order.
await import("../scripts/build-python-compat-bundle.mjs");

const worker = await readFile(new URL("./python-worker.js", import.meta.url), "utf8");
const generated = JSON.parse(
  await readFile(new URL("./python/compat-bundle.json", import.meta.url), "utf8"),
);

assert.equal(PYTHON_COMPAT_MODULES.length, 20, "compatibility manifest must cover all bootstrap modules");
assert.equal(new Set(PYTHON_COMPAT_MODULES.map((module) => module.sourcePath)).size, 20);
assert.equal(new Set(PYTHON_COMPAT_MODULES.map((module) => module.runtimePath)).size, 20);
assert.equal(generated.version, 1);
assert.equal(generated.modules.length, PYTHON_COMPAT_MODULES.length);

for (const [index, expected] of PYTHON_COMPAT_MODULES.entries()) {
  const actual = generated.modules[index];
  assert.equal(actual.runtimePath, expected.runtimePath, `${expected.sourcePath}: runtime path drift`);
  assert.equal(actual.label, expected.label, `${expected.sourcePath}: label drift`);
  assert.equal(
    actual.source,
    await readFile(new URL(`./${expected.sourcePath}`, import.meta.url), "utf8"),
    `${expected.sourcePath}: generated bundle is stale`,
  );
}

assert.match(
  worker,
  /fetch\(new URL\("\.\/python\/compat-bundle\.json", import\.meta\.url\)\)/,
  "Python worker must load the generated compatibility bundle",
);
assert.doesNotMatch(
  worker,
  /fetch\(new URL\("\.\/python\/(?!compat-bundle\.json)/,
  "Python worker must not fan out compatibility source requests",
);
assert.match(
  worker,
  /PYTHON_COMPAT_MODULES/,
  "Python worker must validate the generated bundle against the checked-in manifest",
);

console.log("✓ Python compatibility bootstrap uses one generated source bundle");

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const authoringClient = await readFile(
  new URL("./authoring-client.js", import.meta.url),
  "utf8",
);
const playgroundEntry = await readFile(new URL("./main.js", import.meta.url), "utf8");

assert.doesNotMatch(
  authoringClient,
  /python-editor\.js/,
  "runtime authoring client must not depend on presentation/editor enhancement",
);
assert.doesNotMatch(
  playgroundEntry,
  /^void import\(["']\.\/python-editor\.js["']\)/m,
  "playground startup must not eagerly load editor enhancement",
);
assert.match(
  playgroundEntry,
  /function loadEnhancedPythonEditor\(\)[\s\S]*?import\(["']\.\/python-editor\.js["']\)\.catch\(/,
  "playground entrypoint must own editor enhancement through a fail-soft lazy dynamic import",
);
assert.match(
  playgroundEntry,
  /sceneSourceEditor\.addEventListener\([\s\S]*?["']focus["'][\s\S]*?loadEnhancedPythonEditor\(\)[\s\S]*?\{ once: true \}/,
  "editor enhancement must begin only on the first source-editor focus",
);

console.log("✓ lazy editor enhancement stays outside the runtime authoring dependency graph");

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
assert.match(
  playgroundEntry,
  /void import\(["']\.\/python-editor\.js["']\)\.catch\(/,
  "playground entrypoint must own editor enhancement through a fail-soft dynamic import",
);

console.log("✓ editor enhancement is outside the runtime authoring dependency graph");

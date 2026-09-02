import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const authoringClient = await readFile(
  new URL("./authoring-client.js", import.meta.url),
  "utf8",
);
const playgroundEntry = await readFile(new URL("./main.js", import.meta.url), "utf8");
const editorBootstrap = await readFile(
  new URL("./python-editor-bootstrap.js", import.meta.url),
  "utf8",
);
const playgroundHtml = await readFile(new URL("./index.html", import.meta.url), "utf8");

assert.doesNotMatch(
  authoringClient,
  /python-editor\.js/,
  "runtime authoring client must not depend on presentation/editor enhancement",
);
assert.match(
  editorBootstrap,
  /void import\(["']\.\/python-editor\.js["']\)\.catch\(/,
  "presentation bootstrap must begin fail-soft editor enhancement without waiting for focus",
);
assert.match(
  playgroundHtml,
  /<script type="module" src="\.\/python-editor-bootstrap\.js"><\/script>[\s\S]*?<script type="module" src="\.\/main\.js"><\/script>/,
  "editor enhancement must start independently before the playground runtime entrypoint",
);
assert.match(
  playgroundEntry,
  /function loadEnhancedPythonEditor\(\)[\s\S]*?import\(["']\.\/python-editor\.js["']\)\.catch\(/,
  "playground entrypoint must retain a fail-soft focus fallback for editor enhancement",
);
assert.match(
  playgroundEntry,
  /sceneSourceEditor\.addEventListener\([\s\S]*?["']focus["'][\s\S]*?loadEnhancedPythonEditor\(\)[\s\S]*?\{ once: true \}/,
  "focus path must remain a fallback if bootstrap enhancement was delayed or failed",
);
assert.match(playgroundHtml, /\.workspace \{[\s\S]*?height: 38rem;[\s\S]*?min-height: 0;/);
assert.match(playgroundHtml, /\.editor-pane \{[\s\S]*?height: 100%;[\s\S]*?overflow: hidden;/);
assert.match(playgroundHtml, /\.editor-stack,[\s\S]*?\.editor-panel \{[\s\S]*?overflow: hidden;/);

console.log("✓ editor highlighting starts before focus while runtime startup stays decoupled and bounded");
